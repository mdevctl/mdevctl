# mdevctl - a mediated device management utility for Linux

## Description

mdevctl is a utility for managing and persisting devices in the
mediated device device framework of the Linux kernel.  Mediated
devices are sub-devices of a parent device (ex. a vGPU) which
can be dynamically created and potentially used by drivers like
vfio-mdev for assignment to virtual machines.

## License

Licensed under the GNU Lesser General Public License aka LGPL v2.1.
See [COPYING](COPYING) for details.

## Source repository

https://github.com/mdevctl/mdevctl

## Installation

On RPM based systems, `make rpm` then install the resulting package.
Otherwise, `make install`

## Architecture

mdevctl stores defined mediated devices in /etc/mdevctl.d/ with
directories matching the parent device name and config files named
by the UUID of the mdev device itself.  The format used is JSON; a
configuration file for an mdev device looks like follows:

```
  {
   "mdev_type": "$VENDOR_TYPE",
   "start": "auto|manual",
   "attrs": [
    ...optional list of device-specific attributes...
   ]
  }
```

When a known parent device add udev event occurs (or, for more recent
kernels, change events with MDEV_STATE values), mdevctl is called by
a udev rule to create defined devices with "start": "auto" configured.

mdevctl defines three classes of commands, those that manage device
config files, those that manage the device itself, and listing
commands for showing defined, active, or potential mdev devices.

Starting with the latter, mdevctl is able to manage mdev devices
created either with mdevctl or externally, such as through direct
sysfs interactions.  Likewise, when generating a list of currently
active mdev devices via the `list` command, all mdevs are included.
When provided with the `--defined` option, the list command will show
mdev device configs defined on the system, regardless of whether they
are currently active.  The `types` command provides details of the
mdev types supported on the system, including the number of
instances of each that may be created, the API exposed for each, as
well as vendor provided name and description as available.

Mediated device definitions can be created with the `define` command,
which not only accepts a fully specified configuration via options,
but can also create a config for a currently running mdev.  Thus a
transient device created either through mdevctl or sysfs can be
promoted to a defined device.  The `undefine` command simply removes
the config definition without modifying the running device, while
the `modify` command allows device config to be modified.  Config
modifications to a running device to not take effect until the device
is stopped and restarted.

This leads to the final class of commands, which provides the `start`
and `stop` functionality.  The start command can operate either on
a previously defined mdev or the mdev can be fully specified via
options to create a transient device, ie. a running device with no
persistence.

# Usage

List running mdev devices:

```
# mdevctl list
85006552-1b4b-45ef-ad62-de05be9171df 0000:00:02.0 i915-GVTg_V4_4
83c32df7-d52e-4ec1-9668-1f3c7e4df107 0000:00:02.0 i915-GVTg_V4_8 (defined)
```

List defined mdev devices:

```
# mdevctl list -d
83c32df7-d52e-4ec1-9668-1f3c7e4df107 0000:00:02.0 i915-GVTg_V4_8 auto
b0a3989f-8138-4d49-b63a-59db28ec8b48 0000:00:02.0 i915-GVTg_V4_8 auto
5cf14a12-a437-4c82-a13f-70e945782d7b 0000:00:02.0 i915-GVTg_V4_4 manual
```

List mdev types supported on the host system:

```
# mdevctl types
0000:00:02.0
  i915-GVTg_V4_2
    Available instances: 1
    Device API: vfio-pci
    Description: low_gm_size: 256MB high_gm_size: 1024MB fence: 4 resolution: 1920x1200 weight: 8 
  i915-GVTg_V4_1
    Available instances: 0
    Device API: vfio-pci
    Description: low_gm_size: 512MB high_gm_size: 2048MB fence: 4 resolution: 1920x1200 weight: 16 
  i915-GVTg_V4_8
    Available instances: 4
    Device API: vfio-pci
    Description: low_gm_size: 64MB high_gm_size: 384MB fence: 4 resolution: 1024x768 weight: 2 
  i915-GVTg_V4_4
    Available instances: 3
    Device API: vfio-pci
    Description: low_gm_size: 128MB high_gm_size: 512MB fence: 4 resolution: 1920x1200 weight: 4 
```

