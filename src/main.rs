//! mdevctl is a utility for managing and persisting devices in the mediated device framework of
//! the Linux kernel.  Mediated devices are sub-devices of a parent device (ex. a vGPU) which can
//! be dynamically created and potentially used by drivers like vfio-mdev for assignment to virtual
//! machines.
//!
//! See `mdevctl help` or the manpage for more information.

use anyhow::{anyhow, ensure, Context, Result};
use log::{debug, warn};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::vec::Vec;
use structopt::StructOpt;
use uuid::Uuid;

use crate::callouts::*;
use crate::cli::{LsmdevOptions, MdevctlCommands};
use crate::environment::{DefaultEnvironment, Environment};
use crate::logger::logger;
use crate::mdev::*;

mod callouts;
mod cli;
mod environment;
mod logger;
mod mdev;

#[cfg(test)]
mod tests;

/// Format a map of mediated devices into a json string
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

/// convert 'define' command arguments into a MDev struct
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

        let parent = parent
            .ok_or_else(|| anyhow!("Parent device required to define device via {:?}", jsonfile))?;

        let devs = defined_devices(env, Some(&uuid), Some(&parent))?;
        if !devs.is_empty() {
            return Err(anyhow!(
                "Cowardly refusing to overwrite existing config for {}/{}",
                parent,
                uuid.to_hyphenated().to_string()
            ));
        }

        let filecontents = fs::read_to_string(&jsonfile)
            .with_context(|| format!("Unable to read jsonfile {:?}", jsonfile))?;
        let jsonval = serde_json::from_str(&filecontents)?;
        dev.load_from_json(parent, &jsonval)?;
    } else {
        if uuid_provided {
            dev.load_from_sysfs()?;
            if parent.is_none() && (!dev.active || mdev_type.is_some()) {
                return Err(anyhow!("No parent specified"));
            }
        }

        dev.autostart = auto;
        if parent.is_some() {
            dev.parent = parent;
        }
        if mdev_type.is_some() {
            dev.mdev_type = mdev_type;
        }

        if dev.parent.is_none() {
            return Err(anyhow!("No parent specified"));
        }
        if dev.mdev_type.is_none() {
            return Err(anyhow!("No type specified"));
        }

        if dev.is_defined() {
            return Err(anyhow!(
                "Device {} on {} already defined",
                dev.uuid.to_hyphenated().to_string(),
                dev.parent()?
            ));
        }
    }

    Ok(dev)
}

/// Implementation of the `mdevctl define` command
fn define_command(
    env: &dyn Environment,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    debug!("Defining mdev {:?}", uuid);

    let mut dev = define_command_helper(env, uuid, auto, parent, mdev_type, jsonfile)?;

    Callout::invoke(&mut dev, Action::Define, |dev| dev.define()).map(|_| {
        if uuid.is_none() {
            println!("{}", dev.uuid.to_hyphenated());
        }
    })
}

/// Implementation of the `mdevctl undefine` command
fn undefine_command(env: &dyn Environment, uuid: Uuid, parent: Option<String>) -> Result<()> {
    debug!("Undefining mdev {:?}", uuid);
    let devs = defined_devices(env, Some(&uuid), parent.as_ref())?;
    if devs.is_empty() {
        return Err(anyhow!("No devices match the specified uuid"));
    }
    for (_, mut children) in devs {
        for mut child in children.iter_mut() {
            let _ = Callout::invoke(&mut child, Action::Undefine, |dev| dev.undefine());
        }
    }
    Ok(())
}

/// Implementation of the `mdevctl modify` command
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
    let mut dev = get_defined_device(env, uuid, parent.as_ref())?;
    if mdev_type.is_some() {
        dev.mdev_type = mdev_type;
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

    Callout::invoke(&mut dev, Action::Modify, |dev| dev.write_config())
}

