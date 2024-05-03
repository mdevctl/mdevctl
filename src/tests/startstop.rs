use super::*;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
fn test_start_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: Option<String>,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
    force: bool,
    setupfn: F,
) where
    F: Fn(Rc<TestEnvironment>),
{
    let test = TestEnvironment::new("start", testname);
    let env: Rc<dyn Environment> = test.clone();
    setupfn(test.clone());
    let uuid = uuid.map(|s| Uuid::parse_str(s.as_ref()).unwrap());

    let result = crate::start_command_helper(env, uuid, parent, mdev_type, jsonfile, force);

    if let Ok(dev) = test.assert_result(result, expect, None) {
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
        assert_eq!(dev.uuid.hyphenated().to_string(), contents);
    }
}

#[test]
fn test_start() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    const PARENT2: &str = "0000:00:02.0";
    const PARENT3: &str = "0000:2b:00.0";
    const MDEV_TYPE: &str = "arbitrary_type";

    test_start_helper(
        "uuid-type-parent",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid",
        Expect::Pass,
        None,
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid-no-parent",
        Expect::Fail(None),
        None,
        None,
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-uuid-no-type",
        Expect::Fail(None),
        None,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );
    test_start_helper(
        "no-parent",
        Expect::Fail(None),
        Some(UUID.to_string()),
        None,
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |_| {},
    );
    // should fail if there is no defined device with the given uuid
    test_start_helper(
        "no-type",
        Expect::Fail(None),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        false,
        |_| {},
    );
    // should pass if there is a defined device with the given uuid
    test_start_helper(
        "no-type-defined",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "no-type-parent-defined",
        Expect::Pass,
        Some(UUID.to_string()),
        None,
        None,
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "defined-with-type",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
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
        Expect::Fail(None),
        Some(UUID.to_string()),
        None,
        None,
        None,
        false,
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
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        None,
        None,
        false,
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
        Expect::Fail(None),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some("wrong-type".to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_start_helper(
        "already-running",
        Expect::Fail(Some("Device already exists")),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_active_device(UUID, PARENT, MDEV_TYPE);
        },
    );
    // test with active broken mdev
    test_start_helper(
        "already-running-broken-active-mdev-type",
        Expect::Fail(Some("No such file or directory (os error 2)")),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, false, true);
        },
    );
    test_start_helper(
        "already-running-removed-active-mdev-type",
        Expect::Fail(Some("No such file or directory (os error 2)")),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, false, true);
        },
    );
    test_start_helper(
        "no-instances",
        Expect::Fail(None),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 0, "vfio-pci", "testdev", None);
        },
    );

    test_start_helper(
        "uuid-type-parent",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );

    test_start_helper(
        "defined-multiple-callout-success",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
            test.populate_callout_script("rc0.sh");
        },
    );
    test_start_helper(
        "defined-multiple-callout-fail",
        Expect::Fail(None),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );
    test_start_helper(
        "defined-multiple-callout-fail-force",
        Expect::Pass,
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        true,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_parent_device(PARENT2, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID, PARENT2, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );
    test_start_helper(
        "missing-parent",
        Expect::Fail(Some(
            format!("Unable to find parent device '{}'", PARENT).as_str(),
        )),
        Some(UUID.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |_| {},
    );
    test_start_helper(
        "parent-case",
        Expect::Fail(Some(
            format!(
                "Unable to find parent device '{}'. Did you mean '{}'?",
                PARENT3.to_string().to_uppercase(),
                PARENT3
            )
            .as_str(),
        )),
        Some(UUID.to_string()),
        Some(PARENT3.to_string().to_uppercase()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT3, MDEV_TYPE, 1, "vfio-pci", "test device", None);
        },
    );

    // TODO: test attributes -- difficult because executing the 'start' command by writing to
    // the 'create' file in sysfs does not automatically create the device file structure in
    // the temporary test environment, so writing the sysfs attribute files fails.

    // test start with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_VER: &str = "11111111-1111-0000-0000-000000000000";
    test_start_helper(
        "start-single-with-version-callout-pass",
        Expect::Pass,
        Some(UUID_VER.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_start_helper(
        "start-single-with-version-callout-fail",
        Expect::Fail(None),
        Some(UUID_VER.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_start_helper(
        "start-with-version-callout-multiple-with-version-pass",
        Expect::Pass,
        Some(UUID_VER.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_start_helper(
        "start-with-version-callout-multiple-with-version-pass2",
        Expect::Pass,
        Some(UUID_VER.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh"); // no versioning error
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_start_helper(
        "start-with-version-callout-multiple-with-version-fail",
        Expect::Fail(None),
        Some(UUID_VER.to_string()),
        Some(PARENT.to_string()),
        Some(MDEV_TYPE.to_string()),
        None,
        false,
        |test| {
            test.populate_parent_device(PARENT, MDEV_TYPE, 1, "vfio-pci", "test device", None);
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
}

fn test_stop_helper<F>(testname: &str, expect: Expect, uuid: &str, force: bool, setupfn: F)
where
    F: Fn(Rc<TestEnvironment>),
{
    let test = TestEnvironment::new("stop", testname);
    let env: Rc<dyn Environment> = test.clone();
    setupfn(test.clone());

    let res = crate::stop_command(env, Uuid::parse_str(uuid).unwrap(), force);

    if test.assert_result(res, expect, None).is_ok() {
        let remove_path = test.mdev_base().join(uuid).join("remove");
        assert!(remove_path.exists());
        let contents = fs::read_to_string(remove_path).expect("Unable to read 'remove' file");
        assert_eq!("1", contents);
    }
}

#[test]
fn test_stop() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    const MDEV_TYPE: &str = "arbitrary_type";

    test_stop_helper("default", Expect::Pass, UUID, false, |t| {
        t.populate_active_device(UUID, PARENT, MDEV_TYPE)
    });
    test_stop_helper("callout-success", Expect::Pass, UUID, false, |t| {
        t.populate_active_device(UUID, PARENT, MDEV_TYPE);
        t.populate_callout_script("rc0.sh")
    });
    test_stop_helper("callout-fail", Expect::Fail(None), UUID, false, |t| {
        t.populate_active_device(UUID, PARENT, MDEV_TYPE);
        t.populate_callout_script("rc1.sh")
    });
    test_stop_helper("callout-fail-force", Expect::Pass, UUID, true, |t| {
        t.populate_active_device(UUID, PARENT, MDEV_TYPE);
        t.populate_callout_script("rc1.sh")
    });
    test_stop_helper(
        "broken-active-mdev-type",
        Expect::Fail(None),
        UUID,
        false,
        |t| t.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, false, true),
    );
    test_stop_helper(
        "broken-active-mdev-type",
        Expect::Fail(None),
        UUID,
        false,
        |t| t.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, false, true),
    );
    test_stop_helper(
        "broken-active-parent",
        Expect::Fail(None),
        UUID,
        false,
        |t| t.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, true, false),
    );
    test_stop_helper(
        "removed-active-parent",
        Expect::Fail(None),
        UUID,
        false,
        |t| t.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, true, false),
    );
    test_stop_helper(
        "callout-success-broken-active-parent",
        Expect::Fail(Some(
            format!("Device {UUID} is not an active mdev").as_str(),
        )),
        UUID,
        false,
        |t| {
            t.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, true, false);
            t.populate_callout_script("rc0.sh")
        },
    );
    test_stop_helper(
        "callout-success-removed-active-parent",
        Expect::Fail(Some(
            format!("Device {UUID} is not an active mdev").as_str(),
        )),
        UUID,
        false,
        |t| {
            t.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, true, false);
            t.populate_callout_script("rc0.sh")
        },
    );
    test_stop_helper(
        "callout-fail-force-broken-active-mdev-type",
        Expect::Fail(Some("Device must have a defined mdev_type")),
        UUID,
        true,
        |t| {
            t.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, false, true);
            t.populate_callout_script("rc1.sh")
        },
    );
    test_stop_helper(
        "callout-fail-force-removed-active-mdev-type",
        Expect::Fail(Some("Device must have a defined mdev_type")),
        UUID,
        true,
        |t| {
            t.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, false, true);
            t.populate_callout_script("rc1.sh")
        },
    );
    test_stop_helper(
        "callout-fail-force-broken-active-parent",
        Expect::Fail(Some(
            format!("Device {UUID} is not an active mdev").as_str(),
        )),
        UUID,
        true,
        |t| {
            t.populate_broken_active_device_links(UUID, PARENT, MDEV_TYPE, true, false);
            t.populate_callout_script("rc1.sh")
        },
    );
    test_stop_helper(
        "callout-fail-force-removed-active-parent",
        Expect::Fail(Some(
            format!("Device {UUID} is not an active mdev").as_str(),
        )),
        UUID,
        true,
        |t| {
            t.populate_removed_active_device_attributes(UUID, PARENT, MDEV_TYPE, true, false);
            t.populate_callout_script("rc1.sh")
        },
    );

    // test start with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_VER: &str = "11111111-1111-0000-0000-000000000000";
    test_stop_helper(
        "stop-single-callout-with-version-all-pass",
        Expect::Pass,
        UUID_VER,
        false,
        |test| {
            test.populate_active_device(UUID_VER, PARENT, MDEV_TYPE);
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_stop_helper(
        "stop-single-callout-with-version-all-fail",
        Expect::Fail(None),
        UUID_VER,
        false,
        |test| {
            test.populate_active_device(UUID_VER, PARENT, MDEV_TYPE);
            test.populate_callout_script("ver-rc1.sh"); // versioning
        },
    );
    test_stop_helper(
        "stop-single-callouts-mix-all-pass",
        Expect::Pass,
        UUID_VER,
        false,
        |test| {
            test.populate_active_device(UUID_VER, PARENT, MDEV_TYPE);
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_stop_helper(
        "stop-single-callouts-mix-all-fail",
        Expect::Fail(None),
        UUID_VER,
        false,
        |test| {
            test.populate_active_device(UUID_VER, PARENT, MDEV_TYPE);
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning
        },
    );
}
