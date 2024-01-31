//! A filesystem environment for mdevctl

use crate::callouts::{callout, CalloutScriptCache, CalloutScriptInfo};
use crate::mdev::{MDev, MDevType};
use anyhow::{anyhow, Result};
use log::{debug, warn};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::{env, fs};
use uuid::Uuid;

/// A trait which provides filesystem paths for certain system resources and provides functions to
/// query the state of that environment.
///
/// The main purpose that this is a trait is to enable testability of the mdevctl commands by
/// abstracting out the filesystem locations. Tests can implement [`Environment`] and provide
/// filesystem paths within a mock filesystem environment that will not affect the system.
pub trait Environment: std::fmt::Debug {
    fn root(&self) -> &Path;

    fn find_script(&self, dev: &MDev) -> Option<CalloutScriptInfo>;

    fn as_env(self: Rc<Self>) -> Rc<dyn Environment>;

    fn mdev_base(&self) -> PathBuf {
        self.root().join("sys/bus/mdev/devices")
    }

    fn config_base(&self) -> PathBuf {
        self.root().join("etc/mdevctl.d")
    }

    fn parent_base(&self) -> PathBuf {
        self.root().join("sys/class/mdev_bus")
    }

    fn config_scripts_base(&self) -> PathBuf {
        self.config_base().join("scripts.d")
    }

    fn scripts_base(&self) -> PathBuf;

    fn callout_dir(&self) -> PathBuf {
        self.scripts_base().join("callouts")
    }

    fn old_callout_dir(&self) -> PathBuf {
        self.config_scripts_base().join("callouts")
    }

    fn callout_dirs(&self) -> Vec<PathBuf> {
        vec![self.callout_dir(), self.old_callout_dir()]
    }

    fn notification_dir(&self) -> PathBuf {
        self.scripts_base().join("notifiers")
    }

    fn old_notification_dir(&self) -> PathBuf {
        self.config_scripts_base().join("notifiers")
    }

    fn notification_dirs(&self) -> Vec<PathBuf> {
        vec![self.notification_dir(), self.old_notification_dir()]
    }

    fn self_check(&self) -> Result<()> {
        debug!("checking that the environment is sane");
        // ensure required system dirs exist. Generally distro packages or 'make install' should
        // create these dirs.
        for dir in [
            self.config_base(),
            self.callout_dir(),
            self.notification_dir(),
        ] {
            if !dir.exists() {
                return Err(anyhow!("Required directory {:?} doesn't exist. This may indicate a packaging or installation error", dir));
            }
        }
        Ok(())
    }

    /// convenience function to lookup an active device by uuid and parent
    fn get_active_device(self: Rc<Self>, uuid: Uuid, parent: Option<&String>) -> Result<MDev> {
        let devs = self.get_active_devices(Some(&uuid), parent)?;
        if devs.is_empty() {
            match parent {
                None => Err(anyhow!(
                    "Mediated device {} is not active",
                    uuid.hyphenated().to_string()
                )),
                Some(p) => Err(anyhow!(
                    "Mediated device {}/{} is not active",
                    p,
                    uuid.hyphenated().to_string()
                )),
            }
        } else if devs.len() > 1 {
            Err(anyhow!(
                "Multiple parents found for {}. System error?",
                uuid.hyphenated().to_string()
            ))
        } else {
            let (parent, children) = devs.iter().next().unwrap();
            if children.len() > 1 {
                return Err(anyhow!(
                    "Multiple definitions found for {}/{}",
                    parent,
                    uuid.hyphenated().to_string()
                ));
            }
            Ok(children.first().unwrap().clone())
        }
    }