/// convert 'start' command arguments into a MDev struct
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
            let val = serde_json::from_str(&contents)?;

            if mdev_type.is_some() {
                return Err(anyhow!(
                    "Device type cannot be specified separately from json file"
                ));
            }

            let parent = parent
                .ok_or_else(|| anyhow!("Parent device required to start device via json file"))?;

            let mut d = MDev::new(env, uuid.unwrap_or_else(Uuid::new_v4));
            d.load_from_json(parent, &val)?;
            dev = Some(d);
        }
        _ => {
            // if the user specified a uuid, check to see if they're referring to a defined device
            if uuid.is_some() {
                let devs = defined_devices(env, uuid.as_ref(), parent.as_ref())?;
                let n = devs.values().flatten().count();
                match n.cmp(&1) {
                    Ordering::Greater => {
                        return Err(anyhow!(
                            "Multiple definitions found for device {}. Please specify a parent.",
                            uuid.unwrap().to_hyphenated().to_string()
                        ));
                    }
                    Ordering::Equal => {
                        let d = devs.values().flatten().next();
                        if let Some(d) = d {
                            // See https://github.com/mdevctl/mdevctl/issues/38
                            // If a user specifies the uuid (and optional parent) of a defined device
                            if mdev_type.is_some() && mdev_type != d.mdev_type {
                                return Err(anyhow!(
                                    "Device {} already exists on parent {} with type {}",
                                    d.uuid.to_hyphenated().to_string(),
                                    d.parent().unwrap(),
                                    d.mdev_type.as_ref().unwrap()
                                ));
                            } else {
                                dev = Some(d.clone());
                            }
                        }
                    }
                    _ => (),
                }
            }

            if dev.is_none() {
                let mut d = MDev::new(env, uuid.unwrap_or_else(Uuid::new_v4));
                d.parent = parent;
                d.mdev_type = mdev_type;
                dev = Some(d);
            }

            if let Some(ref d) = dev {
                if d.mdev_type.is_some() && d.parent.is_none() {
                    return Err(anyhow!("can't provide type without parent"));
                }
                if d.mdev_type.is_none() || d.parent.is_none() {
                    return Err(anyhow!("Device is insufficiently specified"));
                }
            }
        }
    }
    dev.ok_or_else(|| anyhow!("Unknown error"))
}

/// Implementation of the `mdevctl start` command
fn start_command(
    env: &dyn Environment,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
) -> Result<()> {
    let mut dev = start_command_helper(env, uuid, parent, mdev_type, jsonfile)?;

    Callout::invoke(&mut dev, Action::Start, |dev| dev.start()).map(|_| {
        if uuid.is_none() {
            println!("{}", dev.uuid.to_hyphenated());
        }
    })
}

/// Implementation of the `mdevctl stop` command
fn stop_command(env: &dyn Environment, uuid: Uuid) -> Result<()> {
    debug!("Stopping '{}'", uuid);
    let mut dev = MDev::new(env, uuid);
    dev.load_from_sysfs()?;

    Callout::invoke(&mut dev, Action::Stop, |dev| dev.stop())
}

/// convenience function to lookup a defined device by uuid and parent
fn get_defined_device<'a>(
    env: &'a dyn Environment,
    uuid: Uuid,
    parent: Option<&String>,
) -> Result<MDev<'a>> {
    let devs = defined_devices(env, Some(&uuid), parent)?;
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

/// Get a map of all defined devices, optionally filtered by uuid and parent
fn defined_devices<'a>(
    env: &'a dyn Environment,
    uuid: Option<&Uuid>,
    parent: Option<&String>,
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
        if (parent.is_some() && parent.unwrap() != parentname) || !parentpath.metadata()?.is_dir() {
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

            let mut f = fs::File::open(path)?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)?;
            let val = serde_json::from_str(&contents)?;
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

/// Implementation of the `mdevctl list` command
fn list_command(
    env: &dyn Environment,
    defined: bool,
    dumpjson: bool,
    verbose: bool,
    uuid: Option<Uuid>,
    parent: Option<String>,
) -> Result<()> {
    let output = list_command_helper(env, defined, dumpjson, verbose, uuid, parent)?;
    println!("{}", output);
    Ok(())
}

