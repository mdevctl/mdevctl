//! Structures for representing a mediated device

use crate::environment::Environment;
use crate::error::Error;
use log::{debug, warn};
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::vec::Vec;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum FormatType {
    Active,
    Defined,
}

pub struct MDevSysfsData {
    pub uuid: Uuid,
    pub parent: String,
    pub mdev_type: String,
}

impl MDevSysfsData {
    pub fn load(env: &Environment, uuid: &Uuid) -> Result<MDevSysfsData, Error> {
        let active_path = Self::active_path(env, uuid);
        let parent = Self::load_parent_from_sysfs(&active_path)
            .map(Some)
            .or_else(|e| match e {
                Error::IOError(_, source) if source.kind() == ErrorKind::NotFound => {
                    debug!("Mdev {uuid:?} does no longer exist in sysfs");
                    Ok(None)
                }
                _ => Err(e),
            })?;
        let mdev_type = Self::load_mdev_type_from_sysfs(&active_path)
            .map(Some)
            .or_else(|e| match e {
                Error::IOError(_, source) if source.kind() == ErrorKind::NotFound => {
                    debug!("Mdev {uuid:?} does no longer exist in sysfs");
                    Ok(None)
                }
                _ => Err(e),
            })?;
        if let (Some(parent), Some(mdev_type)) = (parent, mdev_type) {
            Ok(MDevSysfsData {
                uuid: uuid.to_owned(),
                parent,
                mdev_type,
            })
        } else {
            debug!("Mdev {uuid:?} does not exist in sysfs");
            Err(Error::DeviceNotFound)
        }
    }

    pub fn load_for_mdev(mdev: &MDev) -> Result<MDevSysfsData, Error> {
        Self::load(mdev.env, &mdev.uuid)
    }

    fn active_path(env: &Environment, uuid: &Uuid) -> PathBuf {
        env.mdev_base().join(uuid.hyphenated().to_string())
    }

    fn load_parent_from_sysfs<P: AsRef<Path>>(active_path: P) -> Result<String, Error> {
        let canonpath = fs::canonicalize(active_path.as_ref()).map_err(|e| {
            Error::IOError(
                format!("Can't get canonical path for {:?}", active_path.as_ref()),
                e,
            )
        })?;
        let sysfsparent = canonpath.parent().ok_or_else(|| {
            Error::System(format!(
                "Path to parent of mdev path {canonpath:?} does not exist"
            ))
        })?;
        Self::canonical_basename(sysfsparent)
    }

    fn load_mdev_type_from_sysfs<P: Into<PathBuf>>(active_path: P) -> Result<String, Error> {
        let mut typepath: PathBuf = active_path.into();
        typepath.push("mdev_type");
        Self::canonical_basename(typepath)
    }

    fn canonical_basename<P: AsRef<Path>>(path: P) -> Result<String, Error> {
        let path = fs::canonicalize(path.as_ref()).map_err(|e| {
            Error::IOError(
                format!("Failed to get canonical path for {:?}", path.as_ref()),
                e,
            )
        })?;
        let fname = path
            .file_name()
            .ok_or_else(|| Error::System(format!("Can't get filename for {path:?}")))?;
        match fname.to_str() {
            Some(x) => Ok(x.to_string()),
            None => Err(Error::System(format!(
                "file name {fname:?} is not valid unicode"
            ))),
        }
    }
}

/// Representation of a mediated device
#[derive(Debug, Clone)]
pub struct MDev<'e> {
    pub uuid: Uuid,
    pub active: bool,
    pub autostart: bool,
    pub parent: Option<String>,
    pub mdev_type: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub env: &'e Environment,
}

