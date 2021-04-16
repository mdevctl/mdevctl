use anyhow::{anyhow, Result};
use log::{debug, warn};
use serde_json;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use uuid::Uuid;

const MDEV_BASE: &str = "/sys/bus/mdev/devices";
const PERSIST_BASE: &str = "/etc/mdevctl.d";

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
    Undefine {
        #[structopt(short, long)]
        uuid: Uuid,
        #[structopt(short, long)]
        parent: Option<String>,
    },
    Modify {
        #[structopt(short, long)]
        uuid: Uuid,
        #[structopt(short, long)]
        parent: Option<String>,
        #[structopt(name = "type", short, long)]
        mdev_type: Option<String>,
        #[structopt(long)]
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
    Stop {
        #[structopt(short, long)]
        uuid: Uuid,
    },
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
    Types {
        #[structopt(short, long)]
        parent: Option<String>,
        #[structopt(long)]
        dumpjson: bool,
    },
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

#[derive(Debug)]
struct MdevInfo {
    uuid: Uuid,
    active: bool,
    defined: bool,
    autostart: bool,
    path: PathBuf,
    parent: String,
    mdev_type: String,
    attrs: BTreeMap<String, String>,
}

impl MdevInfo {
    pub fn new(uuid: Uuid) -> MdevInfo {
        MdevInfo {
            uuid: uuid,
            active: false,
            defined: false,
            autostart: false,
            path: PathBuf::new(),
            parent: String::new(),
            mdev_type: String::new(),
            attrs: BTreeMap::new(),
        }
    }

    pub fn load_from_sysfs(&mut self) -> Result<()> {
        debug!("Loading device '{:?}' from sysfs", self.uuid);
        self.path = PathBuf::from(MDEV_BASE);
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

        let mut persist_path = PathBuf::from(PERSIST_BASE);
        persist_path.push(self.parent.to_owned());
        persist_path.push(self.uuid.to_hyphenated().to_string());
        self.defined = persist_path.is_file();

        debug!("loaded device {:?}", self);
        Ok(())
    }

    pub fn load_from_json(&mut self, parent: String, json: &serde_json::Value) -> Result<()> {
        debug!(
            "Loading device '{:?}' from json (parent: {})",
            self.uuid, parent
        );
        self.defined = true;
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

        match json["attrs"].as_array() {
            Some(_) => {
                let attrarray = json["attrs"].as_array().unwrap();
                if !attrarray.is_empty() {
                    for attr in json["attrs"].as_array().unwrap() {
                        let attrobj = attr.as_object().unwrap();
                        for (key, val) in attrobj.iter() {
                            let valstr = val.as_str().unwrap();
                            self.attrs.insert(key.to_string(), valstr.to_string());
                        }
                    }
                }
            }
            _ => (),
        };
        debug!("loaded device {:?}", self);

        Ok(())
    }

    pub fn to_text(&self, fmt: &FormatType, verbose: bool) -> Result<String> {
        match fmt {
            FormatType::Defined => {
                if !self.defined {
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
                if self.defined {
                    output.push_str(" (defined)");
                }
            }
        }

        output.push('\n');
        if verbose && self.attrs.len() > 0 {
            let mut i = 0;
            output.push_str("  Attrs:\n");
            for (key, value) in &self.attrs {
                let txtattr = format!("    @{{{}}}: {{\"{}\":\"{}\"}}\n", i, key, value);
                output.push_str(&txtattr);
                i += 1;
            }
        }
        Ok(output)
    }

    pub fn to_json(&self) -> Result<serde_json::Value> {
        let autostart = match self.autostart {
            true => "auto",
            false => "manual",
        };
        let mut jsonattrs = serde_json::json!([]);
        if self.attrs.len() > 0 {
            for (key, value) in &self.attrs {
                let attr = serde_json::json!({ key: value });
                jsonattrs.as_array_mut().unwrap().push(attr);
            }
        }
        let jsonval: serde_json::Value = serde_json::json!({
            self.uuid.to_hyphenated().to_string(): {
                "mdev_type": self.mdev_type,
                "start": autostart,
                "attrs": jsonattrs
            }
        });

        Ok(jsonval)
    }
}

fn format_json(devices: BTreeMap<String, Vec<MdevInfo>>) -> Result<String> {
    let mut parents = serde_json::map::Map::new();
    for (parentname, children) in devices {
        let mut childrenarray = Vec::new();
        for child in children {
            childrenarray.push(child.to_json()?);
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

fn define_command(
    _uuid: Option<Uuid>,
    _auto: bool,
    _parent: Option<String>,
    _mdev_type: Option<String>,
    _jsonfile: Option<PathBuf>,
) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn undefine_command(_uuid: Uuid, _parent: Option<String>) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn modify_command(
    _uuid: Uuid,
    _parent: Option<String>,
    _mdev_type: Option<String>,
    _addattr: Option<String>,
    _delattr: bool,
    _index: Option<u32>,
    _value: Option<String>,
    _auto: bool,
    _manual: bool,
) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn start_command(
    _uuid: Option<Uuid>,
    _parent: Option<String>,
    _mdev_type: Option<String>,
    _jsonfile: Option<PathBuf>,
) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn stop_command(uuid: Uuid) -> Result<()> {
    debug!("Stopping '{}'", uuid);
    let mut info = MdevInfo::new(uuid);
    info.load_from_sysfs()?;
    let mut remove_path = PathBuf::from(info.path);
    remove_path.push("remove");
    debug!("remove path '{:?}'", remove_path);
    fs::write(remove_path, "1")?;
    Ok(())
}

fn list_command(
    defined: bool,
    dumpjson: bool,
    verbose: bool,
    uuid: Option<Uuid>,
    parent: Option<String>,
) -> Result<()> {
    let mut devices: BTreeMap<String, Vec<MdevInfo>> = BTreeMap::new();
    if defined {
        debug!("Looking up defined mdevs");
        for parentpath in PathBuf::from(PERSIST_BASE).read_dir()? {
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
                let mut dev = MdevInfo::new(u);
                dev.load_from_json(parentname.to_string(), &val)?;
                dev.load_from_sysfs()?;

                childdevices.push(dev);
            }
            devices.insert(parentname.to_string(), childdevices);
        }
    } else {
        debug!("Looking up active mdevs");
        for dev in PathBuf::from(MDEV_BASE).read_dir()? {
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

            let mut info = MdevInfo::new(u);
            match info.load_from_sysfs() {
                Ok(_) => {
                    if parent.is_some() {
                        match parent.as_ref() {
                            Some(p) => {
                                if p.as_ref() != info.parent {
                                    debug!(
                                        "Ignoring device {} because it doesn't match parent {}",
                                        info.uuid, p
                                    );
                                    continue;
                                }
                            }
                            None => (),
                        }
                    }

                    if !devices.contains_key(&info.parent) {
                        devices.insert(info.parent.clone(), Vec::new());
                    };

                    devices.get_mut(&info.parent).unwrap().push(info);
                }
                _ => (),
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

fn types_command(_parent: Option<String>, _dumpjson: bool) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    debug!("Starting up");
    let args = Cli::from_args();
    debug!("Parsed args");
    match args {
        Cli::Define {
            uuid,
            auto,
            parent,
            mdev_type,
            jsonfile,
        } => define_command(uuid, auto, parent, mdev_type, jsonfile),
        Cli::Undefine { uuid, parent } => undefine_command(uuid, parent),
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
            uuid, parent, mdev_type, addattr, delattr, index, value, auto, manual,
        ),
        Cli::Start {
            uuid,
            parent,
            mdev_type,
            jsonfile,
        } => start_command(uuid, parent, mdev_type, jsonfile),
        Cli::Stop { uuid } => stop_command(uuid),
        Cli::List {
            defined,
            dumpjson,
            verbose,
            uuid,
            parent,
        } => list_command(defined, dumpjson, verbose, uuid, parent),
        Cli::Types { parent, dumpjson } => types_command(parent, dumpjson),
    }
}