/// convert 'list' command arguments into a text output
fn list_command_helper(
    env: &dyn Environment,
    defined: bool,
    dumpjson: bool,
    verbose: bool,
    uuid: Option<Uuid>,
    parent: Option<String>,
) -> Result<String> {
    let mut devices: BTreeMap<String, Vec<MDev>> = BTreeMap::new();
    if defined {
        devices = defined_devices(env, uuid.as_ref(), parent.as_ref())?;
    } else {
        debug!("Looking up active mdevs");
        if let Ok(dir) = env.mdev_base().read_dir() {
            for dev in dir {
                let dev = dev?;
                let fname = dev.file_name();
                let basename = fname.to_str().unwrap();
                debug!("found defined mdev {}", basename);
                let u = Uuid::parse_str(basename);

                if u.is_err() {
                    warn!("Can't determine uuid for file '{}'", basename);
                    continue;
                }
                let u = u.unwrap();

                if uuid.is_some() && uuid != Some(u) {
                    debug!(
                        "Ignoring device {} because it doesn't match uuid {}",
                        u,
                        uuid.unwrap()
                    );
                    continue;
                }

                let mut dev = MDev::new(env, u);
                if dev.load_from_sysfs().is_ok() {
                    if parent.is_some() && (parent != dev.parent) {
                        debug!(
                            "Ignoring device {} because it doesn't match parent {}",
                            dev.uuid,
                            parent.as_ref().unwrap()
                        );
                        continue;
                    }

                    let _ = dev.load_definition();

                    let devparent = dev.parent()?;
                    if !devices.contains_key(devparent) {
                        devices.insert(devparent.clone(), Vec::new());
                    };

                    devices.get_mut(devparent).unwrap().push(dev);
                };
            }
        }
    }

    // ensure that devices are sorted in a stable order
    for v in devices.values_mut() {
        v.sort_by_key(|e| e.uuid);
    }

    let output = match dumpjson {
        true => {
            // if specified to a single device, output such that it can be piped into a config
            // file, else print entire heirarchy
            if uuid.is_none() || devices.values().flatten().count() > 1 {
                format_json(devices)?
            } else {
                let jsonval = match devices.values().next() {
                    Some(children) => children
                        .first()
                        .ok_or_else(|| anyhow!("Failed to get device"))?
                        .to_json(false)?,
                    None => serde_json::json!([]),
                };
                serde_json::to_string_pretty(&jsonval)
                    .map_err(|_e| anyhow!("Unable to serialize json"))?
            }
        }
        false => {
            let ft = match defined {
                true => FormatType::Defined,
                false => FormatType::Active,
            };
            devices
                .values()
                // convert child vector into an iterator over the vector's elements
                .flat_map(|v| v.iter())
                // convert MDev elements to a text representation, filtering out errors
                .flat_map(|d| d.to_text(ft, verbose))
                .collect::<String>()
        }
    };
    Ok(output)
}

/// Get a map of all mediated device types that are supported on this machine
fn supported_types(
    env: &dyn Environment,
    parent: Option<String>,
) -> Result<BTreeMap<String, Vec<MDevType>>> {
    debug!("Finding supported mdev types");
    let mut types: BTreeMap<String, Vec<MDevType>> = BTreeMap::new();

    if let Ok(dir) = env.parent_base().read_dir() {
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
                        .replace("\n", ", ")
                        .to_string();
                }

                childtypes.push(t);
            }
            types.insert(parentname.to_string(), childtypes);
        }
    }
    for v in types.values_mut() {
        v.sort_by_key(|t| t.typename.clone());
    }
    Ok(types)
}

