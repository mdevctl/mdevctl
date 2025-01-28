//! A filesystem environment for mdevctl

use crate::callouts::{callout, CalloutScriptCache, CalloutScriptInfo};
use crate::error::Error;
use crate::mdev::{MDev, MDevSysfsData, MDevType};
use log::{debug, warn};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{env, fs};
use uuid::Uuid;

/// An object which provides runtime environment settings and provides functions to
/// query the filesystem paths for certain system resources within that environment.
///
/// The main purpose of this object is to enable testability of the mdevctl commands by
/// abstracting out the filesystem locations. Tests can customize the root path and provide
/// filesystem paths within a mock filesystem environment that will not affect the system.
#[derive(Debug)]
pub struct Environment {
    rootpath: PathBuf,
    callout_scripts: Mutex<CalloutScriptCache>,
}

impl Environment {
    fn root(&self) -> &Path {
        self.rootpath.as_path()
    }

    pub fn mdev_base(&self) -> PathBuf {
        self.root().join("sys/bus/mdev/devices")
    }

    pub fn config_base(&self) -> PathBuf {
        self.root().join("etc/mdevctl.d")
    }

    pub fn parent_base(&self) -> PathBuf {
        self.root().join("sys/class/mdev_bus")
    }

    pub fn config_scripts_base(&self) -> PathBuf {
        self.config_base().join("scripts.d")
    }

    pub fn scripts_base(&self) -> PathBuf {
        self.root().join("usr/lib/mdevctl/scripts.d")
    }

    pub fn callout_dir(&self) -> PathBuf {
        self.scripts_base().join("callouts")
    }

    pub fn old_callout_dir(&self) -> PathBuf {
        self.config_scripts_base().join("callouts")
    }

    pub fn callout_dirs(&self) -> Vec<PathBuf> {
        vec![self.callout_dir(), self.old_callout_dir()]
    }

    pub fn notification_dir(&self) -> PathBuf {
        self.scripts_base().join("notifiers")
    }

    pub fn old_notification_dir(&self) -> PathBuf {
        self.config_scripts_base().join("notifiers")
    }

    pub fn notification_dirs(&self) -> Vec<PathBuf> {
        vec![self.notification_dir(), self.old_notification_dir()]
    }

    pub fn self_check(&self) -> Result<(), Error> {
        debug!("checking that the environment is sane");
        // ensure required system dirs exist. Generally distro packages or 'make install' should
        // create these dirs.
        for dir in [
            self.config_base(),
            self.callout_dir(),
            self.notification_dir(),
        ] {
            if !dir.exists() {
                return Err(Error::System(format!("Required directory {:?} doesn't exist. This may indicate a packaging or installation error", dir)));
            }
        }
        Ok(())
    }

    /// convenience function to lookup an active device by uuid and parent
    pub fn get_active_device(&self, uuid: Uuid, parent: Option<&String>) -> Result<MDev, Error> {
        let devs = self.get_active_devices(Some(&uuid), parent)?;
        if devs.is_empty() {
            Err(Error::DeviceState(
                "device is not active".to_string(),
                uuid,
                parent.cloned(),
            ))
        } else if devs.len() > 1 {
            Err(Error::System(format!(
                "Multiple parents found for {}",
                uuid
            )))
        } else {
            let (parent, children) = devs.iter().next().unwrap();
            if children.len() > 1 {
                return Err(Error::System(format!(
                    "Multiple definitions found for {}/{}",
                    parent, uuid
                )));
            }
            Ok(children.first().unwrap().clone())
        }
    }

