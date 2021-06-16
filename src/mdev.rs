//! Structures for representing a mediated device

use crate::environment::Environment;
use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::convert::TryInto;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::vec::Vec;
use uuid::Uuid;
use std::process::{Command, Stdio, Output, ExitStatus};
use libsystemd::logging::{self, Priority};
use regex::Regex;

#[derive(Clone, Copy)]
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
    pub parent: Option<String>,
    pub mdev_type: Option<String>,
    pub attrs: Vec<(String, String)>,
    env: &'a dyn Environment,
}

impl<'a> MDev<'a> {
    pub fn new(env: &'a dyn Environment, uuid: Uuid) -> MDev<'a> {
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

    pub fn path(&self) -> PathBuf {
        let mut p = self.env.mdev_base();
        p.push(self.uuid.to_hyphenated().to_string());
        p
    }

    // get parent and propagate a consistent error to the caller if absent
    pub fn parent(&self) -> Result<&String> {
        self.parent.as_ref().ok_or_else(|| {
            anyhow!(
                "Device {} is missing a parent",
                self.uuid.to_hyphenated().to_string()
            )
        })
    }

    // get mdev_type and propagate a consistent error to the caller if absent
    pub fn mdev_type(&self) -> Result<&String> {
        self.mdev_type.as_ref().ok_or_else(|| {
            anyhow!(
                "Device {} is missing a mdev_type",
                self.uuid.to_hyphenated().to_string()
            )
        })
    }

    pub fn persist_path(&self) -> Option<PathBuf> {
        self.parent.as_ref().map(|x| {
            let mut path = self.env.persist_base();
            path.push(x);
            path.push(self.uuid.to_hyphenated().to_string());
            path
        })
    }

    pub fn is_defined(&self) -> bool {
        match self.persist_path() {
            Some(p) => p.exists(),
            None => false,
        }
    }

    pub fn load_from_sysfs(&mut self) -> Result<()> {
        debug!("Loading device '{:?}' from sysfs", self.uuid);
        if !self.path().exists() {
            debug!("loaded device {:?}", self);
            return Ok(());
        }

        let canonpath = self.path().canonicalize()?;
        let sysfsparent = canonpath.parent().unwrap();
        let parentname = canonical_basename(sysfsparent)?;
        if self.parent.is_some() && self.parent.as_ref() != Some(&parentname) {
            debug!(
                "Active mdev {:?} has different parent: {}!={}. No match.",
                self.uuid,
                self.parent.as_ref().unwrap(),
                parentname
            );
            return Ok(());
        }
        let mut typepath = self.path();
        typepath.push("mdev_type");
        let mdev_type = canonical_basename(typepath)?;
        if self.mdev_type.is_some() && self.mdev_type.as_ref() != Some(&mdev_type) {
            debug!(
                "Active mdev {:?} has different type: {}!={}. No match.",
                self.uuid,
                self.mdev_type.as_ref().unwrap(),
                mdev_type
            );
            return Ok(());
        }

        // active device in sysfs matches this device. update information
        self.mdev_type = Some(mdev_type);
        self.parent = Some(parentname);
        self.active = true;
        debug!("loaded device {:?}", self);
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

    // load the stored definition from disk if it exists
    pub fn load_definition(&mut self) -> Result<()> {
        if let Some(path) = self.persist_path() {
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

        let mut output = self.uuid.to_hyphenated().to_string();
        output.push(' ');
        output.push_str(&self.parent()?);
        output.push(' ');
        output.push_str(&self.mdev_type()?);
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
        partial.insert("mdev_type".to_string(), self.mdev_type()?.clone().into());
        partial.insert("start".to_string(), autostart.into());
        let jsonattrs: Vec<_> = self
            .attrs
            .iter()
            .map(|(key, value)| serde_json::json!({ key: value }))
            .collect();
        partial.insert("attrs".to_string(), jsonattrs.into());

        let full = serde_json::json!({ self.uuid.to_hyphenated().to_string(): partial });

        match include_uuid {
            true => Ok(full),
            false => Ok(partial.into()),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        debug!("Removing mdev {:?}", self.uuid);
        let mut remove_path = self.path();
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
        let parent = self.parent()?;
        let mdev_type = self.mdev_type()?;
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
            .join(&parent)
            .join("mdev_supported_types");
        debug!("Checking parent for mdev support: {:?}", path);
        if !path.is_dir() {
            return Err(anyhow!(
                "Parent {} is not currently registered for mdev support",
                parent
            ));
        }
        path.push(&mdev_type);
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
        match fs::write(path, self.uuid.to_hyphenated().to_string()) {
            Ok(_) => {
                self.active = true;
                Ok(())
            }
            Err(e) => Err(e).with_context(|| {
                format!(
                    "Failed to create mdev {}, type {} on {}",
                    self.uuid.to_hyphenated().to_string(),
                    mdev_type,
                    parent
                )
            }),
        }
    }

    pub fn start(&mut self, print_uuid: bool) -> Result<()> {
        self.create()?;

        debug!("Setting attributes for mdev {:?}", self.uuid);
        for (k, v) in self.attrs.iter() {
            if let Err(e) = write_attr(&self.path(), &k, &v) {
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
        let p = self.persist_path().ok_or_else(|| {
            anyhow!(
                "Failed to undefine {}",
                self.uuid.to_hyphenated().to_string()
            )
        })?;

        fs::remove_file(&p).with_context(|| format!("Failed to remove file {:?}", p))?;
        Ok(())
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

pub struct Callout<'a> {
    mdev: MDev<'a>,
    state: &'a str,
    conf: String,
    script: PathBuf,
    sname: String,
}

impl<'a> Callout<'a> {
    pub fn new(mdev: MDev<'a>) -> Callout<'a> {
        Callout {
            mdev: mdev,
            state: "none",
            conf: String::new(),
            script: PathBuf::new(),
            sname: String::new(),
        }
    }

    pub fn set_state(&mut self, state: &'a str) {
        self.state = state;
    }

    fn invoke_script<P: AsRef<Path>>(&mut self, script: P, event: &str, action: &str, stderr: bool, stdout: bool, autostart: bool) -> Result<ExitStatus> {
        let mut cmd = Command::new(script.as_ref().as_os_str());
        if event == "notify" {
            cmd.arg("-e")
                .arg(event)
                .arg("-a")
                .arg(action)
                .arg("-s")
                .arg(&self.state)
                .arg("-u")
                .arg(self.mdev.uuid.to_string())
                .arg("-p")
                .arg(self.mdev.parent.as_ref().unwrap())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        } else {
            cmd.arg("-t")
                .arg(self.mdev.mdev_type.as_ref().unwrap())
                .arg("-e")
                .arg(event)
                .arg("-a")
                .arg(action)
                .arg("-s")
                .arg(&self.state)
                .arg("-u")
                .arg(self.mdev.uuid.to_string())
                .arg("-p")
                .arg(self.mdev.parent.as_ref().unwrap())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
        }

        let mut child = cmd.spawn()?;

        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin
                .write_all(&self.conf.as_bytes())
                .with_context(|| "Failed to write to stdin of command")?;
        }

        let output = child.wait_with_output()?;

        self.print_output(script, output, stderr, stdout, autostart)
    }

    fn print_output<P: AsRef<Path>>(&mut self, script: P, output: Output, stderr: bool, stdout: bool, autostart: bool) -> Result<ExitStatus> {
        self.sname = script.as_ref().file_name().unwrap().to_str().unwrap().to_string();

        if stderr {
            let st = String::from_utf8_lossy(&output.stderr);
            if !st.is_empty() {
                let s = format!("{}: {}", &self.sname, st);
                eprint!("{}", &s);

                if autostart {
                    let _ = logging::journal_print(Priority::Warning, &s);
                }
            }
        }
        if stdout {
            let st = String::from_utf8_lossy(&output.stdout);
            if !st.is_empty() {
                let s = format!("{}: {}", &self.sname, st);
                print!("{}", &s);

                if autostart {
                    let _ = logging::journal_print(Priority::Warning, &s);
                }
            }
        }

        Ok(output.status)
    }

    pub fn callout(&mut self, event: &str, action: &str) -> Result<()> {
        if self.conf.is_empty() {
            self.conf = self.mdev.to_json(false)?.to_string();
        }

        let mut rc = Some(0);

        let dir = self.mdev.env.callout_script_base();
        if dir.read_dir()?.count() == 0 {
            return Ok(());
        }

        if self.script.to_str().unwrap().is_empty() {
            for s in dir.read_dir()? {
                let path = &s?.path();
                let res = self.invoke_script(path, event, action, true, false, self.mdev.autostart);
                rc = res?.code();
                if rc != Some(2) {
                    self.script = path.clone();
                    break;
                }
            }
        } else {
            let res = self.invoke_script(self.script.clone(), event, action, true, false, self.mdev.autostart);
            rc = res?.code();
        }

        match rc {
            Some(0) => Ok(()),
            _ => Err(anyhow!("aborting command due to results from callout script {:?}", self.script)),
        }
    }

    pub fn callout_notify(&mut self, event: &str, action: &str) -> Result<()> {
        let dir = self.mdev.env.callout_notification_base();

        if dir.read_dir()?.count() == 0 {
            return Ok(());
        }

        for s in dir.read_dir()? {
            let path = &s?.path();
            let _ = self.invoke_script(path, event, action, true, true, self.mdev.autostart);
        }

        Ok(())
    }

    pub fn callout_get(&mut self, event: &str, action: &str) -> Result<Vec<String>> {
        let mut v: Vec<String> = Vec::new();
        let mut st = String::new();

        let dir = self.mdev.env.callout_script_base();

        if dir.read_dir()?.count() == 0 {
            return Ok(v);
        }

        for s in dir.read_dir()? {
            let path = &s?.path();
            let cmd = Command::new(path.as_os_str())
                .arg("-t")
                .arg(self.mdev.mdev_type.as_ref().unwrap())
                .arg("-e")
                .arg(event)
                .arg("-a")
                .arg(action)
                .arg("-s")
                .arg("none")
                .arg("-u")
                .arg(self.mdev.uuid.to_string())
                .arg("-p")
                .arg(self.mdev.parent.as_ref().unwrap())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            let output = cmd.wait_with_output();

            let rc = output.as_ref().unwrap().status.code();
            if rc != Some(2) {
                self.script = path.clone();
                if output.as_ref().unwrap().status.success() {
                    st = String::from_utf8_lossy(&output.as_ref().unwrap().stdout).to_string();
                    break;
                } else {
                    return Err(anyhow!("failed to get attributes from {:?}", self.script));
                }
            }
        }

        let re = Regex::new(r"\w+").unwrap();
        for cap in re.captures_iter(&st) {
            v.push(cap[0].to_string());
        }

        Ok(v)
    }
}
