use anyhow::Result;
use log::info;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::Builder;
use tempfile::TempDir;
use uuid::Uuid;

use crate::environment::Environment;
use crate::logger::logger;
use crate::mdev::MDev;

const TEST_DATA_DIR: &str = "tests";

fn init() {
    let _ = logger().is_test(true).try_init();
}

#[derive(PartialEq)]
enum Expect {
    Pass,
    Fail,
}

#[derive(Debug)]
struct TestEnvironment {
    datapath: PathBuf,
    scratch: TempDir,
}

impl Environment for TestEnvironment {
    fn root(&self) -> &Path {
        self.scratch.path()
    }
}

impl TestEnvironment {
    pub fn new(testname: &str, testcase: &str) -> TestEnvironment {
        let path: PathBuf = [TEST_DATA_DIR, testname].iter().collect();
        let scratchdir = Builder::new().prefix("mdevctl-test").tempdir().unwrap();
        let test = TestEnvironment {
            datapath: path,
            scratch: scratchdir,
        };
        // populate the basic directories in the environment
        fs::create_dir_all(test.mdev_base()).expect("Unable to create mdev_base");
        fs::create_dir_all(test.persist_base()).expect("Unable to create persist_base");
        fs::create_dir_all(test.parent_base()).expect("Unable to create parent_base");
        fs::create_dir_all(test.callout_script_base())
            .expect("Unable to create callout_script_base");
        fs::create_dir_all(test.callout_notification_base())
            .expect("Unable to create callout_notification_base");
        info!("---- Running test '{}/{}' ----", testname, testcase);
        test
    }

    // set up a few files in the test environment to simulate an defined mediated device
    fn populate_defined_device(&self, uuid: &str, parent: &str, filename: &str) {
        let jsonfile = self.datapath.join(filename);
        let parentdir = self.persist_base().join(parent);
        fs::create_dir_all(&parentdir).expect("Unable to setup parent dir");
        let deffile = parentdir.join(uuid);
        assert!(jsonfile.exists());
        assert!(!deffile.exists());
        fs::copy(jsonfile, deffile).expect("Unable to copy device def");
    }

    // set up a few files in the test environment to simulate an active mediated device
    fn populate_active_device(&self, uuid: &str, parent: &str, mdev_type: &str) {
        use std::os::unix::fs::symlink;

        let (parentdir, parenttypedir) =
            self.populate_parent_device(parent, mdev_type, 1, "", "", None);

        let parentdevdir = parentdir.join(uuid);
        fs::create_dir_all(&parentdevdir).expect("Unable to setup parent device dir");

        let devdir = self.mdev_base().join(uuid);
        fs::create_dir_all(&devdir.parent().unwrap()).expect("Unable to setup mdev dir");
        symlink(&parentdevdir, &devdir).expect("Unable to setup mdev dir");

        let typefile = devdir.join("mdev_type");
        symlink(&parenttypedir, &typefile).expect("Unable to setup mdev type");
    }

    // set up a few files in the test environment to simulate a parent device that supports
    // mediated devices
    fn populate_parent_device(
        &self,
        parent: &str,
        supported_type: &str,
        instances: i32,
        device_api: &str,
        name: &str,
        description: Option<&str>,
    ) -> (PathBuf, PathBuf) {
        let parentdir = self.parent_base().join(parent);
        let parenttypedir = parentdir.join("mdev_supported_types").join(supported_type);
        fs::create_dir_all(&parenttypedir).expect("Unable to setup mdev parent type");

        let instancefile = parenttypedir.join("available_instances");
        fs::write(instancefile, format!("{}", instances))
            .expect("Unable to write available_instances");

        let apifile = parenttypedir.join("device_api");
        fs::write(apifile, format!("{}", device_api)).expect("Unable to write device_api");

        let namefile = parenttypedir.join("name");
        fs::write(namefile, format!("{}", name)).expect("Unable to write name");

        if let Some(desc) = description {
            let descfile = parenttypedir.join("description");
            fs::write(descfile, format!("{}", desc)).expect("Unable to write description");
        }

        (parentdir, parenttypedir)
    }