Modify a defined device from automatic start to manual:

```
# mdevctl modify --uuid 83c32df7-d52e-4ec1-9668-1f3c7e4df107 --manual
# mdevctl list -d
83c32df7-d52e-4ec1-9668-1f3c7e4df107 0000:00:02.0 i915-GVTg_V4_8 manual
b0a3989f-8138-4d49-b63a-59db28ec8b48 0000:00:02.0 i915-GVTg_V4_8 auto
5cf14a12-a437-4c82-a13f-70e945782d7b 0000:00:02.0 i915-GVTg_V4_4 manual
```

Stop a running mdev device:

```
# mdevctl stop -u 83c32df7-d52e-4ec1-9668-1f3c7e4df107
```

Start an mdev device that is not defined

```
# uuidgen
6eba5b41-176e-40db-b93e-7f18e04e0b93
# mdevctl start -u 6eba5b41-176e-40db-b93e-7f18e04e0b93 -p 0000:00:02.0 --type i915-GVTg_V4_1
# mdevctl list
85006552-1b4b-45ef-ad62-de05be9171df 0000:00:02.0 i915-GVTg_V4_4
6eba5b41-176e-40db-b93e-7f18e04e0b93 0000:00:02.0 i915-GVTg_V4_1
```

Promote the new created mdev to a defined device:

```
# mdevctl define --uuid 6eba5b41-176e-40db-b93e-7f18e04e0b93
# mdevctl list -d
83c32df7-d52e-4ec1-9668-1f3c7e4df107 0000:00:02.0 i915-GVTg_V4_8 manual
6eba5b41-176e-40db-b93e-7f18e04e0b93 0000:00:02.0 i915-GVTg_V4_1 manual
b0a3989f-8138-4d49-b63a-59db28ec8b48 0000:00:02.0 i915-GVTg_V4_8 auto
5cf14a12-a437-4c82-a13f-70e945782d7b 0000:00:02.0 i915-GVTg_V4_4 manual
```

## Advanced usage (attributes and JSON)

mdevctl provides support for specifying additional configuration via
device-specific attributes. It also provides support for inspecting
and modifying its internal JSON representation of the configuration
directly.

Example:

```
# mdevctl list -d
783e6dbb-ea0e-411f-94e2-717eaad438bf matrix vfio_ap-passthrough manual
```

Add some attributes:

```
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_adapter --value=5
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_adapter --value=6
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_domain --value=0xab
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_control_domain --value=0xab
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_domain --value=4
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --addattr=assign_control_domain --value=4
# mdevctl list -dv
783e6dbb-ea0e-411f-94e2-717eaad438bf matrix vfio_ap-passthrough manual
  Attrs:
    @{0}: {"assign_adapter":"5"}
    @{1}: {"assign_adapter":"6"}
    @{2}: {"assign_domain":"0xab"}
    @{3}: {"assign_control_domain":"0xab"}
    @{4}: {"assign_domain":"4"}
    @{5}: {"assign_control_domain":"4"}
```

Dump the JSON configuration:

```
# mdevctl list -d -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --dumpjson
{
  "mdev_type": "vfio_ap-passthrough",
  "start": "manual",
  "attrs": [
    {
      "assign_adapter": "5"
    },
    {
      "assign_adapter": "6"
    },
    {
      "assign_domain": "0xab"
    },
    {
      "assign_control_domain": "0xab"
    },
    {
      "assign_domain": "4"
    },
    {
      "assign_control_domain": "4"
    }
  ]
}
```

Remove some attributes:

```
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --delattr --index=5
# mdevctl modify -u 783e6dbb-ea0e-411f-94e2-717eaad438bf --delattr --index=4
# mdevctl list -dv
783e6dbb-ea0e-411f-94e2-717eaad438bf matrix vfio_ap-passthrough manual
  Attrs:
    @{0}: {"assign_adapter":"5"}
    @{1}: {"assign_adapter":"6"}
    @{2}: {"assign_domain":"0xab"}
    @{3}: {"assign_control_domain":"0xab"}
```

Define an mdev device from a file:

