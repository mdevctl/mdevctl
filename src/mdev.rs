//! Structures for representing a mediated device

use crate::environment::Environment;
use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::fs;
use std::io::{Error as ioError, ErrorKind, Read, Result as ioResult};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::vec::Vec;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum FormatType {
    Active,
    Defined,
}

pub struct MDevSysfsData {
    pub uuid: Uuid,
    pub active: bool,
    pub parent: Option<String>,
    pub mdev_type: Option<String>,
}

impl MDevSysfsData {
    pub fn load(env: &Rc<dyn Environment>, uuid: &Uuid) -> Result<MDevSysfsData> {
        let mut mdev_type: Option<String> = None;
        let mut active = true;
        let active_path = Self::active_path(env.clone(), uuid);
        let mut parent = match Self::load_parent_from_sysfs(&active_path) {
            Ok(parentname) => Some(parentname),
            Err(e) => match e.kind() {
                ErrorKind::NotFound => {
                    debug!("Mdev {:?} does not exist in sysfs", uuid);
                    active = false;
                    None
                }
                _ => return Err(e.into()),
            },
        };
        if active {
            mdev_type = match Self::load_mdev_type_from_sysfs(&active_path) {
                Ok(mdev_type) => Some(mdev_type),
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            debug!("Mdev {:?} does no longer exist in sysfs", uuid);
                            parent = None; // remove invalid data
                            active = false;
                            None
                        }
                        _ => return Err(e.into()),
                    }
                }
            };
        }
        Ok(MDevSysfsData {
            uuid: uuid.to_owned(),
            active,
            parent,
            mdev_type,
        })
    }

    pub fn load_with_mdev(mdev: &MDev) -> Result<MDevSysfsData> {
        Self::load(&mdev.env, &mdev.uuid)
    }

    fn active_path(env: Rc<dyn Environment>, uuid: &Uuid) -> PathBuf {
        env.mdev_base().join(uuid.hyphenated().to_string())
    }

    fn load_parent_from_sysfs<P: AsRef<Path>>(active_path: P) -> ioResult<String> {
        let canonpath = fs::canonicalize(&active_path)?;
        let sysfsparent = match canonpath.parent() {
            Some(sysfsparent) => sysfsparent,
            None => {
                return Err(ioError::new(
                    ErrorKind::Other,
                    format!("Path to parent of mdev {:?} does not exist", canonpath),
                ))
            }
        };
        Self::canonical_basename(sysfsparent)
    }

    fn load_mdev_type_from_sysfs<P: Into<PathBuf>>(active_path: P) -> ioResult<String> {
        let mut typepath: PathBuf = active_path.into();
        typepath.push("mdev_type");
        Self::canonical_basename(typepath)
    }

    fn canonical_basename<P: AsRef<Path>>(path: P) -> ioResult<String> {
        let path = fs::canonicalize(path)?;
        let fname = match path.file_name() {
            Some(fname) => fname,
            None => return Err(ioError::new(ErrorKind::Other, "Invalid path")),
        };
        match fname.to_str() {
            Some(x) => Ok(x.to_string()),
            None => Err(ioError::new(ErrorKind::Other, "Invalid file name")),
        }
    }
}

/// Representation of a mediated device
#[derive(Debug, Clone)]
pub struct MDev {
    pub uuid: Uuid,
    pub active: bool,
    pub autostart: bool,
    pub parent: Option<String>,
    pub mdev_type: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub env: Rc<dyn Environment>,
}

impl MDev {
    pub fn new(env: Rc<dyn Environment>, uuid: Uuid) -> MDev {
        MDev {
            uuid,
            active: false,
            autostart: false,
            parent: None,
            mdev_type: None,
            attrs: Vec::new(),
            env,
        }
    }

    pub fn new_from_jsonfile(
        env: Rc<dyn Environment>,
        uuid: Uuid,
        parent: String,
        jsonfile: PathBuf,
    ) -> Result<Self> {
        let _ = std::fs::File::open(&jsonfile)
            .with_context(|| format!("Unable to read file {:?}", jsonfile))?;
        let filecontents = fs::read_to_string(&jsonfile)
            .with_context(|| format!("Unable to read jsonfile {:?}", jsonfile))?;
        let jsonval = serde_json::from_str(&filecontents)?;

        let mut d = MDev::new(env, uuid);
        d.load_from_json(parent, &jsonval)?;
        Ok(d)
    }

    pub fn active_path(&self) -> PathBuf {
        let mut p = self.env.mdev_base();
        p.push(self.uuid.hyphenated().to_string());
        p
    }