    /// Get a map of all active devices, optionally filtered by uuid and parent
    pub fn get_active_devices(
        &self,
        uuid: Option<&Uuid>,
        parent: Option<&String>,
    ) -> Result<BTreeMap<String, Vec<MDev>>, Error> {
        let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
        debug!(
            "Looking up active mdevs: uuid={:?}, parent={:?}",
            uuid, parent
        );
        if let Ok(dir) = self.mdev_base().read_dir() {
            for dir_dev in dir {
                let dir_dev = dir_dev
                    .map_err(|e| Error::IOError("Failed to read directory entry".to_string(), e))?;
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

                let mut dev = MDev::new(self, u);
                if let Ok(sysfs_data) = MDevSysfsData::load_for_mdev(&dev) {
                    dev.set_sysfs_data(sysfs_data);
                    if dev.active {
                        if parent.is_some() && (parent != dev.parent.as_ref()) {
                            debug!(
                                "Ignoring device {} because it doesn't match parent {}",
                                dev.uuid,
                                parent.as_ref().unwrap()
                            );
                            continue;
                        }

                        // retrieve autostart from persisted mdev if possible
                        let mut per_dev = MDev::new(self, u);
                        per_dev.parent.clone_from(&dev.parent);
                        if per_dev.load_definition().is_ok() {
                            dev.autostart = per_dev.autostart;
                        }

                        // if the device is supported by a callout script that gets attributes, show
                        // those in the output
                        let mut c = callout(&mut dev)?;
                        if let Ok(attrs) = c.get_attributes() {
                            let _ = c.dev.add_attributes(&attrs);
                        }

                        let devparent = dev.parent()?;
                        if !devices.contains_key(devparent) {
                            devices.insert(devparent.clone(), Vec::new());
                        };

                        devices.get_mut(devparent).unwrap().push(dev);
                    };
                };
            }
        }
        Ok(devices)
    }

