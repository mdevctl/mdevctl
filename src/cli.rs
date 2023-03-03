//! Command line options for mdevctl

pub use clap::Parser;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[clap(version, about = "List mediated devices", name = "lsmdev")]
pub struct LsmdevOptions {
    #[clap(short, long, help = "Show defined devices")]
    pub defined: bool,
    #[clap(long, help = "Output device list in json format")]
    pub dumpjson: bool,
    #[clap(short, long, help = "Print additional information about the devices")]
    pub verbose: bool,
    #[clap(short, long, help = "List devices matching the specified UUID")]
    pub uuid: Option<Uuid>,
    #[clap(
        short,
        long,
        help = "List devices associated with the specified Parent device"
    )]
    pub parent: Option<String>,
}

// command-line argument definitions.
#[derive(Parser)]
#[clap(version, about = "A mediated device management utility for Linux")]
pub enum MdevctlCommands {
    #[clap(
        about = "Define a persistent mediated device",
        long_about = "Define a persistent mediated device\n\n\
                If the device specified by the UUID currently exists, 'parent' and 'type' may be \
                omitted to use the existing values. The 'auto' option marks the device to start on \
                parent availability.  If defined via 'jsonfile', then 'type', 'startup', and any \
                attributes are provided via the file.\n\n\
                Running devices are unaffected by this command."
    )]
    Define {
        #[clap(
            short,
            long,
            required_unless_present("parent"),
            help = "Assign UUID to the device"
        )]
        uuid: Option<Uuid>,
        #[clap(
            short,
            long,
            help = "Automatically start device on parent availability"
        )]
        auto: bool,
        #[clap(
            short,
            long,
            required_unless_present("uuid"),
            help = "Specify the parent of the device"
        )]
        parent: Option<String>,
        #[clap(
            name = "type",
            short,
            long,
            help = "Specify the mdev type of the device"
        )]
        mdev_type: Option<String>,
        #[clap(
            long, value_parser,
            conflicts_with_all(&["type", "auto"]),
            help = "Specify device details in JSON format"
        )]
        jsonfile: Option<PathBuf>,
    },

    #[clap(
        about = "Undefine a persistent mediated device",
        long_about = "Undefine, or remove a config for an mdev device\n\n\
                If a UUID exists for multiple parents, all will be removed unless a parent is
                specified. \n\n\
                Running devices are unaffected by this command."
    )]
    Undefine {
        #[clap(short, long, help = "UUID of the device to be undefined")]
        uuid: Uuid,
        #[clap(short, long, help = "Parent of the device to be undefined")]
        parent: Option<String>,
        #[clap(
            short,
            long,
            help = "Override a decline by a callout script to undefine"
        )]
        force: bool,
    },

    #[clap(
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
                or manual. Alternatively, the 'jsonfile' option may be used to replace the startup \
                mode and any attributes with the contents of the specified file.\n\n\
                Running devices are unaffected by this command."
    )]
    Modify {
        #[clap(short, long, help = "UUID of the mdev to modify")]
        uuid: Uuid,
        #[clap(short, long, help = "Parent of the mdev to modify")]
        parent: Option<String>,
        #[clap(
            name = "type",
            short,
            long,
            help = "Modify the mdev type for this device"
        )]
        mdev_type: Option<String>,
        #[clap(
            long,
            conflicts_with("delattr"),
            requires("value"),
            help = "add a new attribute",
            value_name = "attr_name"
        )]
        addattr: Option<String>,
        #[clap(long, help = "Delete an attribute")]
        delattr: bool,
        #[clap(long, short, help = "Index of the attribute to modify")]
        index: Option<u32>,
        #[clap(
            long,
            help = "Value for the attribute specified by --addattr",
            value_name = "attr_value"
        )]
        value: Option<String>,
        #[clap(short, long, help = "Device will be started automatically")]
        auto: bool,
        #[clap(
            short,
            long,
            conflicts_with("auto"),
            help = "Device must be started manually"
        )]
        manual: bool,
        #[clap(
            long, value_parser,
            conflicts_with_all(&["type", "addattr", "delattr", "index", "value", "auto", "manual"]),
            help = "Specify device details in JSON format"
        )]
        jsonfile: Option<PathBuf>,
    },
    #[clap(
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
        #[clap(
            short,
            long,
            required_unless_present("parent"),
            help = "UUID of the device to start"
        )]
        uuid: Option<Uuid>,
        #[clap(
            short,
            long,
            required_unless_present("uuid"),
            help = "Parent of the device to start"
        )]
        parent: Option<String>,
        #[clap(name = "type", short, long, help = "Mdev type of the device to start")]
        mdev_type: Option<String>,
        #[clap(
            long,
            value_parser,
            conflicts_with("type"),
            help = "Details of the device to be started, in JSON format"
        )]
        jsonfile: Option<PathBuf>,
    },
    #[clap(about = "Stop a mediated device")]
    Stop {
        #[clap(short, long, help = "UUID of the device to stop")]
        uuid: Uuid,
    },
    #[clap(
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
    List(LsmdevOptions),
    #[clap(
        about = "List available mediated device types",
        long_about = "List available mediated device types\n\n\
                Specifying a 'parent' lists only the types provided by the given parent device. \
                The 'dumpjson' option provides output in machine readable JSON format."
    )]
    Types {
        #[clap(short, long, help = "Show supported types for the specified parent")]
        parent: Option<String>,
        #[clap(long, help = "Output mdev types list in JSON format")]
        dumpjson: bool,
    },
    #[clap(hide = true)]
    StartParentMdevs { parent: String },
}

#[test]
fn test_cli() {
    use clap::CommandFactory;
    MdevctlCommands::command().debug_assert()
}
