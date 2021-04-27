use anyhow::{anyhow, ensure, Context, Result};
use faccess::PathExt;
use log::{debug, warn};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::vec::Vec;
use structopt::StructOpt;
use uuid::Uuid;

mod tests;

#[derive(Debug)]
struct Environment {
    rootdir: PathBuf,
}

impl Environment {
    pub fn new(path: &str) -> Environment {
        Environment {
            rootdir: PathBuf::from(path),
        }
    }

    pub fn mdev_base(&self) -> PathBuf {
        self.rootdir.join("sys/bus/mdev/devices")
    }

    pub fn persist_base(&self) -> PathBuf {
        self.rootdir.join("etc/mdevctl.d")
    }

    pub fn parent_base(&self) -> PathBuf {
        self.rootdir.join("sys/class/mdev_bus")
    }
}

// command-line argument definitions.
#[derive(StructOpt)]
#[structopt(
    about = "A mediated device management utility for Linux",
    global_settings = &[
        structopt::clap::AppSettings::VersionlessSubcommands,
        structopt::clap::AppSettings::UnifiedHelpMessage,
    ]
)]
enum Cli {
    #[structopt(
        about = "Define a persistent mediated device",
        long_about = "Define a persistent mediated device\n\n\
                If the device specified by the UUID currently exists, 'parent' \
                and 'type' may be omitted to use the existing values. The 'auto' \
                option marks the device to start on parent availability. \
                If defined via 'jsonfile', then 'type', 'startup', and any attributes \
                are provided via the file.\n\n\
                Running devices are unaffected by this command."
    )]
    Define {
        #[structopt(short, long, required_unless("parent"))]
        uuid: Option<Uuid>,
        #[structopt(short, long)]
        auto: bool,
        #[structopt(short, long, required_unless("uuid"))]
        parent: Option<String>,
        #[structopt(name = "type", short, long)]
        mdev_type: Option<String>,
        #[structopt(long, parse(from_os_str), conflicts_with_all(&["type", "auto"]))]
        jsonfile: Option<PathBuf>,
    },

    #[structopt(
        about = "Undefine a persistent mediated device",
        long_about = "Undefine, or remove a config for an mdev device\n\n\
                If a UUID exists for multiple parents, all will be removed \
                unless a parent is specified. \n\n\
                Running devices are unaffected by this command."
    )]
    Undefine {
        #[structopt(short, long)]
        uuid: Uuid,
        #[structopt(short, long)]
        parent: Option<String>,
    },

    #[structopt(
        about = "Modify the definition of a mediated device",
        long_about = "Modify the definition of a mediated device\n\n\
                The 'parent' option further identifies a UUID if it is not \
                unique. The parent for a device cannot be modified via this \
                command; undefine and re-define should be used instead. An \
                attribute can be added or removed, which correlates to a \
                sysfs attribute under the created device. Unless an 'index' \
                value is provided, operations are performed at the end of \
                the attribute list. 'value' is to be specified in the format \
                that is accepted by the attribute. Upon device start, mdevctl \
                will go through each attribute in order, writing the value into \
                the corresponding sysfs attribute for the device. The startup \
                mode of the device can also be selected, auto or manual. \n\n\
                Running devices are unaffected by this command."
    )]
    Modify {
        #[structopt(short, long)]
        uuid: Uuid,
        #[structopt(short, long)]
        parent: Option<String>,
        #[structopt(name = "type", short, long)]
        mdev_type: Option<String>,
        #[structopt(long, conflicts_with("delattr"))]
        addattr: Option<String>,
        #[structopt(long)]
        delattr: bool,
        #[structopt(long, short)]
        index: Option<u32>,
        #[structopt(long)]
        value: Option<String>,
        #[structopt(short, long)]
        auto: bool,
        #[structopt(short, long, conflicts_with("auto"))]
        manual: bool,
    },
    #[structopt(
        about = "Start a mediated device",
        long_about = "Start a mediated device\n\n\
                If the UUID is previously defined and unique, the UUID is \
                sufficient to start the device (UUIDs may not collide between \
                running devices). If a UUID is used in multiple defined \
                configurations, the 'parent' is necessary to identify the device to be started. \
                When specified with 'parent' and 'type', the device is fully \
                specified and will be started based only on these parameters. \
                The UUID is optional in this case. If not provided, a UUID is \
                generated and returned as output. A 'jsonfile' may replace the 'type' \
                specification and also include additional attributes to be \
                applied to the started device."
    )]
    Start {
        #[structopt(short, long, required_unless("parent"))]
        uuid: Option<Uuid>,
        #[structopt(short, long, required_unless("uuid"))]
        parent: Option<String>,
        #[structopt(name = "type", short, long)]
        mdev_type: Option<String>,
        #[structopt(long, parse(from_os_str), conflicts_with("type"))]
        jsonfile: Option<PathBuf>,
    },
    #[structopt(about = "Stop a mediated device")]
    Stop {
        #[structopt(short, long)]
        uuid: Uuid,
    },
    #[structopt(
        about = "List mediated devices",
        long_about = "List mediated devices\n\n\
                With no options, information about the currently running mediated \
                devices is provided. Specifying 'defined' lists the \
                configuration of defined devices, regardless of their running \
                state. This may be further reduced by specifying specific \
                'uuid' or 'parent' devices to list. The 'dumpjson' option provides \
                output listing in machine readable JSON format. When a 'uuid' \
                option is provided and the result is a single device, the \
                output contains only the JSON fields necessary to recreate a \
                config file for the device (minus attributes for listings of \
                running devices). When the verbose option is provided, the \
                human readable listing will include attributes for the \
                device(s)."
    )]
    List {
        #[structopt(short, long)]
        defined: bool,
        #[structopt(long)]
        dumpjson: bool,
        #[structopt(short, long)]
        verbose: bool,
        #[structopt(short, long)]
        uuid: Option<Uuid>,
        #[structopt(short, long)]
        parent: Option<String>,
    },
    #[structopt(
        about = "List available mediated device types",
        long_about = "List available mediated device types\n\n\
                Specifying a 'parent' lists only the types provided by the given \
                parent device.  The 'dumpjson' option provides output in machine \
                readable JSON format."
    )]
    Types {
        #[structopt(short, long)]
        parent: Option<String>,
        #[structopt(long)]
        dumpjson: bool,
    },
    #[structopt(setting = structopt::clap::AppSettings::Hidden)]
    StartParentMdevs { parent: String },
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