    /// Get a map of all defined devices, optionally filtered by uuid and parent
    pub fn get_defined_devices(
        &self,
        uuid: Option<&Uuid>,
        parent: Option<&String>,
    ) -> Result<BTreeMap<String, Vec<MDev>>, Error> {
        let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
        debug!(
            "Looking up defined mdevs: uuid={:?}, parent={:?}",
            uuid, parent
        );
        for parentpath in self
            .config_base()
            .read_dir()
            .map_err(|e| {
                Error::IOError(
                    "Failed to read persistent device configuration directory".to_string(),
                    e,
                )
            })?
            .skip_while(|x| match x {
                Ok(d) => d.path() == self.scripts_base(),
                _ => false,
            })
        {
            let parentpath = parentpath.map_err(|e| {
                Error::IOError(
                    "Failed to read entry from persistent device configuration directory"
                        .to_string(),
                    e,
                )
            })?;
            let parentname = parentpath.file_name();
            let parentname = parentname.to_str().unwrap();
            if (parent.is_some() && parent.unwrap() != parentname)
                || !parentpath
                    .metadata()
                    .map_err(|e| {
                        Error::IOError(format!("Failed to read metadata for {parentpath:?}"), e)
                    })?
                    .is_dir()
            {
                debug!("Ignoring child devices for parent {}", parentname);
                continue;
            }

            let mut childdevices = Vec::new();

            match parentpath.path().read_dir() {
                Ok(res) => {
                    for child in res {
                        let child = child.map_err(|e| {
                            Error::IOError(
                                format!("Failed to read entry from directory {parentpath:?}"),
                                e,
                            )
                        })?;
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
                                f.read_to_string(&mut contents).map_err(|e| {
                                    Error::IOError(
                                        format!("Failed to read contents of {path:?}"),
                                        e,
                                    )
                                })?;
                                let val = serde_json::from_str(&contents)?;
                                let mut dev = MDev::new(self, u);
                                dev.load_from_json(parentname.to_string(), &val)?;
                                match MDevSysfsData::load_for_mdev(&dev) {
                                    Ok(sysfs_data) => {
                                        if dev.sysfs_data_matches(&sysfs_data) {
                                            dev.set_sysfs_data(sysfs_data);
                                        }
                                    }
                                    Err(Error::DeviceNotFound) => (),
                                    Err(e) => warn!(
                                        "For device {} a sysfs update caused the error: {:?}",
                                        u, e
                                    ),
                                };
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
    pub fn get_defined_device(&self, uuid: Uuid, parent: Option<&String>) -> Result<MDev, Error> {
        let devs = self.get_defined_devices(Some(&uuid), parent)?;
        if devs.is_empty() {
            Err(Error::DeviceState(
                "device is not defined".to_string(),
                uuid,
                parent.cloned(),
            ))
        } else if devs.len() > 1 {
            Err(Error::DeviceState(
                match parent {
                    None => "Multiple definitions found, specify a parent",
                    Some(_) => "Multiple definitions found",
                }
                .to_string(),
                uuid,
                parent.cloned(),
            ))
        } else {
            let (parent, children) = devs.iter().next().unwrap();
            if children.len() > 1 {
                return Err(Error::DeviceState(
                    "Multiple definitions found".to_string(),
                    uuid,
                    Some(parent.clone()),
                ));
            }
            Ok(children.first().unwrap().clone())
        }
    }

    /// Get a map of all mediated device types that are supported on this machine
    pub fn get_supported_types(
        &self,
        parent: Option<String>,
    ) -> Result<BTreeMap<String, Vec<MDevType>>, Error> {
        debug!("Finding supported mdev types");
        let mut types: BTreeMap<String, Vec<MDevType>> = BTreeMap::new();

        if let Ok(dir) = self.parent_base().read_dir() {
            for parentpath in dir {
                let parentpath = parentpath.map_err(|e| {
                    Error::IOError(
                        "Failed to read entry from mdev parent device directory".to_string(),
                        e,
                    )
                })?;
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
                for child in parentpath.read_dir().map_err(|e| {
                    Error::IOError(format!("Failed to read directory {parentpath:?}"), e)
                })? {
                    let child = child.map_err(|e| {
                        Error::IOError(
                            format!("Failed to read entry from directory {parentpath:?}"),
                            e,
                        )
                    })?;
                    if !child
                        .metadata()
                        .map_err(|e| {
                            Error::IOError(
                                format!("Unable to determine file type for {child:?}"),
                                e,
                            )
                        })?
                        .is_dir()
                    {
                        continue;
                    }

                    let mut t = MDevType::new();
                    t.parent = parentname.to_string();

                    let mut path = child.path();
                    t.typename = path.file_name().unwrap().to_str().unwrap().to_string();
                    debug!("found mdev type {}", t.typename);

                    path.push("available_instances");
                    debug!("Checking available instances: {:?}", path);
                    t.available_instances = file_contents(&path)?.trim().parse().map_err(|e| {
                        Error::System(format!(
                            "Failed to parse available instances as a string: {e}"
                        ))
                    })?;

                    path.pop();
                    path.push("device_api");
                    t.device_api = file_contents(&path)?.trim().to_string();

                    path.pop();
                    path.push("name");
                    if path.exists() {
                        t.name = file_contents(&path)?.trim().to_string();
                    }

                    path.pop();
                    path.push("description");
                    if path.exists() {
                        t.description =
                            file_contents(&path)?.trim().replace('\n', ", ").to_string();
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

    pub fn find_script(&self, dev: &mut MDev) -> Option<CalloutScriptInfo> {
        return self
            .callout_scripts
            .lock()
            .unwrap()
            .find_versioned_script(dev);
    }

    pub fn new(root: String) -> Self {
        let root = match env::var("MDEVCTL_ENV_ROOT") {
            Ok(d) => d,
            _ => root.to_string(),
        };
        Environment {
            rootpath: PathBuf::from(env::var("MDEVCTL_ENV_ROOT").unwrap_or(root)),
            callout_scripts: Mutex::new(CalloutScriptCache::new()),
        }
    }
}

fn file_contents(path: &PathBuf) -> Result<String, Error> {
    fs::read_to_string(path)
        .map_err(|e| Error::IOError(format!("Failed to read contents of {path:?}"), e))
}