    fn compare_to_file(&self, filename: &str, actual: &str) {
        let path = self.datapath.join(filename);
        let flag = get_flag(REGEN_FLAG);
        if flag {
            regen(&path, actual).expect("Failed to regenerate expected output");
        }
        let expected = fs::read_to_string(path).unwrap_or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                println!(
                    "File {:?} not found, run tests with {}=1 to automatically \
                         generate expected output",
                    filename, REGEN_FLAG
                );
            }
            Default::default()
        });

        assert_eq!(expected, actual);
    }

    fn load_from_json<'a>(&'a self, uuid: &str, parent: &str, filename: &str) -> Result<MDev<'a>> {
        let path = self.datapath.join(filename);
        let uuid = Uuid::parse_str(uuid);
        assert!(uuid.is_ok());
        let uuid = uuid.unwrap();
        let mut dev = MDev::new(self, uuid);

        let jsonstr = fs::read_to_string(path)?;
        let jsonval: serde_json::Value = serde_json::from_str(&jsonstr)?;
        dev.load_from_json(parent.to_string(), &jsonval)?;

        Ok(dev)
    }
}

fn get_flag(varname: &str) -> bool {
    match env::var(varname) {
        Err(_) => {
            return false;
        }
        Ok(s) => match s.trim().parse::<i32>() {
            Err(_) => return false,
            Ok(n) => return n > 0,
        },
    }
}

fn regen(filename: &PathBuf, data: &str) -> Result<()> {
    let parentdir = filename.parent().unwrap();
    fs::create_dir_all(parentdir)?;

    fs::write(filename, data.as_bytes())
        .and_then(|_| {
            println!("Regenerated expected data file {:?}", filename);
            Ok(())
        })
        .map_err(|err| err.into())
}

const REGEN_FLAG: &str = "MDEVCTL_TEST_REGENERATE_OUTPUT";

fn test_load_json_helper(uuid: &str, parent: &str, expect: Expect) {
    let test = TestEnvironment::new("load-json", uuid);

    let res = test.load_from_json(uuid, parent, &format!("{}.in", uuid));
    if expect == Expect::Fail {
        info!("{:?}", res);
        res.expect_err("Expected command to fail");
        return;
    }

    let dev = res.expect("Command failed unexpectedly");
    let jsonval = dev.to_json(false).unwrap();
    let jsonstr = serde_json::to_string_pretty(&jsonval).unwrap();

    test.compare_to_file(&format!("{}.out", uuid), &jsonstr);
    assert_eq!(uuid, dev.uuid.to_hyphenated().to_string());
    assert_eq!(Some(parent.to_string()), dev.parent);
}

#[test]
fn test_load_json() {
    init();

    test_load_json_helper(
        "c07ab7b2-8aa2-427a-91c6-ffc949bb77f9",
        "0000:00:02.0",
        Expect::Pass,
    );
    test_load_json_helper(
        "783e6dbb-ea0e-411f-94e2-717eaad438bf",
        "0001:00:03.1",
        Expect::Pass,
    );
    test_load_json_helper(
        "5269fe7a-18d1-48ad-88e1-3fda4176f536",
        "0000:00:03.0",
        Expect::Pass,
    );
    test_load_json_helper(
        "5269fe7a-18d1-48ad-88e1-3fda4176f536",
        "0000:00:03.0",
        Expect::Pass,
    );
    // json file has malformed attributes - an array of one object with multiple fields
    test_load_json_helper(
        "b6f7e33f-ea28-4f9d-8c42-797ff0ec2888",
        "0000:00:03.0",
        Expect::Fail,
    );
    // json file has malformed attributes - an array of strings
    test_load_json_helper(
        "fe7a39db-973b-47b4-9b77-1d7b97267d59",
        "0000:00:03.0",
        Expect::Fail,
    );
    // json file has malformed attributes - no array
    test_load_json_helper(
        "37ccb149-a0ce-49e3-8391-a952ef07bdc2",
        "0000:00:03.0",
        Expect::Fail,
    );
}

fn test_define_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    use crate::define_command_helper;
    let test = TestEnvironment::new("define", testname);

    // load the jsonfile from the test path.
    let jsonfile = match jsonfile {
        Some(f) => Some(test.datapath.join(f)),
        None => None,
    };

    setupfn(&test);

    let def = define_command_helper(&test, uuid, auto, parent, mdev_type, jsonfile);
    if expect == Expect::Fail {
        def.expect_err("expected define command to fail");
        return;
    }

    let def = def.expect("define command failed unexpectedly");
    let path = def.persist_path().unwrap();
    assert!(!path.exists());
    def.define().expect("Failed to define device");
    assert!(path.exists());
    assert!(def.is_defined());
    let filecontents = fs::read_to_string(&path).unwrap();
    test.compare_to_file(&format!("{}.expected", testname), &filecontents);
}