    // get parent and propagate a consistent error to the caller if absent
    pub fn parent(&self) -> Result<&String> {
        self.parent.as_ref().ok_or_else(|| {
            anyhow!(
                "Device {} is missing a parent",
                self.uuid.hyphenated().to_string()
            )
        })
    }

    // get mdev_type and propagate a consistent error to the caller if absent
    pub fn mdev_type(&self) -> Result<&String> {
        self.mdev_type.as_ref().ok_or_else(|| {
            anyhow!(
                "Device {} is missing a mdev_type",
                self.uuid.hyphenated().to_string()
            )
        })
    }

    pub fn persistent_path(&self) -> Option<PathBuf> {
        self.parent.as_ref().map(|x| {
            let mut path = self.env.config_base();
            path.push(x);
            path.push(self.uuid.hyphenated().to_string());
            path
        })
    }

    pub fn is_defined(&self) -> bool {
        match self.persistent_path() {
            Some(p) => p.exists(),
            None => false,
        }
    }

    pub fn set_sysfs_data(&mut self, sysfs_data: MDevSysfsData) {
        self.active = sysfs_data.active;
        self.parent = sysfs_data.parent;
        self.mdev_type = sysfs_data.mdev_type;
    }

    pub fn is_sysfs_data_matching(&self, sysfs_data: &MDevSysfsData) -> bool {
        if self.parent.is_some() && self.parent != sysfs_data.parent {
            debug!(
                "Active mdev {:?} has different parent: {}!={}. No match.",
                self.uuid,
                self.parent.as_ref().unwrap(),
                sysfs_data.parent.as_ref().unwrap()
            );
            return false;
        }

        if self.mdev_type.is_some() && self.mdev_type != sysfs_data.mdev_type {
            debug!(
                "Active mdev {:?} has different type: {}!={}. No match.",
                self.uuid,
                self.mdev_type.as_ref().unwrap(),
                sysfs_data.mdev_type.as_ref().unwrap()
            );
            return false;
        }
        true
    }