```
# cat vfio_ap_device.json
{
  "mdev_type": "vfio_ap-passthrough",
  "start": "manual",
  "attrs": [
    {
      "assign_adapter": "5"
    },
    {
      "assign_domain": "0x47"
    },
    {
      "assign_domain": "0xff"
    }
  ]
}
# mdevctl define -p matrix --jsonfile vfio_ap_device.json
e2e73122-cc39-40ee-89eb-b0a47d334cae
# mdevctl list -dv
783e6dbb-ea0e-411f-94e2-717eaad438bf matrix vfio_ap-passthrough manual
  Attrs:
    @{0}: {"assign_adapter":"5"}
    @{1}: {"assign_adapter":"6"}
    @{2}: {"assign_domain":"0xab"}
    @{3}: {"assign_control_domain":"0xab"}
e2e73122-cc39-40ee-89eb-b0a47d334cae matrix vfio_ap-passthrough manual
  Attrs:
    @{0}: {"assign_adapter":"5"}
    @{1}: {"assign_domain":"0x47"}
    @{2}: {"assign_domain":"0xff"}
```

See `mdevctl --help` or the manpage for more information.

# Invoking External Scripts for Device Events

Certain mediated devices may require additional operations or functionality,
such as configuration checking or event reporting, before or after mdevctl
executes a command. In order to remain device-type agnostic, mdevctl will
"call-out" to external scripts to handle the extraneous work. These scripts
are associated with a specific device type to perform any operations or
additional functionality not handled within mdevctl. Additionally, external
programs may wish to receive notifications of any action performed by mdevctl
to e.g. respond to any device changes or keep device management in parallel.

A call-out script is invoked at various points during an mdevctl command
process and are categorized by an "event" paired with an "action", along
with information regarding the mediated device. Two main event types are
"pre" and "post", which are invoked before and after primary command
execution respectively. The same call-out script is invoked for both events.

A "notify" event script is invoked to report the status of mdevctl's
command results. This may be used to signal external programs of changes
made to a mediated device, or simply to assist with debugging efforts.

A "get" event script is invoked for the define and list commands to acquire
device attributes from sysfs.

Essentially, the procedure in mdevctl looks like this:

1. command-line parsing & setup
2. invoke pre-command call-out
3. primary command execution (e.g. start mdev / write device config)\*
4. invoke post-command call-out\*
5. invoke notifier\*

\* step is skipped if 2 fails.

## Script Parameters

Each call-out and notifier script is invoked by mdevctl with the following
parameters:

**-e EVENT**
: denotes the specific call-out or notification event
- "pre" for pre-command call-out
- "post" for post-command call-out
- "notify" for notification scripts
- "get" for acquiring device data

**-a ACTION**
: denotes what specific action the script is asked to do
- for pre/post/notify events, this will be synonymous with an mdevctl command
 (e.g. define, start)
- for a get events, this will be "attributes"

**-s STATE**
: a trinary value defining the current state of mdevctl's command
execution
- none: mdevctl has yet to execute the command, or the pre-command
call-out failed
- success: mdevctl has completed the command successfully
- failure: mdevctl has completed the command with an error

**-u UUID & -p PARENT**
: UUID and parent of the device.

**stdin**
: as standard input, the device's JSON configuration will be provided. This may
represent:
- a persistent device config created by the define command
- a transient device config representing a sysfs device
- an "in progress" config imposed by the modify command
- a device config passed by the `--jsonfile` parameter.

A script does not need to handle every action. As such, the script
should take care to ignore any unrecognized / unsupported actions
and allow mdevctl to carry on as normal.

## Pre-command Event

A pre-command event is invoked by mdevctl after any command-line parsing and
setup, but before a command's execution (such as prior to writing the
persistent device configuration, or prior to starting a device). For example,
a script for a vfio_ap-passthrough device may check if the requested matrix is
already in use before mdevctl can start the device.

Errors reported by pre-command event scripts are disruptive. If a script
reports an error, then mdevctl will exit early with an appropriate message
and the command execution will not be performed (e.g. the device will not be
defined or started). A notifier event will still be invoked in this case.

This call-out will invoke a script with the "pre" `EVENT`, an `ACTION`
reflecting the mdevctl command, a "none" `STATUS`, and a `UUID` and `PARENT`
of the device.