enum FormatType {
    Active,
    Defined,
}

#[derive(Debug, Clone)]
struct MdevTypeInfo {
    parent: String,
    typename: String,
    available_instances: i32,
    device_api: String,
    name: String,
    description: String,
}

impl MdevTypeInfo {
    pub fn new() -> MdevTypeInfo {
        MdevTypeInfo {
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

#[derive(Debug, Clone)]
struct MdevInfo<'a> {
    uuid: Uuid,
    active: bool,
    autostart: bool,
    path: PathBuf,
    parent: String,
    mdev_type: String,
    attrs: Vec<(String, String)>,
    env: &'a Environment,
}

impl<'a> MdevInfo<'a> {
    pub fn new(env: &'a Environment, uuid: Uuid) -> MdevInfo<'a> {
        MdevInfo {
            uuid: uuid,
            active: false,
            autostart: false,
            path: PathBuf::new(),
            parent: String::new(),
            mdev_type: String::new(),
            attrs: Vec::new(),
            env: env,
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
        self.autostart = match startval {
            Some("auto") => true,
            _ => false,
        };

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
        let mut existing = MdevInfo::new(self.env, self.uuid);

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

fn format_json(devices: BTreeMap<String, Vec<MdevInfo>>) -> Result<String> {
    let mut parents = serde_json::map::Map::new();
    for (parentname, children) in devices {
        let mut childrenarray = Vec::new();
        for child in children {
            childrenarray.push(child.to_json(true)?);
        }
        parents.insert(parentname, childrenarray.into());
    }
    // don't serialize an empty object if there are no devices
    let jsonval = match parents.len() {
        0 => serde_json::json!([]),
        _ => serde_json::json!([parents]),
    };
    serde_json::to_string_pretty(&jsonval).map_err(|_e| anyhow!("Unable to serialize json"))
}

// convert 'define' command arguments into a MdevInfo struct
fn define_command_helper(
    env: &Environment,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<MdevInfo> {
    let uuid_provided = uuid.is_some();
    let uuid = uuid.unwrap_or_else(Uuid::new_v4);
    let mut dev = MdevInfo::new(env, uuid);

    if let Some(jsonfile) = jsonfile {
        if !jsonfile.readable() {
            return Err(anyhow!("Unable to read file {:?}", jsonfile));
        }

        if mdev_type.is_some() {
            return Err(anyhow!(
                "Device type cannot be specified separately from {:?}",
                jsonfile
            ));
        }

        if parent.is_none() {
            return Err(anyhow!(
                "Parent device required to define device via {:?}",
                jsonfile
            ));
        }

        let devs = defined_devices(env, &Some(uuid), &parent)?;
        if !devs.is_empty() {
            return Err(anyhow!(
                "Cowardly refusing to overwrite existing config for {}/{}",
                parent.unwrap(),
                uuid.to_hyphenated().to_string()
            ));
        }

        let filecontents = fs::read_to_string(&jsonfile)
            .with_context(|| format!("Unable to read jsonfile {:?}", jsonfile))?;
        let jsonval: serde_json::Value = serde_json::from_str(&filecontents)?;
        dev.load_from_json(parent.unwrap(), &jsonval)?;
    } else {
        if uuid_provided {
            dev.load_from_sysfs()?;
            if parent.is_none() {
                if !dev.active || mdev_type.is_some() {
                    return Err(anyhow!("No parent specified"));
                }
            }
        }

        dev.autostart = auto;
        if let Some(p) = parent {
            dev.parent = p;
        }
        if let Some(t) = mdev_type {
            dev.mdev_type = t;
        }

        if dev.parent.is_empty() {
            return Err(anyhow!("No parent specified"));
        }
        if dev.mdev_type.is_empty() {
            return Err(anyhow!("No type specified"));
        }

        if dev.is_defined() {
            return Err(anyhow!(
                "Device {} on {} already defined, try modify?",
                dev.uuid.to_hyphenated().to_string(),
                dev.parent
            ));
        }
    }

    Ok(dev)
}

fn define_command(
    env: &Environment,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    debug!("Defining mdev {:?}", uuid);

    let dev = define_command_helper(env, uuid, auto, parent, mdev_type, jsonfile)?;
    dev.define().and_then(|_| {
        if uuid.is_none() {
            println!("{}", dev.uuid.to_hyphenated());
        }
        Ok(())
    })
}

fn undefine_command(env: &Environment, uuid: Uuid, parent: Option<String>) -> Result<()> {
    debug!("Undefining mdev {:?}", uuid);
    let devs = defined_devices(env, &Some(uuid), &parent)?;
    if devs.is_empty() {
        return Err(anyhow!("No devices match the specified uuid"));
    }
    for (_, mut children) in devs {
        for child in children.iter_mut() {
            child.undefine()?;
        }
    }
    Ok(())
}

fn modify_command(
    env: &Environment,
    uuid: Uuid,
    parent: Option<String>,
    mdev_type: Option<String>,
    addattr: Option<String>,
    delattr: bool,
    index: Option<u32>,
    value: Option<String>,
    auto: bool,
    manual: bool,
) -> Result<()> {
    let mut dev = get_defined_device(env, uuid, &parent)?;
    if let Some(t) = mdev_type {
        dev.mdev_type = t
    }

    if auto {
        dev.autostart = true;
    } else if manual {
        dev.autostart = false;
    }

    match addattr {
        Some(attr) => match value {
            None => return Err(anyhow!("No attribute value provided")),
            Some(v) => dev.add_attribute(attr, v, index)?,
        },
        None => {
            if delattr {
                dev.delete_attribute(index)?;
            }
        }
    }

    dev.write_config()
}

fn write_attr(basepath: &Path, attr: &str, val: &str) -> Result<()> {
    debug!("Writing attribute '{}' -> '{}'", attr, val);
    let path = basepath.join(attr);
    if !path.exists() {
        return Err(anyhow!("Invalid attribute '{}'", val));
    } else if !path.writable() {
        return Err(anyhow!("Attribute '{}' cannot be set", val));
    }
    fs::write(path, val).with_context(|| format!("Failed to write {} to attribute {}", val, attr))
}

fn start_command_helper(
    env: &Environment,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<MdevInfo> {
    debug!("Starting device '{:?}'", uuid);
    let mut dev: Option<MdevInfo> = None;
    match jsonfile {
        Some(fname) => {
            let contents = fs::read_to_string(&fname)
                .with_context(|| format!("Unable to read jsonfile {:?}", fname))?;
            let val: serde_json::Value = serde_json::from_str(&contents)?;

            if mdev_type.is_some() {
                return Err(anyhow!(
                    "Device type cannot be specified separately from json file"
                ));
            }

            if parent.is_none() {
                return Err(anyhow!(
                    "Parent device required to start device via json file"
                ));
            }

            let mut d = MdevInfo::new(env, uuid.unwrap_or_else(Uuid::new_v4));
            d.load_from_json(parent.unwrap(), &val)?;
            dev = Some(d);
        }
        _ => {
            if mdev_type.is_some() && parent.is_none() {
                return Err(anyhow!("can't provide type without parent"));
            }

            /* The device is not fully specified without TYPE, we must find a config file, with optional
             * PARENT for disambiguation */
            if mdev_type.is_none() && uuid.is_some() {
                dev = match get_defined_device(env, uuid.unwrap(), &parent) {
                    Ok(d) => Some(d),
                    Err(e) => return Err(e),
                }
            }
            if uuid.is_none() {
                if parent.is_none() || mdev_type.is_none() {
                    return Err(anyhow!("Device is insufficiently specified"));
                }
            }

            if dev.is_none() {
                let mut d = MdevInfo::new(env, uuid.unwrap_or_else(Uuid::new_v4));
                d.parent = parent.unwrap();
                d.mdev_type = mdev_type.unwrap();
                dev = Some(d);
            }
        }
    }
    Ok(dev.unwrap())
}

fn start_command(
    env: &Environment,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    let mut dev = start_command_helper(env, uuid, parent, mdev_type, jsonfile)?;
    dev.start(uuid.is_none())
}

fn stop_command(env: &Environment, uuid: Uuid) -> Result<()> {
    debug!("Stopping '{}'", uuid);
    let mut info = MdevInfo::new(env, uuid);
    info.load_from_sysfs()?;
    info.stop()
}

fn get_defined_device<'a>(
    env: &'a Environment,
    uuid: Uuid,
    parent: &Option<String>,
) -> Result<MdevInfo<'a>> {
    let u = Some(uuid);
    let devs = defined_devices(env, &u, parent)?;
    if devs.is_empty() {
        return match parent {
            None => Err(anyhow!(
                "Mediated device {} is not defined",
                uuid.to_hyphenated().to_string()
            )),
            Some(p) => Err(anyhow!(
                "Mediated device {}/{} is not defined",
                p,
                uuid.to_hyphenated().to_string()
            )),
        };
    } else if devs.len() > 1 {
        return match parent {
            None => Err(anyhow!(
                "Multiple definitions found for {}, specify a parent",
                uuid.to_hyphenated().to_string()
            )),
            Some(p) => Err(anyhow!(
                "Multiple definitions found for {}/{}",
                p,
                uuid.to_hyphenated().to_string()
            )),
        };
    } else {
        let (parent, children) = devs.iter().next().unwrap();
        if children.len() > 1 {
            return Err(anyhow!(
                "Multiple definitions found for {}/{}",
                parent,
                uuid.to_hyphenated().to_string()
            ));
        }
        return Ok(children.get(0).unwrap().clone());
    }
}

fn defined_devices<'a>(
    env: &'a Environment,
    uuid: &Option<Uuid>,
    parent: &Option<String>,
) -> Result<BTreeMap<String, Vec<MdevInfo<'a>>>> {
    let mut devices: BTreeMap<String, Vec<MdevInfo>> = BTreeMap::new();
    debug!(
        "Looking up defined mdevs: uuid={:?}, parent={:?}",
        uuid, parent
    );
    for parentpath in env.persist_base().read_dir()? {
        let parentpath = parentpath?;
        let parentname = parentpath.file_name();
        let parentname = parentname.to_str().unwrap();
        if (parent.is_some() && parent.as_ref().unwrap() != parentname)
            || !parentpath.metadata()?.is_dir()
        {
            debug!("Ignoring child devices for parent {}", parentname);
            continue;
        }

        let mut childdevices = Vec::new();

        for child in parentpath.path().read_dir()? {
            let child = child?;
            if !child.metadata()?.is_file() {
                continue;
            }

            let path = child.path();
            let basename = path.file_name().unwrap().to_str().unwrap();
            let u = Uuid::parse_str(basename).unwrap();
            debug!("found mdev {:?}", u);
            if uuid.is_some() && uuid.as_ref().unwrap() != &u {
                debug!(
                    "Ignoring device {} because it doesn't match uuid {}",
                    u,
                    uuid.unwrap()
                );
                continue;
            }

            let mut f = fs::File::open(path)?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)?;
            let val: serde_json::Value = serde_json::from_str(&contents)?;
            let mut dev = MdevInfo::new(env, u);
            dev.load_from_json(parentname.to_string(), &val)?;
            dev.load_from_sysfs()?;

            childdevices.push(dev);
        }
        if !childdevices.is_empty() {
            devices.insert(parentname.to_string(), childdevices);
        }
    }
    Ok(devices)
}

fn list_command(
    env: &Environment,
    defined: bool,
    dumpjson: bool,
    verbose: bool,
    uuid: Option<Uuid>,
    parent: Option<String>,
) -> Result<()> {
    let mut devices: BTreeMap<String, Vec<MdevInfo>> = BTreeMap::new();
    if defined {
        devices = defined_devices(env, &uuid, &parent)?;
    } else {
        debug!("Looking up active mdevs");
        for dev in env.mdev_base().read_dir()? {
            let dev = dev?;
            let fname = dev.file_name();
            let basename = fname.to_str().unwrap();
            debug!("found defined mdev {}", basename);
            let u = Uuid::parse_str(basename).unwrap();

            if uuid.is_some() && u != uuid.unwrap() {
                debug!(
                    "Ignoring device {} because it doesn't match uuid {}",
                    u,
                    uuid.unwrap()
                );
                continue;
            }

            let mut info = MdevInfo::new(env, u);
            if info.load_from_sysfs().is_ok() {
                if let Some(p) = parent.as_ref() {
                    if p != &info.parent {
                        debug!(
                            "Ignoring device {} because it doesn't match parent {}",
                            info.uuid, p
                        );
                        continue;
                    }
                }

                if !devices.contains_key(&info.parent) {
                    devices.insert(info.parent.clone(), Vec::new());
                };

                devices.get_mut(&info.parent).unwrap().push(info);
            };
        }
    }

    if dumpjson {
        let output = format_json(devices)?;
        println!("{}", output);
    } else {
        let mut output = String::new();
        for (_parent, children) in devices {
            let ft = match defined {
                true => FormatType::Defined,
                false => FormatType::Active,
            };
            for dev in children {
                output.push_str(&dev.to_text(&ft, verbose)?);
            }
        }
        println!("{}", output);
    }

    Ok(())
}

fn supported_types(
    env: &Environment,
    parent: Option<String>,
) -> Result<BTreeMap<String, Vec<MdevTypeInfo>>> {
    debug!("Finding supported mdev types");
    let mut types: BTreeMap<String, Vec<MdevTypeInfo>> = BTreeMap::new();

    for parentpath in env.parent_base().read_dir()? {
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

            let mut t = MdevTypeInfo::new();
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
                    .replace("\n", ", ")
                    .to_string();
            }

            childtypes.push(t);
        }
        types.insert(parentname.to_string(), childtypes);
    }
    Ok(types)
}

fn types_command(env: &Environment, parent: Option<String>, dumpjson: bool) -> Result<()> {
    let types = supported_types(env, parent)?;
    debug!("{:?}", types);
    if dumpjson {
        let mut jsontypes: serde_json::Value = serde_json::json!([]);
        for (parent, children) in types {
            let mut jsonchildren: serde_json::Value = serde_json::json!([]);
            for child in children {
                jsonchildren.as_array_mut().unwrap().push(child.to_json()?);
            }
            let jsonparent = serde_json::json!({ parent: jsonchildren });
            jsontypes.as_array_mut().unwrap().push(jsonparent);
        }
        println!("{}", serde_json::to_string_pretty(&jsontypes)?);
    } else {
        for (parent, children) in types {
            println!("{}", parent);
            for child in children {
                println!("  {}", child.typename);
                println!("    Available instances: {}", child.available_instances);
                println!("    Device API: {}", child.device_api);
                if !child.name.is_empty() {
                    println!("    Name: {}", child.name);
                }
                if !child.description.is_empty() {
                    println!("    Description: {}", child.description);
                }
            }
        }
    }
    Ok(())
}

fn start_parent_mdevs_command(env: &Environment, parent: String) -> Result<()> {
    let mut devs = defined_devices(env, &None, &Some(parent))?;
    if devs.is_empty() {
        // nothing to do
        return Ok(());
    }

    ensure!(devs.len() == 1, "More than one parent found");

    for (_, children) in devs.iter_mut() {
        for child in children {
            if child.autostart {
                debug!("Autostarting {:?}", child.uuid);
                if let Err(e) = child.start(false) {
                    for x in e.chain() {
                        warn!("{}", x);
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    debug!("Starting up");
    let args = Cli::from_args();
    let env = Environment::new("/");
    debug!("Parsed args");
    match args {
        Cli::Define {
            uuid,
            auto,
            parent,
            mdev_type,
            jsonfile,
        } => define_command(&env, uuid, auto, parent, mdev_type, jsonfile),
        Cli::Undefine { uuid, parent } => undefine_command(&env, uuid, parent),
        Cli::Modify {
            uuid,
            parent,
            mdev_type,
            addattr,
            delattr,
            index,
            value,
            auto,
            manual,
        } => modify_command(
            &env, uuid, parent, mdev_type, addattr, delattr, index, value, auto, manual,
        ),
        Cli::Start {
            uuid,
            parent,
            mdev_type,
            jsonfile,
        } => start_command(&env, uuid, parent, mdev_type, jsonfile),
        Cli::Stop { uuid } => stop_command(&env, uuid),
        Cli::List {
            defined,
            dumpjson,
            verbose,
            uuid,
            parent,
        } => list_command(&env, defined, dumpjson, verbose, uuid, parent),
        Cli::Types { parent, dumpjson } => types_command(&env, parent, dumpjson),
        Cli::StartParentMdevs { parent } => start_parent_mdevs_command(&env, parent),
    }
}
