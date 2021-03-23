use anyhow::{anyhow, Result};
use log::{debug, warn};
use std::fs;
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
        #[structopt(short, long)]
        r#type: Option<String>,
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
        #[structopt(short, long)]
        r#type: Option<String>,
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
        #[structopt(short, long)]
        r#type: Option<String>,
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

#[derive(Debug)]
struct MdevInfo {
    uuid: Uuid,
    active: bool,
    defined: bool,
    autostart: bool,
    path: PathBuf,
    parent: String,
    mdev_type: String,
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
}

fn define_command(
    _uuid: Option<Uuid>,
    _auto: bool,
    _parent: Option<String>,
    r#_type: Option<String>,
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
    r#_type: Option<String>,
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
    r#_type: Option<String>,
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
    _defined: bool,
    _dumpjson: bool,
    _verbose: bool,
    _uuid: Option<Uuid>,
    _parent: Option<String>,
) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn types_command(_parent: Option<String>, _dumpjson: bool) -> Result<()> {
    return Err(anyhow!("Not implemented"));
}

fn main() -> Result<()> {
    env_logger::init();
    debug!("Starting up");
    let args = Cli::from_args();
    debug!("Parsed args");
    match args {
        Cli::Define {
            uuid,
            auto,
            parent,
            r#type,
            jsonfile,
        } => define_command(uuid, auto, parent, r#type, jsonfile),
        Cli::Undefine { uuid, parent } => undefine_command(uuid, parent),
        Cli::Modify {
            uuid,
            parent,
            r#type,
            addattr,
            delattr,
            index,
            value,
            auto,
            manual,
        } => modify_command(
            uuid, parent, r#type, addattr, delattr, index, value, auto, manual,
        ),
        Cli::Start {
            uuid,
            parent,
            r#type,
            jsonfile,
        } => start_command(uuid, parent, r#type, jsonfile),
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