    /// Get a map of all active devices, optionally filtered by uuid and parent
    fn get_active_devices(
        self: Rc<Self>,
        uuid: Option<&Uuid>,
        parent: Option<&String>,
    ) -> Result<BTreeMap<String, Vec<MDev>>> {
        let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
        debug!(
            "Looking up active mdevs: uuid={:?}, parent={:?}",
            uuid, parent
        );
        if let Ok(dir) = self.mdev_base().read_dir() {
            for dir_dev in dir {
                let dir_dev = dir_dev?;
                let fname = dir_dev.file_name();
                let basename = fname.to_str().unwrap();
                debug!("found defined mdev {}", basename);
                let u = Uuid::parse_str(basename);

                if u.is_err() {
                    warn!("Can't determine uuid for file '{}'", basename);
                    continue;
                }
                let u = u.unwrap();

                if uuid.is_some() && uuid != Some(&u) {
                    debug!(
                        "Ignoring device {} because it doesn't match uuid {}",
                        u,
                        uuid.unwrap()
                    );
                    continue;
                }

                let mut dev = MDev::new(self.clone().as_env(), u);
                if dev.load_from_sysfs().is_ok() {
                    if parent.is_some() && (parent != dev.parent.as_ref()) {
                        debug!(
                            "Ignoring device {} because it doesn't match parent {}",
                            dev.uuid,
                            parent.as_ref().unwrap()
                        );
                        continue;
                    }

                    // retrieve autostart from persisted mdev if possible
                    let mut per_dev = MDev::new(self.clone().as_env(), u);
                    per_dev.parent = dev.parent.clone();
                    if per_dev.load_definition().is_ok() {
                        dev.autostart = per_dev.autostart;
                    }

                    // if the device is supported by a callout script that gets attributes, show
                    // those in the output
                    let mut c = callout(&mut dev);
                    if let Ok(attrs) = c.get_attributes() {
                        let _ = c.dev.add_attributes(&attrs);
                    }

                    let devparent = dev.parent()?;
                    if !devices.contains_key(devparent) {
                        devices.insert(devparent.clone(), Vec::new());
                    };

                    devices.get_mut(devparent).unwrap().push(dev);
                };
            }
        }
        Ok(devices)
    }

    /// Get a map of all defined devices, optionally filtered by uuid and parent
    fn get_defined_devices(
        self: Rc<Self>,
        uuid: Option<&Uuid>,
        parent: Option<&String>,
    ) -> Result<BTreeMap<String, Vec<MDev>>> {
        let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
        debug!(
            "Looking up defined mdevs: uuid={:?}, parent={:?}",
            uuid, parent
        );
        let thisenv = self.as_env();
        for parentpath in thisenv.config_base().read_dir()?.skip_while(|x| match x {
            Ok(d) => d.path() == thisenv.scripts_base(),
            _ => false,
        }) {
            let parentpath = parentpath?;
            let parentname = parentpath.file_name();
            let parentname = parentname.to_str().unwrap();
            if (parent.is_some() && parent.unwrap() != parentname)
                || !parentpath.metadata()?.is_dir()
            {
                debug!("Ignoring child devices for parent {}", parentname);
                continue;
            }

            let mut childdevices = Vec::new();

            match parentpath.path().read_dir() {
                Ok(res) => {
                    for child in res {
                        let child = child?;
                        match child.metadata() {
                            Ok(metadata) => {
                                if !metadata.is_file() {
                                    continue;
                                }
                            }
                            Err(e) => {
                                warn!("unable to access file {:?}: {}", child.path(), e);
                                continue;
                            }
                        }

                        let path = child.path();
                        let basename = path.file_name().unwrap().to_str().unwrap();
                        let u = Uuid::parse_str(basename);
                        if u.is_err() {
                            warn!("Can't determine uuid for file '{}'", basename);
                            continue;
                        }
                        let u = u.unwrap();

                        debug!("found mdev {:?}", u);
                        if uuid.is_some() && uuid != Some(&u) {
                            debug!(
                                "Ignoring device {} because it doesn't match uuid {}",
                                u,
                                uuid.unwrap()
                            );
                            continue;
                        }

                        match fs::File::open(&path) {
                            Ok(mut f) => {
                                let mut contents = String::new();
                                f.read_to_string(&mut contents)?;
                                let val = serde_json::from_str(&contents)?;
                                let mut dev = MDev::new(thisenv.clone(), u);
                                dev.load_from_json(parentname.to_string(), &val)?;
                                dev.load_from_sysfs()?;

                                childdevices.push(dev);
                            }
                            Err(e) => {
                                warn!("Unable to open file {:?}: {}", path, e);
                                continue;
                            }
                        };
                    }
                }
                Err(e) => warn!("Unable to read directory {:?}: {}", parentpath.path(), e),
            }
            if !childdevices.is_empty() {
                devices.insert(parentname.to_string(), childdevices);
            }
        }
        Ok(devices)
    }