#[test]
fn test_define() {
    init();

    const DEFAULT_UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const DEFAULT_PARENT: &str = "0000:00:03.0";
    test_define_helper(
        "no-uuid-no-type",
        Expect::Fail,
        None,
        true,
        Some(DEFAULT_PARENT.to_string()),
        None,
        None,
        |_| {},
    );
    // if no uuid is specified, one will be auto-generated
    test_define_helper(
        "no-uuid",
        Expect::Pass,
        None,
        true,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |_| {},
    );
    // specify autostart
    test_define_helper(
        "uuid-auto",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        true,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |_| {},
    );
    // specify manual start
    test_define_helper(
        "uuid-manual",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |_| {},
    );
    // invalid to specify an separate mdev_type if defining via jsonfile
    test_define_helper(
        "jsonfile-type",
        Expect::Fail,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        Some(PathBuf::from("defined.json")),
        |_| {},
    );
    // specifying via jsonfile properly
    test_define_helper(
        "jsonfile",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        None,
        Some(PathBuf::from("defined.json")),
        |_| {},
    );
    // If uuid is already active, specifying mdev_type will result in an error
    test_define_helper(
        "uuid-running-no-parent",
        Expect::Fail,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        None,
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "i915-GVTg_V5_4");
        },
    );
    // If uuid is already active, should use mdev_type from running mdev
    test_define_helper(
        "uuid-running-no-type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        None,
        None,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "i915-GVTg_V5_4");
        },
    );
    // ok to define a device with the same uuid as a running device even if they have different
    // parent devices
    test_define_helper(
        "uuid-running-diff-parent",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |test| {
            test.populate_active_device(DEFAULT_UUID, "0000:00:02.0", "i915-GVTg_V5_4");
        },
    );
    // ok to define a device with the same uuid as a running device even if they have different
    // mdev_types
    test_define_helper(
        "uuid-running-diff-type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "different_type");
        },
    );
    // defining a device that is already defined should result in an error
    test_define_helper(
        "uuid-already-defined",
        Expect::Fail,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        |test| {
            test.populate_defined_device(DEFAULT_UUID, DEFAULT_PARENT, "defined.json");
        },
    );
}

fn test_modify_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: &str,
    parent: Option<String>,
    mdev_type: Option<String>,
    addattr: Option<String>,
    delattr: bool,
    index: Option<u32>,
    value: Option<String>,
    auto: bool,
    manual: bool,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    use crate::modify_command;
    let test = TestEnvironment::new("modify", testname);
    setupfn(&test);
    let uuid = Uuid::parse_str(uuid).unwrap();
    let result = modify_command(
        &test,
        uuid,
        parent.clone(),
        mdev_type,
        addattr,
        delattr,
        index,
        value,
        auto,
        manual,
    );
    if expect == Expect::Fail {
        assert!(result.is_err());
        return;
    }

    result.expect("Modify command failed unexpectedly");

    let def = crate::get_defined_device(&test, uuid, parent.as_ref())
        .expect("Couldn't find defined device");
    let path = def.persist_path().unwrap();
    assert!(path.exists());
    assert!(def.is_defined());
    let filecontents = fs::read_to_string(&path).unwrap();
    test.compare_to_file(&format!("{}.expected", testname), &filecontents);
}

#[test]
fn test_modify() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    test_modify_helper(
        "device-not-defined",
        Expect::Fail,
        UUID,
        None,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        |_| {},
    );
    test_modify_helper(
        "auto",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        true,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "manual",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        true,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "delattr",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        true,
        Some(2),
        None,
        false,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "delattr-noindex",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        true,
        None,
        None,
        false,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "addattr",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        Some("added-attr".to_string()),
        false,
        Some(3),
        Some("added-attr-value".to_string()),
        false,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "addattr-noindex",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        Some("added-attr".to_string()),
        false,
        None,
        Some("added-attr-value".to_string()),
        false,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "mdev_type",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        Some("changed-mdev-type".to_string()),
        None,
        false,
        None,
        None,
        false,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "multiple-noparent",
        Expect::Fail,
        UUID,
        None,
        None,
        None,
        false,
        None,
        None,
        true,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, "0000:00:02.0", "defined.json");
        },
    );
    test_modify_helper(
        "multiple-parent",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        true,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, "0000:00:02.0", "defined.json");
        },
    );
    test_modify_helper(
        "auto-manual",
        Expect::Fail,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        true,
        true,
        |_| {},
    );
}

