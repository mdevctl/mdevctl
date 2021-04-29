use anyhow::{anyhow, ensure, Context, Result};
use log::{debug, warn};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::vec::Vec;
use structopt::StructOpt;
use uuid::Uuid;

use crate::cli::Cli;
use crate::environment::{DefaultEnvironment, Environment};
use crate::logger::logger;
use crate::mdev::*;

mod cli;
mod environment;
mod logger;
mod mdev;

#[cfg(test)]
mod tests;

fn format_json(devices: BTreeMap<String, Vec<MDev>>) -> Result<String> {
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

// convert 'define' command arguments into a MDev struct
fn define_command_helper(
    env: &dyn Environment,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<MDev> {
    let uuid_provided = uuid.is_some();
    let uuid = uuid.unwrap_or_else(Uuid::new_v4);
    let mut dev = MDev::new(env, uuid);

    if let Some(jsonfile) = jsonfile {
        let _ = std::fs::File::open(&jsonfile)
            .with_context(|| format!("Unable to read file {:?}", jsonfile));

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
            if parent.is_none() && (!dev.active || mdev_type.is_some()) {
                return Err(anyhow!("No parent specified"));
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
    env: &dyn Environment,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    debug!("Defining mdev {:?}", uuid);

    let dev = define_command_helper(env, uuid, auto, parent, mdev_type, jsonfile)?;
    dev.define().map(|_| {
        if uuid.is_none() {
            println!("{}", dev.uuid.to_hyphenated());
        }
    })
}

fn undefine_command(env: &dyn Environment, uuid: Uuid, parent: Option<String>) -> Result<()> {
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

#[allow(clippy::too_many_arguments)]
fn modify_command(
    env: &dyn Environment,
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

fn start_command_helper(
    env: &dyn Environment,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<MDev> {
    debug!("Starting device '{:?}'", uuid);
    let mut dev: Option<MDev> = None;
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

            let mut d = MDev::new(env, uuid.unwrap_or_else(Uuid::new_v4));
            d.load_from_json(parent.unwrap(), &val)?;
            dev = Some(d);
        }
        _ => {
            if mdev_type.is_some() && parent.is_none() {
                return Err(anyhow!("can't provide type without parent"));
            }

            /* The device is not fully specified without TYPE, we must find a config file, with optional
             * PARENT for disambiguation */
            if mdev_type.is_none() {
                if let Some(uuid) = uuid {
                    dev = match get_defined_device(env, uuid, &parent) {
                        Ok(d) => Some(d),
                        Err(e) => return Err(e),
                    };
                }
            }
            if uuid.is_none() && (parent.is_none() || mdev_type.is_none()) {
                return Err(anyhow!("Device is insufficiently specified"));
            }

            if dev.is_none() {
                let mut d = MDev::new(env, uuid.unwrap_or_else(Uuid::new_v4));
                d.parent = parent.unwrap();
                d.mdev_type = mdev_type.unwrap();
                dev = Some(d);
            }
        }
    }
    Ok(dev.unwrap())
}

fn start_command(
    env: &dyn Environment,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    let mut dev = start_command_helper(env, uuid, parent, mdev_type, jsonfile)?;
    dev.start(uuid.is_none())
}

fn stop_command(env: &dyn Environment, uuid: Uuid) -> Result<()> {
    debug!("Stopping '{}'", uuid);
    let mut dev = MDev::new(env, uuid);
    dev.load_from_sysfs()?;
    dev.stop()
}

fn get_defined_device<'a>(
    env: &'a dyn Environment,
    uuid: Uuid,
    parent: &Option<String>,
) -> Result<MDev<'a>> {
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
    env: &'a dyn Environment,
    uuid: &Option<Uuid>,
    parent: &Option<String>,
) -> Result<BTreeMap<String, Vec<MDev<'a>>>> {
    let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
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
            let mut dev = MDev::new(env, u);
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
    env: &dyn Environment,
    defined: bool,
    dumpjson: bool,
    verbose: bool,
    uuid: Option<Uuid>,
    parent: Option<String>,
) -> Result<()> {
    let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
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

            let mut dev = MDev::new(env, u);
            if dev.load_from_sysfs().is_ok() {
                if let Some(p) = parent.as_ref() {
                    if p != &dev.parent {
                        debug!(
                            "Ignoring device {} because it doesn't match parent {}",
                            dev.uuid, p
                        );
                        continue;
                    }
                }

                if !devices.contains_key(&dev.parent) {
                    devices.insert(dev.parent.clone(), Vec::new());
                };

                devices.get_mut(&dev.parent).unwrap().push(dev);
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
    env: &dyn Environment,
    parent: Option<String>,
) -> Result<BTreeMap<String, Vec<MDevType>>> {
    debug!("Finding supported mdev types");
    let mut types: BTreeMap<String, Vec<MDevType>> = BTreeMap::new();

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
                    .replace("\n", ", ")
                    .to_string();
            }

            childtypes.push(t);
        }
        types.insert(parentname.to_string(), childtypes);
    }
    Ok(types)
}

fn types_command(env: &dyn Environment, parent: Option<String>, dumpjson: bool) -> Result<()> {
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

fn start_parent_mdevs_command(env: &dyn Environment, parent: String) -> Result<()> {
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
    logger().init();
    debug!("Starting up");
    let args = Cli::from_args();
    let env = DefaultEnvironment::new();
    debug!("{:?}", env);
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
