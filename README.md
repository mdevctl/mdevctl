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

mdevctl is built with rust's `cargo` tool.  To build the executable, run `cargo
build`. This will compile the code and also generate a Makefile that can be
used for installing the executable and all support files into your system.  On
RPM based systems, you can run `make rpm` then install the resulting package.
Otherwise, run `make install`.

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