/// convert 'types' command arguments into a text output
fn types_command_helper(
    env: &dyn Environment,
    parent: Option<String>,
    dumpjson: bool,
) -> Result<String> {
    let types = supported_types(env, parent)?;
    let mut output = String::new();
    debug!("{:?}", types);
    if dumpjson {
        let mut parents = serde_json::map::Map::new();
        for (parent, children) in types {
            let mut childarray = Vec::new();
            for child in children {
                childarray.push(child.to_json()?);
            }
            parents.insert(parent, childarray.into());
        }

        let jsonval = match parents.len() {
            0 => serde_json::json!([]),
            _ => serde_json::json!([parents]),
        };
        let jsonstr = serde_json::to_string_pretty(&jsonval)
            .map_err(|_e| anyhow!("Unable to serialize json"))?;
        output.push_str(&jsonstr);
    } else {
        for (parent, children) in types {
            output.push_str(&format!("{}\n", parent));
            for child in children {
                output.push_str(&format!("  {}\n", child.typename));
                output.push_str(&format!(
                    "    Available instances: {}\n",
                    child.available_instances
                ));
                output.push_str(&format!("    Device API: {}\n", child.device_api));
                if !child.name.is_empty() {
                    output.push_str(&format!("    Name: {}\n", child.name));
                }
                if !child.description.is_empty() {
                    output.push_str(&format!("    Description: {}\n", child.description));
                }
            }
        }
    }
    Ok(output)
}

/// Implementation of the `mdevctl types` command
fn types_command(env: &dyn Environment, parent: Option<String>, dumpjson: bool) -> Result<()> {
    let output = types_command_helper(env, parent, dumpjson)?;
    println!("{}", output);
    Ok(())
}

/// Implementation of the `start-parent-mdevs` command
fn start_parent_mdevs_command(env: &dyn Environment, parent: String) -> Result<()> {
    let mut devs = defined_devices(env, None, Some(&parent))?;
    if devs.is_empty() {
        // nothing to do
        return Ok(());
    }

    ensure!(devs.len() == 1, "More than one parent found");

    for (_, children) in devs.iter_mut() {
        for child in children {
            if child.autostart {
                debug!("Autostarting {:?}", child.uuid);
                if let Err(e) = Callout::invoke(child, Action::Start, |child| child.start()) {
                    for x in e.chain() {
                        warn!("{}", x);
                    }
                }
            }
        }
    }
    Ok(())
}

/// parse command line arguments and dispatch to command-specific functions
fn main() -> Result<()> {
    logger().init();
    debug!("Starting up");

    let env = DefaultEnvironment::new();
    debug!("{:?}", env);
    // check if we're running as the symlink executable 'lsmdev'. If so, just execute the 'list'
    // command directly
    let exe = std::env::args_os().next().unwrap();
    match exe.to_str() {
        Some(val) if val.ends_with("lsmdev") => {
            debug!("running as 'lsmdev'");
            let opts = LsmdevOptions::from_args();
            list_command(
                &env,
                opts.defined,
                opts.dumpjson,
                opts.verbose,
                opts.uuid,
                opts.parent,
            )
        }
        _ => match MdevctlCommands::from_args() {
            MdevctlCommands::Define {
                uuid,
                auto,
                parent,
                mdev_type,
                jsonfile,
            } => define_command(&env, uuid, auto, parent, mdev_type, jsonfile),
            MdevctlCommands::Undefine { uuid, parent } => undefine_command(&env, uuid, parent),
            MdevctlCommands::Modify {
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
            MdevctlCommands::Start {
                uuid,
                parent,
                mdev_type,
                jsonfile,
            } => start_command(&env, uuid, parent, mdev_type, jsonfile),
            MdevctlCommands::Stop { uuid } => stop_command(&env, uuid),
            MdevctlCommands::List(list) => list_command(
                &env,
                list.defined,
                list.dumpjson,
                list.verbose,
                list.uuid,
                list.parent,
            ),
            MdevctlCommands::Types { parent, dumpjson } => types_command(&env, parent, dumpjson),
            MdevctlCommands::StartParentMdevs { parent } => {
                start_parent_mdevs_command(&env, parent)
            }
        },
    }
}
