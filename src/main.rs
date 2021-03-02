use anyhow::{anyhow, Result};
use log::debug;
use std::path::PathBuf;
use structopt::StructOpt;
use uuid::Uuid;

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

fn stop_command(_uuid: Uuid) -> Result<()> {
    return Err(anyhow!("Not implemented"));
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