```
echo $config | $script -e "pre" -a "start" -s "none" -u "f0bb71ac-9b72-4d2d-bbdb-67f41d3cd26e" -p "matrix"
```

## Post-command Event

A post-command event is invoked by mdevctl after a command has executed (such
as after a device configuration is written, or after a device has been started)
to perform any additional steps that may be required for a device
configuration. *The same script invoked for the pre-command call-out is also
invoked for this call-out.*

Errors reported by scripts during a post-command event are non-disruptive.

This call-out will invoke a script with the "post" `EVENT`, an `ACTION`
reflecting the mdevctl command, a `STATUS` reflecting the success/failure
of mdevctl's primary command execution,and a `UUID` and `PARENT` of the
device.

```
echo $config | $script -e "post" -a "start" -s "success" -u "f0bb71ac-9b72-4d2d-bbdb-67f41d3cd26e" -p "matrix"
```

## Notifier Events

Notifier events are invoked by mdevctl to convey information to external
listeners. For example, a script may signal to another program that an mdevctl
command has executed successfully. A notifier always follows either a
pre or post-command event.

Errors reported by scripts during a notifier event are non-disruptive.

A notifier call-out will invoke a script with the "notify" `EVENT`, an `ACTION`
reflecting the mdevctl command, a `STATUS` reflecting the success/failure
of mdevctl's primary command execution, or "none" if the pre-command
call-out failed, and a `UUID` and `PARENT` of the device.

```
echo $config | $script -e "notify" -a "start" -s "success" -u "f0bb71ac-9b72-4d2d-bbdb-67f41d3cd26e" -p "matrix"
```

## Get Attributes Event

Get events are invoked to the script as a "get" `EVENT` to request extraneous
data from the mediated device in the case where mdevctl cannot easily acquire
them (e.g. from a sysfs device, or an active mdev started by mdevctl). This
event is always paired with the "attributes" `ACTION`.

This call-out is made during the define and list commands. For define,
this will acquire the device attributes when creating a device configuration
by providing the UUID for an existing device that was not previously defined
by mdevctl. When list is provided the `--dumpjson` option, this will acquire
the device attributes when providing an active mdev that is queried from sysfs.

A get call-out will invoke a script with the "get" `EVENT`, an "attributes"
`ACTION`, a "none" `STATUS`, and a `UUID` and `PARENT` of the device.

```
echo $config | $script -e "get" -a "attributes" -s "none" -u "f0bb71ac-9b72-4d2d-bbdb-67f41d3cd26e" -p "matrix"
```

The expected output from the script is a JSON formatted array of device
attributes that may be easily plugged into the device configuration:

```
[
    {
        "assign_adapter": "2"
    },
    {
        "assign_domain": "0x3b"
    }
]
```

## Auto Start

It is worth mentioning that for auto start devices, a pre/post call-out and
notifier is invoked for each device. The parameters are the same as for a
start command. The pre-command event is non-disruptive in this case as
to allow mdevctl to attempt each device.

All errors reported by the pre/post events are redirected to systemd.

Note: if a notification script is used to convey information to another
program or daemon, it is not guaranteed that the program will be started
prior to mdevctl's invocation.

## Script Installation

For pre/post/get events, it may be the case that a script can be used to
satisfy various device types. As such, a special "locator" script is required
for mdevctl to find the appropriate script to execute. The locator script must
accept a device type as a parameter and return via standard output either a
valid path to the script or an empty string if the device type is not
supported.

For pre/post events, the locator scripts must reside within
`/etc/mdevctl/callouts/command.d`, and for get events the locator scripts
must reside within `/etc/mdevctl/callouts/get.d`. mdevctl will execute all
locator scripts in the respective directory until either a valid path is
returned or all scripts have returned an empty path.

It is the responsibility of the call-out script to install its locator script
to the appropriate directory.

An example execution of a locator script:
`/etc/mdevctl/callouts/command.d/ap_command.sh -t vfio_ap-passthrough`

An example output from the locator script:
`/usr/sbin/ap_check`

Notifier event scripts must reside within
`/etc/mdevctl/notification/notifiers.d/`. This event type does not require a
locator script, and all scripts within this directory will be executed
regardless of device type.
