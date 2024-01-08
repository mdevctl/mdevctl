use super::*;

fn test_invoke_callout<F>(
    testname: &str,
    expect: Expect,
    action: Action,
    uuid: Uuid,
    parent: &str,
    mdev_type: &str,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("invoke-callout", testname);
    setupfn(&test);

    let mut empty_mdev = MDev::new(&test, uuid);
    empty_mdev.mdev_type = match mdev_type {
        "" => None,
        _ => Some(mdev_type.to_string()),
    };
    empty_mdev.parent = Some(parent.to_string());

    let mut callout = callout(&mut empty_mdev);
    let res = callout.invoke(action, false, |_| Ok(()));
    let try_force = res.is_err();
    let _ = test.assert_result(res, expect, Some("non-forced"));

    // now force and ensure it succeeds
    if try_force {
        let res = callout.invoke(action, true, |_| Ok(()));
        let _ = test.assert_result(res, Expect::Pass, Some("forced"));
    }
}

fn test_get_callout<F>(
    testname: &str,
    expect: Expect,
    uuid: Uuid,
    parent: &str,
    mdev_type: &str,
    setupfn: F,
) where
    F: Fn(&TestEnvironment),
{
    let test = TestEnvironment::new("get-callout", testname);
    setupfn(&test);

    let mut empty_mdev = MDev::new(&test, uuid);
    empty_mdev.mdev_type = match mdev_type {
        "" => None,
        _ => Some(mdev_type.to_string()),
    };
    empty_mdev.parent = Some(parent.to_string());

    let res = callout(&mut empty_mdev).get_attributes();
    let _ = test.assert_result(res, expect, None);
}

#[test]
#[should_panic]
fn test_invoke_callout_panic() {
    init();
    const DEFAULT_UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const DEFAULT_PARENT: &str = "test_parent";

    test_invoke_callout(
        "test_invoke_callout_create_panic",
        Expect::Pass,
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        "",
        |test| {
            test.populate_callout_script("rc0.sh");
        },
    );
}

#[test]
#[should_panic]
fn test_get_callout_panic() {
    init();
    const DEFAULT_UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const DEFAULT_PARENT: &str = "test_parent";

    test_get_callout(
        "test_get_callout_create_panic",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        "",
        |test| {
            test.populate_callout_script("rc0.sh");
        },
    );
}

