//! Structures for representing a mediated device

use crate::environment::Environment;
use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::convert::TryInto;
use std::fs;
use std::path::{Path, PathBuf};
use std::vec::Vec;
use uuid::Uuid;

pub enum FormatType {
    Active,
    Defined,
}

/// Representation of a mediated device
#[derive(Debug, Clone)]
pub struct MDev<'a> {
    pub uuid: Uuid,
    pub active: bool,
    pub autostart: bool,
    pub path: PathBuf,
    pub parent: String,
    pub mdev_type: String,
    pub attrs: Vec<(String, String)>,
    env: &'a dyn Environment,
}

impl<'a> MDev<'a> {
    pub fn new(env: &'a dyn Environment, uuid: Uuid) -> MDev<'a> {
        MDev {
            uuid,
            active: false,
            autostart: false,
            path: PathBuf::new(),
            parent: String::new(),
            mdev_type: String::new(),
            attrs: Vec::new(),
            env,
        }
    }

    pub fn persist_path(&self) -> Option<PathBuf> {
        if self.parent.is_empty() {
            return None;
        }

        let mut path = self.env.persist_base();
        path.push(&self.parent);
        path.push(self.uuid.to_hyphenated().to_string());
        Some(path)
    }

    pub fn is_defined(&self) -> bool {
        match self.persist_path() {
            Some(p) => p.exists(),
            None => false,
        }
    }

    pub fn load_from_sysfs(&mut self) -> Result<()> {
        debug!("Loading device '{:?}' from sysfs", self.uuid);
        self.path = self.env.mdev_base();
        self.path.push(self.uuid.to_hyphenated().to_string());
        self.active = match self.path.symlink_metadata() {
            Ok(attr) => attr.file_type().is_symlink(),
            _ => false,
        };

        if !self.active {
            debug!("loaded device {:?}", self);
            return Ok(());
        }

        let canonpath = self.path.canonicalize()?;
        let sysfsparent = canonpath.parent().unwrap();
        let parentname = canonical_basename(sysfsparent)?;
        if !self.parent.is_empty() && self.parent != parentname {
            warn!(
                "Overwriting parent for mdev {:?}: {} => {}",
                self.uuid, self.parent, parentname
            );
        }
        self.parent = parentname;
        let mut typepath = self.path.to_owned();
        typepath.push("mdev_type");
        let mdev_type = canonical_basename(typepath)?;
        if !self.mdev_type.is_empty() && self.mdev_type != mdev_type {
            warn!(
                "Overwriting mdev type for mdev {:?}: {} => {}",
                self.uuid, self.mdev_type, mdev_type
            );
        }
        self.mdev_type = mdev_type;

        debug!("loaded device {:?}", self);
        Ok(())
    }

    pub fn load_from_json(&mut self, parent: String, json: &serde_json::Value) -> Result<()> {
        debug!(
            "Loading device '{:?}' from json (parent: {})",
            self.uuid, parent
        );
        if !self.parent.is_empty() && self.parent != parent {
            warn!(
                "Overwriting parent for mdev {:?}: {} => {}",
                self.uuid, self.parent, parent
            );
        }
        self.parent = parent;
        if json["mdev_type"].is_null() || json["start"].is_null() {
            return Err(anyhow!("invalid json"));
        }
        let mdev_type = json["mdev_type"].as_str().unwrap().to_string();
        if !self.mdev_type.is_empty() && self.mdev_type != mdev_type {
            warn!(
                "Overwriting mdev type for mdev {:?}: {} => {}",
                self.uuid, self.mdev_type, mdev_type
            );
        }
        self.mdev_type = mdev_type;
        let startval = json["start"].as_str();
        self.autostart = matches!(startval, Some("auto"));

        if let Some(attrarray) = json["attrs"].as_array() {
            if !attrarray.is_empty() {
                for attr in json["attrs"].as_array().unwrap() {
                    let attrobj = attr.as_object().unwrap();
                    for (key, val) in attrobj.iter() {
                        let valstr = val.as_str().unwrap();
                        self.attrs.push((key.to_string(), valstr.to_string()));
                    }
                }
            }
        };
        debug!("loaded device {:?}", self);

        Ok(())
    }

    pub fn to_text(&self, fmt: &FormatType, verbose: bool) -> Result<String> {
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

        let mut output = self.uuid.to_hyphenated().to_string();
        output.push(' ');
        output.push_str(&self.parent);
        output.push(' ');
        output.push_str(&self.mdev_type);
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
        if verbose && !self.attrs.is_empty() {
            output.push_str("  Attrs:\n");
            for (i, (key, value)) in self.attrs.iter().enumerate() {
                let txtattr = format!("    @{{{}}}: {{\"{}\":\"{}\"}}\n", i, key, value);
                output.push_str(&txtattr);
            }
        }
        Ok(output)
    }

    pub fn to_json(&self, include_uuid: bool) -> Result<serde_json::Value> {
        let autostart = match self.autostart {
            true => "auto",
            false => "manual",
        };
        let mut partial = serde_json::Map::new();
        partial.insert("mdev_type".to_string(), self.mdev_type.clone().into());
        partial.insert("start".to_string(), autostart.into());
        if !self.attrs.is_empty() {
            let mut jsonattrs = Vec::new();
            for (key, value) in &self.attrs {
                let attr = serde_json::json!({ key: value });
                jsonattrs.push(attr);
            }
            partial.insert("attrs".to_string(), jsonattrs.into());
        }

        let full: serde_json::Value =
            serde_json::json!({ self.uuid.to_hyphenated().to_string(): partial });

        match include_uuid {
            true => Ok(full),
            false => Ok(partial.into()),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        debug!("Removing mdev {:?}", self.uuid);
        let mut remove_path = self.path.clone();
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

    pub fn create(&mut self) -> Result<()> {
        debug!("Creating mdev {:?}", self.uuid);
        let mut existing = MDev::new(self.env, self.uuid);

        if existing.load_from_sysfs().is_ok() && existing.active {
            if existing.parent != self.parent {
                return Err(anyhow!("Device exists under different parent"));
            }
            if existing.mdev_type != self.mdev_type {
                return Err(anyhow!("Device exists with different type"));
            }
            return Err(anyhow!("Device already exists"));
        }

        let mut path: PathBuf = self
            .env
            .parent_base()
            .join(&self.parent)
            .join("mdev_supported_types");
        debug!("Checking parent for mdev support: {:?}", path);
        if !path.is_dir() {
            return Err(anyhow!(
                "Parent {} is not currently registered for mdev support",
                self.parent
            ));
        }
        path.push(&self.mdev_type);
        debug!(
            "Checking parent for mdev type {}: {:?}",
            self.mdev_type, path
        );
        if !path.is_dir() {
            return Err(anyhow!(
                "Parent {} does not support mdev type {}",
                self.parent,
                self.mdev_type
            ));
        }
        path.push("available_instances");
        debug!("Checking available instances: {:?}", path);
        let avail: i32 = fs::read_to_string(&path)?.trim().parse()?;

        debug!("Available instances: {}", avail);
        if avail == 0 {
            return Err(anyhow!(
                "No available instances of {} on {}",
                self.mdev_type,
                self.parent
            ));
        }
        path.pop();
        path.push("create");
        debug!("Creating mediated device: {:?} -> {:?}", self.uuid, path);
        match fs::write(path, self.uuid.to_hyphenated().to_string()) {
            Ok(_) => {
                self.active = true;
                Ok(())
            }
            Err(e) => Err(e).with_context(|| {
                format!(
                    "Failed to create mdev {}, type {} on {}",
                    self.uuid.to_hyphenated().to_string(),
                    self.mdev_type,
                    self.parent
                )
            }),
        }
    }

    pub fn start(&mut self, print_uuid: bool) -> Result<()> {
        self.create()?;

        debug!("Setting attributes for mdev {:?}", self.uuid);
        for (k, v) in self.attrs.iter() {
            if let Err(e) = write_attr(&self.path, &k, &v) {
                self.stop()?;
                return Err(e);
            }
        }

        if print_uuid {
            println!("{}", self.uuid.to_hyphenated().to_string());
        }

        Ok(())
    }

    pub fn write_config(&self) -> Result<()> {
        let jsonstring = serde_json::to_string_pretty(&self.to_json(false)?)?;
        let path = self.persist_path().unwrap();
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
        match self.persist_path() {
            Some(p) => fs::remove_file(p).with_context(|| {
                format!(
                    "Failed to undefine {}",
                    self.uuid.to_hyphenated().to_string()
                )
            }),
            None => Err(anyhow!(
                "Failed to undefine {}",
                self.uuid.to_hyphenated().to_string()
            )),
        }
    }

    pub fn add_attribute(&mut self, name: String, value: String, index: Option<u32>) -> Result<()> {
        match index {
            Some(i) => {
                let i: usize = i.try_into().unwrap();
                if i > self.attrs.len() {
                    return Err(anyhow!("Attribute index {} is invalid", i));
                }
                self.attrs.insert(i, (name, value));
            }
            None => self.attrs.push((name, value)),
        }

        Ok(())
    }

    pub fn delete_attribute(&mut self, index: Option<u32>) -> Result<()> {
        match index {
            Some(i) => {
                let i: usize = i.try_into().unwrap();
                if i > self.attrs.len() {
                    return Err(anyhow!("Attribute index {} is invalid", i));
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

fn canonical_basename<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = fs::canonicalize(path)?;
    let fname = path.file_name();
    if fname.is_none() {
        return Err(anyhow!("Invalid path"));
    }
    let fname = fname.unwrap().to_str();
    match fname {
        Some(x) => Ok(x.to_string()),
        None => Err(anyhow!("Invalid file name")),
    }
}

fn write_attr(basepath: &Path, attr: &str, val: &str) -> Result<()> {
    debug!("Writing attribute '{}' -> '{}'", attr, val);
    let path = basepath.join(attr);
    if !path.exists() {
        return Err(anyhow!("Invalid attribute '{}'", val));
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
        let mut jsonobj: serde_json::Value = serde_json::json!({
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
