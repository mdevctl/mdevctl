use std::path::PathBuf;
pub use structopt::StructOpt;
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
pub enum Cli {
    #[structopt(
        about = "Define a persistent mediated device",
        long_about = "Define a persistent mediated device\n\n\
                If the device specified by the UUID currently exists, 'parent' and 'type' may be \
                omitted to use the existing values. The 'auto' option marks the device to start on \
                parent availability.  If defined via 'jsonfile', then 'type', 'startup', and any \
                attributes are provided via the file.\n\n\
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
                If a UUID exists for multiple parents, all will be removed unless a parent is
                specified. \n\n\
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
                The 'parent' option further identifies a UUID if it is not unique. The parent for a \
                device cannot be modified via this command; undefine and re-define should be used \
                instead. An attribute can be added or removed, which correlates to a sysfs \
                attribute under the created device. Unless an 'index' value is provided, operations \
                are performed at the end of the attribute list. 'value' is to be specified in the \
                format that is accepted by the attribute. Upon device start, mdevctl will go \
                through each attribute in order, writing the value into the corresponding sysfs \
                attribute for the device. The startup mode of the device can also be selected, auto \
                or manual. \n\n\
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
                If the UUID is previously defined and unique, the UUID is sufficient to start the \
                device (UUIDs may not collide between running devices). If a UUID is used in \
                multiple defined configurations, the 'parent' is necessary to identify the device \
                to be started.  When specified with 'parent' and 'type', the device is fully \
                specified and will be started based only on these parameters.  The UUID is optional \
                in this case. If not provided, a UUID is generated and returned as output. A \
                'jsonfile' may replace the 'type' specification and also include additional \
                attributes to be applied to the started device."
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
                With no options, information about the currently running mediated devices is \
                provided. Specifying 'defined' lists the configuration of defined devices, \
                regardless of their running state. This may be further reduced by specifying \
                specific 'uuid' or 'parent' devices to list. The 'dumpjson' option provides output \
                listing in machine readable JSON format. When a 'uuid' option is provided and the \
                result is a single device, the output contains only the JSON fields necessary to \
                recreate a config file for the device (minus attributes for listings of running \
                devices). When the verbose option is provided, the human readable listing will \
                include attributes for the device(s)."
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
                Specifying a 'parent' lists only the types provided by the given parent device. \
                The 'dumpjson' option provides output in machine readable JSON format."
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