#[test]
fn test_callouts() {
    init();

    const DEFAULT_UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const DEFAULT_TYPE: &str = "test_type";
    const DEFAULT_PARENT: &str = "test_parent";
    test_invoke_callout(
        "test_callout_all_success",
        Expect::Pass,
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc0.sh");
        },
    );
    test_invoke_callout(
        "test_callout_all_fail",
        Expect::Fail(None),
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh");
        },
    );
    // Expected behavior: script will report that the requested device type / parent does not
    // match the script's type / parent. mdevctl will continue with regularly scheduled programming.
    test_invoke_callout(
        "test_callout_wrong_type",
        Expect::Pass,
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc2.sh");
        },
    );
    // This test is expected to fail. If the correct script is executed, then it will`
    // return error code 1.
    test_invoke_callout(
        "test_callout_type_c",
        Expect::Fail(None),
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "parent_c",
        "type_c",
        |test| {
            test.populate_callout_script("type-a.sh");
            test.populate_callout_script("type-b.sh");
            test.populate_callout_script("type-c.sh");
        },
    );
    test_invoke_callout(
        "test_callout_no_script",
        Expect::Pass,
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "parent_d",
        "type_d",
        |test| {
            test.populate_callout_script("type-a.sh");
            test.populate_callout_script("type-b.sh");
            test.populate_callout_script("type-c.sh");
        },
    );
    // Each pre/post function in the test script will check for
    // a device type and parent with the command name appended
    // to the end
    test_invoke_callout(
        "test_callout_params_define",
        Expect::Pass,
        Action::Define,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "test_parent_define",
        "test_type_define",
        |test| {
            test.populate_callout_script("params.sh");
        },
    );
    test_invoke_callout(
        "test_callout_params_modify",
        Expect::Pass,
        Action::Modify,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "test_parent_modify",
        "test_type_modify",
        |test| {
            test.populate_callout_script("params.sh");
        },
    );
    test_invoke_callout(
        "test_callout_params_start",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "test_parent_start",
        "test_type_start",
        |test| {
            test.populate_callout_script("params.sh");
        },
    );
    test_invoke_callout(
        "test_callout_params_stop",
        Expect::Pass,
        Action::Stop,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "test_parent_stop",
        "test_type_stop",
        |test| {
            test.populate_callout_script("params.sh");
        },
    );
    test_invoke_callout(
        "test_callout_params_undefine",
        Expect::Pass,
        Action::Undefine,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        "test_parent_undefine",
        "test_type_undefine",
        |test| {
            test.populate_callout_script("params.sh");
        },
    );
    // test the Get Attributes callouts
    test_get_callout(
        "test_callout_good_json",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("good-json.sh");
        },
    );
    test_get_callout(
        "test_callout_bad_json",
        Expect::Fail(None),
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("bad-json.sh");
        },
    );
    test_invoke_callout(
        "test_callout_order_fail",
        Expect::Fail(None),
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc1.sh", Some("00.sh"), true);
            test.populate_callout_script_full("rc0.sh", Some("99.sh"), true);
        },
    );
    test_invoke_callout(
        "test_callout_order_pass",
        Expect::Pass,
        Action::Test,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc0.sh", Some("00.sh"), true);
            test.populate_callout_script_full("rc1.sh", Some("99.sh"), true);
        },
    );
    test_invoke_callout(
        "test_callout_location_priority_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc0.sh", None, true);
            test.populate_callout_script_full("rc1.sh", None, false);
        },
    );
    test_invoke_callout(
        "test_callout_location_priority_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc0.sh", None, false);
            test.populate_callout_script_full("rc1.sh", None, true);
        },
    );
    test_invoke_callout(
        "test_callout_location_priority_skip_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc2.sh", None, true);
            test.populate_callout_script_full("rc1.sh", None, false);
        },
    );
    test_invoke_callout(
        "test_callout_location_priority_skip_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script_full("rc2.sh", None, true);
            test.populate_callout_script_full("rc0.sh", None, false);
        },
    );
    test_get_callout(
        "test_callout_good_json_null",
        Expect::Pass,
        Uuid::parse_str(DEFAULT_UUID).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("good-json-null-terminated.sh");
        },
    );

    // test start with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_VER: &str = "11111111-1111-0000-0000-000000000000";
    const UUID_VER_RC1: &str = "11111111-1111-0000-0000-111111111111";
    const UUID_VER_RC2: &str = "11111111-1111-0000-0000-222222222222";
    const UUID_VER_BAD_JSON: &str = "11111111-1111-0000-0000-aaaaaaaaaaaa";
    const UUID_VER_ACTION_DUMMY: &str = "11111111-1111-0000-0000-bbbbbbbbbbbb";
    const UUID_VER_EVENT_DUMMY: &str = "11111111-1111-0000-0000-cccccccccccc";
    const UUID_VER_MODIFY_MISSING: &str = "11111111-1111-0000-0000-dddddddddddd";
    const UUID_VER_PROVIDES: &str = "11111111-1111-0000-0000-eeeeeeeeeeee";
    const UUID_VER_INVALID_JSON: &str = "11111111-1111-0000-0000-ffffffffffff";

    test_invoke_callout(
        "test_callout_with_version_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_invoke_callout(
        "test_callout_with_version_mix_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_mix_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_get_callout(
        "test_callout_with_version_good_json",
        Expect::Pass,
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_get_callout(
        "test_callout_with_version_bad_json",
        Expect::Fail(None),
        Uuid::parse_str(UUID_VER).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc0-get-attr-bad-json.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_get_capabilities_rc1_run_with_version_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER_RC1).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_get_capabilities_rc2_run_without_version_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(UUID_VER_RC2).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_get_capabilities_bad_run_without_version_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(UUID_VER_BAD_JSON).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_unknown_action_with_version_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER_ACTION_DUMMY).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_unknown_event_with_verion_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER_EVENT_DUMMY).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_missing_modify_run_start_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER_MODIFY_MISSING).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_missing_modify_run_modify_fail",
        Expect::Fail(None),
        Action::Modify,
        Uuid::parse_str(UUID_VER_MODIFY_MISSING).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_json_provides_with_version_pass",
        Expect::Pass,
        Action::Start,
        Uuid::parse_str(UUID_VER_PROVIDES).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_invoke_callout(
        "test_callout_with_version_json_invalid_with_version_without_version_fail",
        Expect::Fail(None),
        Action::Start,
        Uuid::parse_str(UUID_VER_INVALID_JSON).unwrap(),
        DEFAULT_PARENT,
        DEFAULT_TYPE,
        |test| {
            test.populate_callout_script("rc1.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
}