impl<'e> MDev<'e> {
    pub fn new(env: &'e Environment, uuid: Uuid) -> MDev<'e> {
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
        env: &'e Environment,
        uuid: Uuid,
        parent: String,
        jsonfile: PathBuf,
    ) -> Result<Self, Error> {
        let _ = std::fs::File::open(&jsonfile)
            .map_err(|e| Error::IOError(format!("Unable to read file {jsonfile:?}"), e))?;
        let filecontents = fs::read_to_string(&jsonfile)
            .map_err(|e| Error::IOError(format!("Unable to read jsonfile {jsonfile:?}"), e))?;
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
    pub fn parent(&self) -> Result<&String, Error> {
        self.parent.as_ref().ok_or_else(|| {
            Error::DeviceState(
                "parent device is not specified".to_string(),
                self.uuid,
                None,
            )
        })
    }

    // get mdev_type and propagate a consistent error to the caller if absent
    pub fn mdev_type(&self) -> Result<&String, Error> {
        self.mdev_type.as_ref().ok_or_else(|| {
            Error::DeviceState(
                "mdev_type is not specified".to_string(),
                self.uuid,
                self.parent.clone(),
            )
        })
    }

    pub fn persistent_path(&self) -> Result<PathBuf, Error> {
        self.parent().map(|x| {
            let mut path = self.env.config_base();
            path.push(x);
            path.push(self.uuid.hyphenated().to_string());
            path
        })
    }

    pub fn is_defined(&self) -> bool {
        match self.persistent_path() {
            Ok(p) => p.exists(),
            _ => false,
        }
    }

    pub fn set_sysfs_data(&mut self, sysfs_data: MDevSysfsData) {
        if self.uuid != sysfs_data.uuid {
            warn!(
                "Attempting to set sysfs data for device {} from sysfs data for UUID {}",
                self.uuid, sysfs_data.uuid
            );
            return;
        }
        self.parent = Some(sysfs_data.parent);
        self.mdev_type = Some(sysfs_data.mdev_type);
        self.active = true;
    }

    pub fn sysfs_data_matches(&self, sysfs_data: &MDevSysfsData) -> bool {
        if self.parent.as_ref() != Some(&sysfs_data.parent) {
            debug!(
                "Active mdev {:?} has different parent: {:?}!={}. No match.",
                self.uuid, self.parent, sysfs_data.parent
            );
            return false;
        }

        if self.mdev_type.as_ref() != Some(&sysfs_data.mdev_type) {
            debug!(
                "Active mdev {:?} has different type: {:?}!={}. No match.",
                self.uuid, self.mdev_type, sysfs_data.mdev_type
            );
            return false;
        }
        true
    }

    pub fn add_attributes(&mut self, attrs: &serde_json::Value) -> Result<(), Error> {
        if !attrs.is_array() && !attrs.is_null() {
            return Err(Error::DeviceFormat(
                "attributes field is not an array".to_string(),
            ));
        }

        if let Some(attrarray) = attrs.as_array() {
            if !attrarray.is_empty() {
                for attr in attrarray {
                    let attrobj = attr.as_object().ok_or_else(|| {
                        Error::DeviceFormat(
                            "invalid JSON format for attribute: not an object".to_string(),
                        )
                    })?;
                    // attributes are represented by JSON objects with a single field.
                    if attrobj.len() != 1 {
                        return Err(Error::DeviceFormat(
                            "invalid JSON format for attribute: too many fields".to_string(),
                        ));
                    }
                    // get the key and value from the first (only) map entry
                    if let Some((key, val)) = attrobj.iter().next() {
                        let valstr = val.as_str().ok_or_else(|| {
                            Error::DeviceFormat(format!("invalid JSON format for attribute {{{key:?}, {val}}}: value must be of type str"))
                        })?;
                        self.attrs.push((key.to_string(), valstr.to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn load_from_json(
        &mut self,
        parent: String,
        json: &serde_json::Value,
    ) -> Result<(), Error> {
        debug!(
            "Loading device '{:?}' from json (parent: {})",
            self.uuid, parent
        );
        if let Some(current) = self.parent.as_ref() {
            if current != &parent {
                warn!(
                    "Overwriting parent for mdev {:?}: {} => {}",
                    self.uuid, current, parent
                );
            }
        }
        self.parent = Some(parent);
        let mdev_type = json["mdev_type"]
            .as_str()
            .ok_or_else(|| Error::DeviceFormat("JSON must specify 'mdev_type' field".to_string()))?
            .to_string();
        if let Some(current) = self.mdev_type.as_ref() {
            if current != &mdev_type {
                warn!(
                    "Overwriting mdev type for mdev {:?}: {} => {}",
                    self.uuid, current, mdev_type
                );
            }
        }
        self.mdev_type = Some(mdev_type);
        let startval = json["start"]
            .as_str()
            .ok_or_else(|| Error::DeviceFormat("JSON must specify 'start' field".to_string()))?;
        self.autostart = startval == "auto";

        self.add_attributes(&json["attrs"])?;
        debug!("loaded device {self:?}");

        Ok(())
    }

    // load the stored definition from disk if it exists
    pub fn load_definition(&mut self) -> Result<(), Error> {
        if let Ok(path) = self.persistent_path().as_ref() {
            let mut f = fs::File::open(path)
                .map_err(|e| Error::IOError(format!("Failed to open file {path:?}"), e))?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)
                .map_err(|e| Error::IOError(format!("Failed to read file {path:?}"), e))?;
            let val = serde_json::from_str(&contents)?;
            let parent = self.parent().cloned()?;
            self.load_from_json(parent, &val)?;
        }
        Ok(())
    }

    pub fn to_text(&self, fmt: FormatType, verbose: bool) -> Result<String, Error> {
        match fmt {
            FormatType::Defined => {
                if !self.is_defined() {
                    return Err(Error::DeviceState(
                        "Device is not defined".to_string(),
                        self.uuid,
                        self.parent.clone(),
                    ));
                }
            }
            FormatType::Active => {
                if !self.active {
                    return Err(Error::DeviceState(
                        "Device is not active".to_string(),
                        self.uuid,
                        self.parent.clone(),
                    ));
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
                let txtattr = format!("    @{{{i}}}: {{\"{key}\":\"{value}\"}}\n");
                output.push_str(&txtattr);
            }
        }
        output
    }

    pub fn to_json(&self, include_uuid: bool) -> Result<serde_json::Value, Error> {
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

    pub fn stop(&mut self) -> Result<(), Error> {
        debug!("Removing mdev {:?}", self.uuid);
        let mut remove_path = self.active_path();
        remove_path.push("remove");
        debug!("remove path '{remove_path:?}'");
        fs::write(remove_path, "1")
            .map_err(|e| Error::IOError(format!("Error removing device {:?}", self.uuid), e))
            .inspect(|_| self.active = false)
    }

    fn find_parent_dir(&self) -> Result<PathBuf, Error> {
        let parent = self.parent()?;
        let path: PathBuf = self.env.parent_base().join(parent);

        if path.is_dir() {
            return Ok(path);
        }

        // check if there's a similar parent dir with different capitalization
        let parentsdir =
            self.env.parent_base().read_dir().map_err(|e| {
                Error::IOError("Failed to read parent device directory".to_string(), e)
            })?;
        for subdir in parentsdir {
            let dir = subdir.map_err(|e| {
                Error::IOError(
                    "Failed to read entry in directory parent device directory".to_string(),
                    e,
                )
            })?;
            let parentname = dir.file_name();
            if parentname.to_string_lossy().to_lowercase() == parent.to_lowercase() {
                return Err(Error::ParentNotFound(format!(
                    "{} (Did you mean {}?)",
                    parent,
                    parentname.to_string_lossy()
                )));
            }
        }
        Err(Error::ParentNotFound(parent.clone()))
    }

    fn create(&mut self) -> Result<(), Error> {
        debug!("Creating mdev {:?}", self.uuid);
        let parent = self.parent()?;
        let mdev_type = self.mdev_type()?;
        match MDevSysfsData::load(self.env, &self.uuid) {
            Ok(mdev_sysfs_data) => {
                if Some(&mdev_sysfs_data.parent) != self.parent.as_ref() {
                    return Err(Error::DeviceExists(format!(
                        "device {} found under different parent '{}'",
                        self.uuid, mdev_sysfs_data.parent
                    )));
                }
                if Some(&mdev_sysfs_data.mdev_type) != self.mdev_type.as_ref() {
                    return Err(Error::DeviceExists(format!(
                        "device {} found with different type '{}'",
                        self.uuid, mdev_sysfs_data.mdev_type
                    )));
                }
                return Err(Error::DeviceExists(self.uuid.to_string()));
            }
            Err(Error::DeviceNotFound) => (),
            Err(e) => {
                warn!(
                    "A sysfs lookup for device {} caused the error: {:?}",
                    self.uuid, e
                );
            }
        }

        let mut path = self.find_parent_dir()?;
        path.push("mdev_supported_types");
        debug!("Checking parent for mdev support: {path:?}");
        if !path.is_dir() {
            return Err(Error::Unsupported(format!(
                "parent {parent} is not currently registered for mdev support"
            )));
        }
        path.push(mdev_type);
        debug!("Checking parent for mdev type {mdev_type}: {path:?}");
        if !path.is_dir() {
            return Err(Error::Unsupported(format!(
                "parent {parent} does not support mdev type {mdev_type}"
            )));
        }
        path.push("available_instances");
        debug!("Checking available instances: {path:?}");
        let avail: i32 = fs::read_to_string(&path)
            .map_err(|e| {
                Error::IOError(
                    "Failed to read number of available instances".to_string(),
                    e,
                )
            })?
            .trim()
            .parse()
            .map_err(|e| {
                Error::System(format!(
                    "Failed to parse available instances as a string: {e}"
                ))
            })?;

        debug!("Available instances: {avail}");
        if avail == 0 {
            return Err(Error::InsufficientResources(format!(
                "No available instances of {mdev_type} on {parent}"
            )));
        }
        path.pop();
        path.push("create");
        debug!("Creating mediated device: {:?} -> {:?}", self.uuid, path);
        fs::write(path, self.uuid.hyphenated().to_string())
            .map_err(|e| {
                Error::IOError(
                    format!(
                        "Failed to create mdev {}, type {} on {}",
                        self.uuid.hyphenated(),
                        mdev_type,
                        parent
                    ),
                    e,
                )
            })
            .inspect(|_| self.active = true)
    }

    pub fn start(&mut self) -> Result<(), Error> {
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

    pub fn write_config(&self) -> Result<(), Error> {
        let jsonstring = serde_json::to_string_pretty(&self.to_json(false)?)?;
        let path = self.persistent_path()?;
        let parentdir = path
            .parent()
            .ok_or_else(|| Error::System(format!("Can't get parent directory of {path:?}")))?;
        debug!("Ensuring parent directory {parentdir:?} exists");
        fs::create_dir_all(parentdir).map_err(|e| {
            Error::IOError(
                format!("Failed to create parent directory {parentdir:?}"),
                e,
            )
        })?;
        debug!("Writing config for {:?} to {:?}", self.uuid, path);
        fs::write(path, jsonstring.as_bytes()).map_err(|e| {
            Error::IOError(
                format!("Failed to write config for device {:?}", self.uuid),
                e,
            )
        })
    }

    pub fn define(&self) -> Result<(), Error> {
        self.write_config()
    }

    pub fn undefine(&mut self) -> Result<(), Error> {
        let p = self.persistent_path()?;
        fs::remove_file(&p)
            .map_err(|e| Error::IOError(format!("Failed to remove file {p:?}"), e))?;
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
    ) -> Result<(), Error> {
        match index {
            Some(i) => {
                if i > self.attrs.len() {
                    return Err(Error::InvalidConfiguration(format!(
                        "Attribute index {} is invalid\n{}",
                        i,
                        self.attribute_hint()
                    )));
                }
                self.attrs.insert(i, (name, value));
            }
            None => self.attrs.push((name, value)),
        }

        Ok(())
    }

    pub fn delete_attribute(&mut self, index: Option<usize>) -> Result<(), Error> {
        match index {
            Some(i) => {
                if i >= self.attrs.len() {
                    return Err(Error::InvalidConfiguration(format!(
                        "Attribute index {} is invalid\n{}",
                        i,
                        self.attribute_hint()
                    )));
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

fn write_attr(basepath: &Path, attr: &str, val: &str) -> Result<(), Error> {
    debug!("Writing attribute '{attr}' -> '{val}'");
    let path = basepath.join(attr);
    if !path.exists() {
        return Err(Error::Unsupported(format!("Invalid attribute '{attr}'")));
    }
    fs::write(path, val)
        .map_err(|e| Error::IOError(format!("Failed to write {val} to attribute {attr}"), e))
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

    pub fn to_json(&self) -> serde_json::Value {
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

        serde_json::json!({ &self.typename: jsonobj })
    }
}