    /// convenience function to lookup a defined device by uuid and parent
    fn get_defined_device(self: Rc<Self>, uuid: Uuid, parent: Option<&String>) -> Result<MDev> {
        let devs = self.get_defined_devices(Some(&uuid), parent)?;
        if devs.is_empty() {
            match parent {
                None => Err(anyhow!(
                    "Mediated device {} is not defined",
                    uuid.hyphenated().to_string()
                )),
                Some(p) => Err(anyhow!(
                    "Mediated device {}/{} is not defined",
                    p,
                    uuid.hyphenated().to_string()
                )),
            }
        } else if devs.len() > 1 {
            match parent {
                None => Err(anyhow!(
                    "Multiple definitions found for {}, specify a parent",
                    uuid.hyphenated().to_string()
                )),
                Some(p) => Err(anyhow!(
                    "Multiple definitions found for {}/{}",
                    p,
                    uuid.hyphenated().to_string()
                )),
            }
        } else {
            let (parent, children) = devs.iter().next().unwrap();
            if children.len() > 1 {
                return Err(anyhow!(
                    "Multiple definitions found for {}/{}",
                    parent,
                    uuid.hyphenated().to_string()
                ));
            }
            Ok(children.first().unwrap().clone())
        }
    }

    /// Get a map of all mediated device types that are supported on this machine
    fn get_supported_types(
        self: Rc<Self>,
        parent: Option<String>,
    ) -> Result<BTreeMap<String, Vec<MDevType>>> {
        debug!("Finding supported mdev types");
        let mut types: BTreeMap<String, Vec<MDevType>> = BTreeMap::new();

        if let Ok(dir) = self.parent_base().read_dir() {
            for parentpath in dir {
                let parentpath = parentpath?;
                let parentname = parentpath.file_name();
                let parentname = parentname.to_str().unwrap();
                debug!("Looking for supported types for device {}", parentname);
                if parent.is_some() && parent.as_ref().unwrap() != parentname {
                    debug!("Ignoring types for parent {}", parentname);
                    continue;
                }

                let mut childtypes = Vec::new();
                let mut parentpath = parentpath.path();
                parentpath.push("mdev_supported_types");
                for child in parentpath.read_dir()? {
                    let child = child?;
                    if !child.metadata()?.is_dir() {
                        continue;
                    }

                    let mut t = MDevType::new();
                    t.parent = parentname.to_string();

                    let mut path = child.path();
                    t.typename = path.file_name().unwrap().to_str().unwrap().to_string();
                    debug!("found mdev type {}", t.typename);

                    path.push("available_instances");
                    debug!("Checking available instances: {:?}", path);
                    t.available_instances = fs::read_to_string(&path)?.trim().parse()?;

                    path.pop();
                    path.push("device_api");
                    t.device_api = fs::read_to_string(&path)?.trim().to_string();

                    path.pop();
                    path.push("name");
                    if path.exists() {
                        t.name = fs::read_to_string(&path)?.trim().to_string();
                    }

                    path.pop();
                    path.push("description");
                    if path.exists() {
                        t.description = fs::read_to_string(&path)?
                            .trim()
                            .replace('\n', ", ")
                            .to_string();
                    }

                    childtypes.push(t);
                }
                types.insert(parentname.to_string(), childtypes);
            }
        }
        for v in types.values_mut() {
            v.sort_by(|a, b| a.typename.cmp(&b.typename));
        }
        Ok(types)
    }
}

/// A default implementation of the Environment trait which uses '/' as the filesystem root.
#[derive(Debug)]
pub struct DefaultEnvironment {
    rootpath: PathBuf,
    callout_scripts: Mutex<CalloutScriptCache>,
}

impl Environment for DefaultEnvironment {
    fn root(&self) -> &Path {
        self.rootpath.as_path()
    }

    fn find_script(&self, dev: &MDev) -> Option<CalloutScriptInfo> {
        return self
            .callout_scripts
            .lock()
            .unwrap()
            .find_versioned_script(dev);
    }

    fn as_env(self: Rc<Self>) -> Rc<dyn Environment> {
        self.clone()
    }

    fn scripts_base(&self) -> PathBuf {
        PathBuf::from(env!(
            "MDEVCTL_SCRIPTDIR",
            "MDEVCTL_SCRIPTDIR environment variable not defined"
        ))
    }
}

impl DefaultEnvironment {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Rc<dyn Environment> {
        let root = match env::var("MDEVCTL_ENV_ROOT") {
            Ok(d) => d,
            _ => "/".to_string(),
        };
        Rc::new(DefaultEnvironment {
            rootpath: PathBuf::from(root),
            callout_scripts: Mutex::new(CalloutScriptCache::new()),
        })
    }
}
