use super::*;
use std::{fs, path::PathBuf};
use uuid::Uuid;

fn test_define_command_callout<F>(
    testname: &str,
    expect: Expect,
    uuid: Option<Uuid>,
    parent: Option<String>,
    mdev_type: Option<String>,
    force: bool,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("define-callouts", testname);
    let env: Rc<dyn Environment> = test.clone();
    setupfn(&test);

    use crate::define_command;
    let res = define_command(env, uuid, false, parent, mdev_type, None, force);

    let _ = test.assert_result(res, expect, None);
}

#[allow(clippy::too_many_arguments)]
fn test_define_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: Option<Uuid>,
    auto: bool,
    parent: Option<String>,
    mdev_type: Option<String>,
    jsonfile: Option<PathBuf>,
    force: bool,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    use crate::define_command_helper;
    let test = TestEnvironment::new("define", testname);
    let env: Rc<dyn Environment> = test.clone();

    // load the jsonfile from the test path.
    let jsonfile = jsonfile.map(|f| test.datapath.join(f));

    setupfn(&test);

    let res = define_command_helper(env, uuid, auto, parent, mdev_type, jsonfile, force);
    let expected_testfilename = format!("{}.expected", testname);
    if let Ok(def) = test.assert_result(res, expect, None) {
        let path = def.persistent_path().unwrap();
        assert!(!path.exists());
        def.define().expect("Failed to define device");
        assert!(path.exists());
        assert!(def.is_defined());
        let filecontents = fs::read_to_string(&path).unwrap();
        test.compare_to_file(&expected_testfilename, &filecontents);
    } else {
        test.unused_file(&expected_testfilename);
    }
}