    pub fn add_attributes(&mut self, attrs: &serde_json::Value) -> Result<()> {
        if !attrs.is_array() && !attrs.is_null() {
            return Err(anyhow!("attributes field is not an array"));
        }

        if let Some(attrarray) = attrs.as_array() {
            if !attrarray.is_empty() {
                for attr in attrarray {
                    let attrobj = attr.as_object().ok_or_else(|| {
                        anyhow!("invalid JSON format for attribute: not an object")
                    })?;
                    // attributes are represented by JSON objects with a single field.
                    if attrobj.len() != 1 {
                        return Err(anyhow!(
                            "invalid JSON format for attribute: too many fields"
                        ));
                    }
                    // get the key and value from the first (only) map entry
                    if let Some((key, val)) = attrobj.iter().next() {
                        let valstr = val.as_str().ok_or_else(|| {
                            anyhow!("invalid JSON format for attribute {{{:?}, {}}}: value must be of type str", key, val)
                        })?;
                        self.attrs.push((key.to_string(), valstr.to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn load_from_json(&mut self, parent: String, json: &serde_json::Value) -> Result<()> {
        debug!(
            "Loading device '{:?}' from json (parent: {})",
            self.uuid, parent
        );
        if self.parent.is_some() && self.parent.as_ref() != Some(&parent) {
            warn!(
                "Overwriting parent for mdev {:?}: {} => {}",
                self.uuid,
                self.parent.as_ref().unwrap(),
                parent
            );
        }
        self.parent = Some(parent);
        if json["mdev_type"].is_null() || json["start"].is_null() {
            return Err(anyhow!("invalid json"));
        }
        let mdev_type = json["mdev_type"].as_str().unwrap().to_string();
        if self.mdev_type.is_some() && self.mdev_type.as_ref() != Some(&mdev_type) {
            warn!(
                "Overwriting mdev type for mdev {:?}: {} => {}",
                self.uuid,
                self.mdev_type.as_ref().unwrap(),
                mdev_type
            );
        }
        self.mdev_type = Some(mdev_type);
        let startval = json["start"].as_str();
        self.autostart = matches!(startval, Some("auto"));

        self.add_attributes(&json["attrs"])?;
        debug!("loaded device {:?}", self);

        Ok(())
    }

    // load the stored definition from disk if it exists
    pub fn load_definition(&mut self) -> Result<()> {
        if let Some(path) = self.persistent_path() {
            let mut f = fs::File::open(path)?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)?;
            let val = serde_json::from_str(&contents)?;
            let parent = self.parent.as_ref().unwrap().clone();
            self.load_from_json(parent, &val)?;
        }
        Ok(())
    }

    pub fn to_text(&self, fmt: FormatType, verbose: bool) -> Result<String> {
        match fmt {
            FormatType::Defined => {
                if !self.is_defined() {
                    return Err(anyhow!("Device is not defined"));
                }
            }
            FormatType::Active => {
                if !self.active {
                    return Err(anyhow!("Device is not active"));
                }
            }
        }

        let mut output = self.uuid.hyphenated().to_string();
        output.push(' ');
        output.push_str(self.parent()?);
        output.push(' ');
        output.push_str(self.mdev_type()?);
        output.push(' ');
        output.push_str(match self.autostart {
            true => "auto",
            false => "manual",
        });

        match fmt {
            FormatType::Defined => {
                if self.active {
                    output.push_str(" (active)");
                }
            }
            FormatType::Active => {
                if self.is_defined() {
                    output.push_str(" (defined)");
                }
            }
        }

        output.push('\n');
        if verbose {
            let attr_string = self.fmt_attrs();
            output.push_str(&attr_string);
        }
        Ok(output)
    }

    fn fmt_attrs(&self) -> String {
        let mut output = String::new();
        if !self.attrs.is_empty() {
            output.push_str("  Attrs:\n");
            for (i, (key, value)) in self.attrs.iter().enumerate() {
                let txtattr = format!("    @{{{}}}: {{\"{}\":\"{}\"}}\n", i, key, value);
                output.push_str(&txtattr);
            }
        }
        output
    }

    pub fn to_json(&self, include_uuid: bool) -> Result<serde_json::Value> {
        let autostart = match self.autostart {
            true => "auto",
            false => "manual",
        };
        let mut partial = serde_json::Map::new();
        partial.insert("mdev_type".to_string(), self.mdev_type()?.clone().into());
        partial.insert("start".to_string(), autostart.into());
        let jsonattrs: Vec<_> = self
            .attrs
            .iter()
            .map(|(key, value)| serde_json::json!({ key: value }))
            .collect();
        partial.insert("attrs".to_string(), jsonattrs.into());

        let full = serde_json::json!({ self.uuid.hyphenated().to_string(): partial });

        match include_uuid {
            true => Ok(full),
            false => Ok(partial.into()),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        debug!("Removing mdev {:?}", self.uuid);
        let mut remove_path = self.active_path();
        remove_path.push("remove");
        debug!("remove path '{:?}'", remove_path);
        match fs::write(remove_path, "1") {
            Ok(_) => {
                self.active = false;
                Ok(())
            }
            Err(e) => Err(e).with_context(|| format!("Error removing device {:?}", self.uuid)),
        }
    }

    fn find_parent_dir(&self) -> Result<PathBuf> {
        let parent = self.parent()?;
        let path: PathBuf = self.env.parent_base().join(parent);

        if path.is_dir() {
            return Ok(path);
        }

        // check if there's a similar parent dir with different capitalization
        let parentsdir = self.env.parent_base().read_dir()?;
        for subdir in parentsdir {
            let dir = subdir?;
            let parentname = dir.file_name();
            if parentname.to_string_lossy().to_lowercase() == parent.to_lowercase() {
                return Err(anyhow!(
                    "Unable to find parent device '{}'. Did you mean '{}'?",
                    parent,
                    parentname.to_string_lossy()
                ));
            }
        }
        Err(anyhow!("Unable to find parent device '{}'", parent))
    }

    fn create(&mut self) -> Result<()> {
        debug!("Creating mdev {:?}", self.uuid);
        let parent = self.parent()?;
        let mdev_type = self.mdev_type()?;
        match MDevSysfsData::load(&self.env, &self.uuid) {
            Ok(mdev_sysfs_data) => {
                if mdev_sysfs_data.active {
                    if mdev_sysfs_data.parent != self.parent {
                        return Err(anyhow!("Device exists under different parent"));
                    }
                    if mdev_sysfs_data.mdev_type != self.mdev_type {
                        return Err(anyhow!("Device exists with different type"));
                    }
                    return Err(anyhow!("Device already exists"));
                }
            }
            Err(e) => {
                warn!(
                    "A sysfs lookup for device {} caused the error: {:?}",
                    self.uuid, e
                );
            }
        }

        let mut path = self.find_parent_dir()?;
        path.push("mdev_supported_types");
        debug!("Checking parent for mdev support: {:?}", path);
        if !path.is_dir() {
            return Err(anyhow!(
                "Parent {} is not currently registered for mdev support",
                parent
            ));
        }
        path.push(mdev_type);
        debug!("Checking parent for mdev type {}: {:?}", mdev_type, path);
        if !path.is_dir() {
            return Err(anyhow!(
                "Parent {} does not support mdev type {}",
                parent,
                mdev_type
            ));
        }
        path.push("available_instances");
        debug!("Checking available instances: {:?}", path);
        let avail: i32 = fs::read_to_string(&path)?.trim().parse()?;

        debug!("Available instances: {}", avail);
        if avail == 0 {
            return Err(anyhow!(
                "No available instances of {} on {}",
                mdev_type,
                parent
            ));
        }
        path.pop();
        path.push("create");
        debug!("Creating mediated device: {:?} -> {:?}", self.uuid, path);
        match fs::write(path, self.uuid.hyphenated().to_string()) {
            Ok(_) => {
                self.active = true;
                Ok(())
            }
            Err(e) => Err(e).with_context(|| {
                format!(
                    "Failed to create mdev {}, type {} on {}",
                    self.uuid.hyphenated(),
                    mdev_type,
                    parent
                )
            }),
        }
    }

    pub fn start(&mut self) -> Result<()> {
        self.create()?;

        debug!("Setting attributes for mdev {:?}", self.uuid);
        for (k, v) in self.attrs.iter() {
            if let Err(e) = write_attr(&self.active_path(), k, v) {
                self.stop()?;
                return Err(e);
            }
        }

        Ok(())
    }

    pub fn write_config(&self) -> Result<()> {
        let jsonstring = serde_json::to_string_pretty(&self.to_json(false)?)?;
        let path = self.persistent_path().unwrap();
        let parentdir = path.parent().unwrap();
        debug!("Ensuring parent directory {:?} exists", parentdir);
        fs::create_dir_all(parentdir)?;
        debug!("Writing config for {:?} to {:?}", self.uuid, path);
        fs::write(path, jsonstring.as_bytes())
            .with_context(|| format!("Failed to write config for device {:?}", self.uuid))
    }

    pub fn define(&self) -> Result<()> {
        self.write_config()
    }

    pub fn undefine(&mut self) -> Result<()> {
        let p = self
            .persistent_path()
            .ok_or_else(|| anyhow!("Failed to undefine {}", self.uuid.hyphenated().to_string()))?;

        fs::remove_file(&p).with_context(|| format!("Failed to remove file {:?}", p))?;
        Ok(())
    }

    fn attribute_hint(&self) -> String {
        match self.attrs.is_empty() {
            true => format!("Device {} has no attributes", self.uuid.hyphenated()),
            false => self.fmt_attrs(),
        }
    }

    pub fn add_attribute(
        &mut self,
        name: String,
        value: String,
        index: Option<usize>,
    ) -> Result<()> {
        match index {
            Some(i) => {
                if i > self.attrs.len() {
                    return Err(anyhow!(
                        "Attribute index {} is invalid\n{}",
                        i,
                        self.attribute_hint()
                    ));
                }
                self.attrs.insert(i, (name, value));
            }
            None => self.attrs.push((name, value)),
        }

        Ok(())
    }

    pub fn delete_attribute(&mut self, index: Option<usize>) -> Result<()> {
        match index {
            Some(i) => {
                if i >= self.attrs.len() {
                    return Err(anyhow!(
                        "Attribute index {} is invalid\n{}",
                        i,
                        self.attribute_hint()
                    ));
                }
                self.attrs.remove(i);
            }
            None => {
                self.attrs.pop();
            }
        }

        Ok(())
    }
}

fn write_attr(basepath: &Path, attr: &str, val: &str) -> Result<()> {
    debug!("Writing attribute '{}' -> '{}'", attr, val);
    let path = basepath.join(attr);
    if !path.exists() {
        return Err(anyhow!("Invalid attribute '{}'", attr));
    }
    fs::write(path, val).with_context(|| format!("Failed to write {} to attribute {}", val, attr))
}

/// Representation of a mediated device type
#[derive(Debug, Clone)]
pub struct MDevType {
    pub parent: String,
    pub typename: String,
    pub available_instances: i32,
    pub device_api: String,
    pub name: String,
    pub description: String,
}

impl MDevType {
    pub fn new() -> MDevType {
        MDevType {
            parent: String::new(),
            typename: String::new(),
            available_instances: 0,
            device_api: String::new(),
            name: String::new(),
            description: String::new(),
        }
    }

    pub fn to_json(&self) -> Result<serde_json::Value> {
        let mut jsonobj = serde_json::json!({
            "available_instances": self.available_instances,
            "device_api": self.device_api,
        });
        if !self.name.is_empty() {
            jsonobj.as_object_mut().unwrap().insert(
                "name".to_string(),
                serde_json::Value::String(self.name.clone()),
            );
        }
        if !self.description.is_empty() {
            jsonobj.as_object_mut().unwrap().insert(
                "description".to_string(),
                serde_json::Value::String(self.description.clone()),
            );
        }

        Ok(serde_json::json!({ &self.typename: jsonobj }))
    }
}