fn test_undefine_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: &str,
    parent: Option<String>,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("undefine", testname);
    setupfn(&test);
    let uuid = Uuid::parse_str(uuid).unwrap();

    let result = crate::undefine_command(&test, uuid, parent.clone());

    if expect == Expect::Fail {
        result.expect_err("undefine command should have failed");
        return;
    }

    result.expect("undefine command should have succeeded");

    let devs = crate::defined_devices(&test, Some(&uuid), parent.as_ref())
        .expect("failed to query defined devices");
    assert!(devs.is_empty());
}

#[test]
fn test_undefine() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    const PARENT2: &str = "0000:00:02.0";

    test_undefine_helper(
        "single",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    // if multiple devices with the same uuid exists, the one with the matching parent should
    // be undefined
    test_undefine_helper(
        "multiple-parent",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    // if multiple devices with the same uuid exists and no parent is specified, they should
    // all be undefined
    test_undefine_helper("multiple-noparent", Expect::Pass, UUID, None, |test| {
        test.populate_defined_device(UUID, PARENT, "defined.json");
        test.populate_defined_device(UUID, PARENT2, "defined.json");
    });
    test_undefine_helper(
        "nonexistent",
        Expect::Fail,
        UUID,
        Some(PARENT.to_string()),
        |_| {},
    );
}

fn test_start_helper<F>(
    testname: &str,
    expect_setup: Expect,
    expect_execute: Expect,
    uuid: Option<String>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("start", testname);
    setupfn(&test);
    let uuid = uuid.map(|s| Uuid::parse_str(s.as_ref()).unwrap());

    let dev = crate::start_command_helper(&test, uuid, parent, mdev_type, jsonfile);

    if expect_setup == Expect::Fail {
        dev.expect_err("start command should have failed");
        return;
    }
    let mut dev = dev.expect("Couldn't run start command");

    let result = dev.start();
    if expect_execute == Expect::Fail {
        result.expect_err("start command should have failed");
        return;
    }
    result.expect("Couldn't start the device");

    let create_path = test
        .parent_base()
        .join(dev.parent.unwrap())
        .join("mdev_supported_types")
        .join(dev.mdev_type.unwrap())
        .join("create");
    assert!(create_path.exists());
    if uuid.is_some() {
        assert_eq!(uuid.unwrap(), dev.uuid);
    }
    let contents = fs::read_to_string(create_path).expect("Unable to read 'create' file");
    assert_eq!(dev.uuid.to_hyphenated().to_string(), contents);
}

#[test]
fn test_start() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    const PARENT2: &str = "0000:00:02.0";
    const MDEV_TYPE: &str = "arbitrary_type";

    test_start_helper(
        "uuid-type-parent",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid",
        Expect::Pass,
        Expect::Pass,
        None,
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid-no-parent",
        Expect::Fail,
        Expect::Fail,
        None,
        None,
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid-no-type",
        Expect::Fail,
        Expect::Fail,
        None,
        Some(PARENT.to_string()),
        None,
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-parent",
        Expect::Fail,
        Expect::Fail,
        Some(UUID.to_string()),
        None,
        Some(MDEV_TYPE.to_string()),
        None,
        |_| {},
    );
    // should fail if there is no defined device with the given uuid
    test_start_helper(
        "no-type",
        Expect::Fail,
        Expect::Fail,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        |_| {},
    );
    // should pass if there is a defined device with the given uuid
    test_start_helper(
        "no-type-defined",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "no-type-parent-defined",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        None,
        None,
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "defined-with-type",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    // if there are multiple defined devices with the same UUID, must disambiguate with parent
    test_start_helper(
        "defined-multiple-underspecified",
        Expect::Fail,
        Expect::Fail,
        Some(UUID.to_string()),
        None,
        None,
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    test_start_helper(
        "defined-multiple",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    // test specifying a uuid and a parent matching an existing defined device but with a different
    // type. See https://github.com/mdevctl/mdevctl/issues/38
    test_start_helper(
        "defined-diff-type",
        Expect::Fail,
        Expect::Fail,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some("wrong-type".to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "already-running",
        Expect::Pass,
        Expect::Fail,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_active_device(UUID, PARENT, MDEV_TYPE);
        },
    );
    test_start_helper(
        "no-instances",
        Expect::Pass,
        Expect::Fail,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 0, "vfio-pci", "testdev", None);
        },
    );

    test_start_helper(
        "uuid-type-parent",
        Expect::Pass,
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    // TODO: test attributes -- difficult because executing the 'start' command by writing to
    // the 'create' file in sysfs does not automatically create the device file structure in
    // the temporary test environment, so writing the sysfs attribute files fails.
}

#[test]
fn test_stop() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    const MDEV_TYPE: &str = "arbitrary_type";

    let test = TestEnvironment::new("stop", "default");
    test.populate_active_device(UUID, PARENT, MDEV_TYPE);

    crate::stop_command(&test, Uuid::parse_str(UUID).unwrap())
        .expect("stop command failed unexpectedly");

    let remove_path = test.mdev_base().join(UUID).join("remove");
    assert!(remove_path.exists());
    let contents = fs::read_to_string(remove_path).expect("Unable to read 'remove' file");
    assert_eq!("1", contents);
}

#[test]
fn test_invalid_files() {
    init();

    const PARENT: &str = "0000:00:03.0";
    const MDEV_TYPE: &str = "arbitrary_type";

    // just make sure that the list command can deal with invalid files without panic-ing
    let test = TestEnvironment::new("invalid-files", "invalid-active");
    test.populate_active_device("invalid-uuid-value", PARENT, MDEV_TYPE);
    let result = crate::list_command(&test, false, false, false, None, None);
    assert!(result.is_ok());

    let test = TestEnvironment::new("invalid-files", "invalid-defined");
    test.populate_defined_device("invalid-uuid-value", PARENT, "device.json");
    let result = crate::list_command(&test, true, false, false, None, None);
    assert!(result.is_ok());
}

fn test_list_helper<F>(
    subtest: &str,
    expect: Expect,
    defined: bool,
    verbose: bool,
    uuid: Option<String>,
    parent: Option<String>,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    use crate::list_command_helper;
    let uuid = uuid.map(|s| Uuid::parse_str(s.as_ref()).unwrap());
    let test = TestEnvironment::new("list", "default");

    setupfn(&test);

    let res = list_command_helper(&test, defined, false, verbose, uuid, parent.clone());
    if expect == Expect::Fail {
        res.expect_err("expected list command to fail");
        return;
    }

    let output = res.expect("list command failed unexpectedly");
    test.compare_to_file(&format!("{}.text", subtest), &output);

    let res = list_command_helper(&test, defined, true, verbose, uuid, parent.clone());
    if expect == Expect::Fail {
        res.expect_err("expected list command to fail");
        return;
    }

    let output = res.expect("list command failed unexpectedly");
    test.compare_to_file(&format!("{}.json", subtest), &output);
}

#[test]
fn test_list() {
    init();

    const UUID: &[&str] = &[
        "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9",
        "59e8b599-afdd-4766-a59e-415ef4f5a492",
        "4a0a190f-dcf3-4def-9342-c48768f0c940",
        "9f579710-6ffc-4201-987a-4ffa0fb1f3a5",
        "3eee6cd9-35ad-43bd-9be1-14ee2b7389c9",
    ];
    const PARENT: &[&str] = &["0000:00:02.0", "0000:00:03.0"];
    const MDEV_TYPE: &[&str] = &["arbitrary_type1", "arbitrary_type2"];

    // first test with an empty environment -- nothing defined, nothing active
    test_list_helper(
        "active-none",
        Expect::Pass,
        false,
        false,
        None,
        None,
        |_| {},
    );
    test_list_helper(
        "defined-none",
        Expect::Pass,
        true,
        false,
        None,
        None,
        |_| {},
    );

    // now setup test environment with some active devices and some defined devices. Include
    // multiple parents, multiple types, some parents with multiple devices, some with same UUID on
    // different parents, etc
    let setup = |test: &TestEnvironment| {
        test.populate_active_device(UUID[0], PARENT[0], MDEV_TYPE[0]);
        test.populate_active_device(UUID[1], PARENT[1], MDEV_TYPE[1]);
        test.populate_defined_device(UUID[2], PARENT[0], "device2.json");
        test.populate_defined_device(UUID[3], PARENT[1], "device1.json");
        test.populate_defined_device(UUID[3], PARENT[0], "device1.json");
    };

    test_list_helper("active", Expect::Pass, false, false, None, None, setup);
    test_list_helper(
        "active-verbose",
        Expect::Pass,
        false,
        true,
        None,
        None,
        setup,
    );
    test_list_helper(
        "active-parent",
        Expect::Pass,
        false,
        false,
        None,
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "active-parent-verbose",
        Expect::Pass,
        false,
        true,
        None,
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "active-uuid",
        Expect::Pass,
        false,
        false,
        Some(UUID[0].to_string()),
        None,
        setup,
    );
    test_list_helper(
        "active-uuid-verbose",
        Expect::Pass,
        false,
        true,
        Some(UUID[0].to_string()),
        None,
        setup,
    );
    test_list_helper(
        "active-uuid-parent",
        Expect::Pass,
        false,
        false,
        Some(UUID[0].to_string()),
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "active-uuid-parent-verbose",
        Expect::Pass,
        false,
        true,
        Some(UUID[0].to_string()),
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper("defined", Expect::Pass, true, false, None, None, setup);
    test_list_helper(
        "defined-verbose",
        Expect::Pass,
        true,
        true,
        None,
        None,
        setup,
    );
    test_list_helper(
        "defined-parent",
        Expect::Pass,
        true,
        false,
        None,
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "defined-parent-verbose",
        Expect::Pass,
        true,
        true,
        None,
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "defined-uuid",
        Expect::Pass,
        true,
        false,
        Some(UUID[3].to_string()),
        None,
        setup,
    );
    test_list_helper(
        "defined-uuid-verbose",
        Expect::Pass,
        true,
        true,
        Some(UUID[3].to_string()),
        None,
        setup,
    );
    test_list_helper(
        "defined-uuid-parent",
        Expect::Pass,
        true,
        false,
        Some(UUID[3].to_string()),
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "defined-uuid-parent-verbose",
        Expect::Pass,
        true,
        true,
        Some(UUID[3].to_string()),
        Some(PARENT[0].to_string()),
        setup,
    );
    test_list_helper(
        "no-match-uuid",
        Expect::Pass,
        true,
        true,
        Some("466983a3-1240-4543-8d02-01c29a08fb0c".to_string()),
        None,
        setup,
    );
    test_list_helper(
        "no-match-parent",
        Expect::Pass,
        true,
        true,
        None,
        Some("nonexistent".to_string()),
        setup,
    );
}

fn test_types_helper(
    test: &TestEnvironment,
    subtest: &str,
    expect: Expect,
    parent: Option<String>,
) {
    use crate::types_command_helper;

    // test text output
    let res = types_command_helper(test, parent.clone(), false);
    if expect == Expect::Fail {
        res.expect_err("expected types command to fail");
        return;
    }

    let output = res.expect("types command failed unexpectedly");
    test.compare_to_file(&format!("{}.text", subtest), &output);

    // test JSON output
    let res = types_command_helper(test, parent.clone(), true);
    if expect == Expect::Fail {
        res.expect_err("expected types command to fail");
        return;
    }

    let output = res.expect("types command failed unexpectedly");
    test.compare_to_file(&format!("{}.json", subtest), &output);
}

#[test]
fn test_types() {
    init();

    let test = TestEnvironment::new("types", "default");

    // test an empty environment without any devices that suppport mdevs
    test_types_helper(&test, "empty", Expect::Pass, None);

    // populate test environment with a variety of parent devices that support certain mdev types
    let mut parents = BTreeMap::new();
    parents.insert(
        "0000:00:02.0",
        vec![
            ("mdev_type1", 5, "vfio-pci", "name1", Some("description 1")),
            ("mdev_type2", 16, "vfio-pci", "name2", None),
            ("mdev_type3", 1, "vfio-pci", "name3", Some("description 3")),
        ],
    );
    parents.insert(
        "0000:00:03.0",
        vec![
            ("nvidia-155", 4, "vfio-pci", "GRID M10-2B", None),
            ("nvidia-36", 16, "vfio-pci", "GRID M10-0Q", None),
        ],
    );
    parents.insert(
        "0.0.26ab",
        vec![("vfio_ccw-io", 4, "vfio_mdev", "name", Some("description"))],
    );

    for (parent, types) in parents {
        for t in types {
            test.populate_parent_device(parent, t.0, t.1, t.2, t.3, t.4);
        }
    }

    test_types_helper(&test, "full", Expect::Pass, None);
    test_types_helper(
        &test,
        "parent-match-1",
        Expect::Pass,
        Some("0000:00:02.0".to_string()),
    );
    test_types_helper(
        &test,
        "parent-match-2",
        Expect::Pass,
        Some("0000:00:03.0".to_string()),
    );
    test_types_helper(
        &test,
        "parent-match-3",
        Expect::Pass,
        Some("0.0.26ab".to_string()),
    );
    test_types_helper(
        &test,
        "parent-no-match",
        Expect::Pass,
        Some("missing".to_string()),
    );
}