#[test]
fn test_define() {
    init();

    const DEFAULT_UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const DEFAULT_PARENT: &str = "0000:00:03.0";
    test_define_helper(
        "no-uuid-no-type",
        Expect::Fail(None),
        None,
        true,
        Some(DEFAULT_PARENT.to_string()),
        None,
        None,
        false,
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
        false,
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
        false,
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
        false,
        |_| {},
    );
    // invalid to specify an separate mdev_type if defining via jsonfile
    test_define_helper(
        "jsonfile-type",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        Some(PathBuf::from("defined.json")),
        false,
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
        false,
        |_| {},
    );
    // If uuid is already active, specifying mdev_type will result in an error
    test_define_helper(
        "uuid-running-no-parent",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        None,
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
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
        false,
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
        false,
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
        false,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "different_type");
        },
    );
    // defining a device with the same uuid as a running device with a broken mdev_type
    test_define_helper(
        "uuid-running-broken-active-mdev_type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
        |test| {
            test.populate_broken_active_device_links(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    test_define_helper(
        "uuid-running-removed-active-mdev_type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
        |test| {
            test.populate_removed_active_device_attributes(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    // defining a device with the same uuid as a running device with a broken mdev_type without specifying mdev_type
    test_define_helper(
        "uuid-running-broken-active-mdev_type-no-mdev_type",
        Expect::Fail(Some("No type specified")),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        None,
        None,
        false,
        |test| {
            test.populate_broken_active_device_links(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    test_define_helper(
        "uuid-running-removed-active-mdev_type-no-mdev_type",
        Expect::Fail(Some("No type specified")),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        None,
        None,
        false,
        |test| {
            test.populate_removed_active_device_attributes(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    // defining a device with the same uuid as a running device with a broken parent
    test_define_helper(
        "uuid-running-broken-active-parent",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
        |test| {
            test.populate_broken_active_device_links(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                true,
                false,
            );
        },
    );
    test_define_helper(
        "uuid-running-removed-active-parent",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
        |test| {
            test.populate_removed_active_device_attributes(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                true,
                false,
            );
        },
    );
    // force defining a device with the same uuid as a running device with a broken mdev_type
    test_define_helper(
        "uuid-running-force-broken-active-mdev_type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        true,
        |test| {
            test.populate_broken_active_device_links(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    test_define_helper(
        "uuid-running-force-removed-active-mdev_type",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        true,
        |test| {
            test.populate_removed_active_device_attributes(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                false,
                true,
            );
        },
    );
    // force defining a device with the same uuid as a running device with a broken parent
    test_define_helper(
        "uuid-running-force-broken-active-parent",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        true,
        |test| {
            test.populate_broken_active_device_links(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                true,
                false,
            );
        },
    );
    test_define_helper(
        "uuid-running-force-removed-active-parent",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        true,
        |test| {
            test.populate_removed_active_device_attributes(
                DEFAULT_UUID,
                DEFAULT_PARENT,
                "i915-GVTg_V5_4",
                true,
                false,
            );
        },
    );
    // defining a device that is already defined should result in an error
    test_define_helper(
        "uuid-already-defined",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        false,
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        None,
        false,
        |test| {
            test.populate_defined_device(DEFAULT_UUID, DEFAULT_PARENT, "defined.json");
        },
    );

    // test define with callouts
    test_define_command_callout(
        "define-with-callout-all-pass",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("rc0.sh");
        },
    );
    test_define_command_callout(
        "define-with-callout-all-fail",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("rc1.sh");
        },
    );
    // test define with get attributes
    test_define_command_callout(
        "define-with-callout-all-good-json",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "i915-GVTg_V5_4");
            test.populate_callout_script("good-json.sh");
        },
    );
    test_define_command_callout(
        "define-with-callout-all-bad-json",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_active_device(DEFAULT_UUID, DEFAULT_PARENT, "i915-GVTg_V5_4");
            test.populate_callout_script("bad-json.sh");
        },
    );
    test_define_command_callout(
        "define-with-callout-all-fail-force",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        true,
        |test| {
            test.populate_callout_script("rc1.sh");
        },
    );

    // test define with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    test_define_command_callout(
        "define-with-version-callout-all-pass",
        Expect::Pass,
        Uuid::parse_str("11111111-1111-0000-0000-000000000000").ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_define_command_callout(
        "define-with-version-callout-all-fail",
        Expect::Fail(None),
        Uuid::parse_str("11111111-1111-0000-0000-000000000000").ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_define_command_callout(
        "define-with-version-callout-multiple-with-version-pass",
        Expect::Pass,
        Uuid::parse_str("11111111-1111-0000-0000-000000000000").ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_define_command_callout(
        "define-with-version-callout-multiple-with-version-pass2",
        Expect::Pass,
        Uuid::parse_str("11111111-1111-0000-0000-000000000000").ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning error
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_define_command_callout(
        "define-with-version-callout-multiple-with-version-fail",
        Expect::Fail(None),
        Uuid::parse_str("11111111-1111-0000-0000-000000000000").ok(),
        Some(DEFAULT_PARENT.to_string()),
        Some("i915-GVTg_V5_4".to_string()),
        false,
        |test| {
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
}

fn test_undefine_helper<F>(
    testname: &str,
    expect: Expect,
    uuid: &str,
    parent: Option<String>,
    force: bool,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("undefine", testname);
    let env: Rc<dyn Environment> = test.clone();
    setupfn(&test);
    let uuid = Uuid::parse_str(uuid).unwrap();

    let result = crate::undefine_command(env.clone(), uuid, parent.clone(), force);

    if test.assert_result(result, expect, None).is_err() {
        return;
    }

    let devs = test
        .get_defined_devices(Some(&uuid), parent.as_ref())
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
        false,
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
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    // if multiple devices with the same uuid exists and no parent is specified, they should
    // all be undefined
    test_undefine_helper(
        "multiple-noparent",
        Expect::Pass,
        UUID,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, PARENT2, "defined.json");
        },
    );
    test_undefine_helper(
        "nonexistent",
        Expect::Fail(None),
        UUID,
        Some(PARENT.to_string()),
        false,
        |_| {},
    );

    // callout script always returns with RC=0
    test_undefine_helper(
        "single-callout-all-pass",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh");
        },
    );
    // callout script rejects in pre event undefine with RC=1
    test_undefine_helper(
        "single-callout-pre-fail",
        Expect::Fail(None),
        UUID,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );
    // force command even with callout script failure
    test_undefine_helper(
        "single-callout-pre-fail-force",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        true,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );

    // test define with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_VER: &str = "11111111-1111-0000-0000-000000000000";
    test_undefine_helper(
        "undefine-single-with-version-callout-all-pass",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_undefine_helper(
        "undefine-single-with-version-callout-all-fail",
        Expect::Fail(None),
        UUID_VER,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_undefine_helper(
        "define-with-version-callout-multiple-with-version-pass",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_undefine_helper(
        "define-with-version-callout-multiple-with-version-pass2",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh"); // no versioning error
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_undefine_helper(
        "define-with-version-callout-multiple-with-version-fail",
        Expect::Fail(None),
        UUID_VER,
        Some(PARENT.to_string()),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
}
