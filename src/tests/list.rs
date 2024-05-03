use super::*;
use uuid::Uuid;

#[test]
fn test_invalid_files() {
    init();

    const PARENT: &str = "0000:00:03.0";
    const MDEV_TYPE: &str = "arbitrary_type";
    let mut outbuf: Vec<u8> = Default::default();

    // just make sure that the list command can deal with invalid files without panic-ing
    let test = TestEnvironment::new("invalid-files", "invalid-active");
    let env: Rc<dyn Environment> = test.clone();
    test.populate_active_device("invalid-uuid-value", PARENT, MDEV_TYPE);
    let result = crate::list_command(env.clone(), false, false, false, None, None, &mut outbuf);
    assert!(result.is_ok());

    let test = TestEnvironment::new("invalid-files", "invalid-defined");
    test.populate_defined_device("invalid-uuid-value", PARENT, "device.json");
    let result = crate::list_command(env.clone(), true, false, false, None, None, &mut outbuf);
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
    F: Fn(&Rc<TestEnvironment>),
{
    use crate::list_command;
    let uuid = uuid.map(|s| Uuid::parse_str(s.as_ref()).unwrap());
    let test = TestEnvironment::new("list", "default");
    let env: Rc<dyn Environment> = test.clone();

    setupfn(&test);

    let mut outbuf: Vec<u8> = Default::default();
    let res = list_command(
        env.clone(),
        defined,
        false,
        verbose,
        uuid,
        parent.clone(),
        &mut outbuf,
    );
    let text_testfilename = format!("{}.text", subtest);
    if test.assert_result(res, expect, Some("json")).is_ok() {
        let actual = String::from_utf8(outbuf).expect("failed to convert list output from utf8");
        test.compare_to_file(&text_testfilename, &actual);
    } else {
        test.unused_file(&text_testfilename);
    }

    let mut outbuf: Vec<u8> = Default::default();
    let res = list_command(
        env.clone(),
        defined,
        true,
        verbose,
        uuid,
        parent.clone(),
        &mut outbuf,
    );
    let json_testfilename = format!("{}.json", subtest);
    if test.assert_result(res, expect, Some("text")).is_ok() {
        let actual = String::from_utf8(outbuf).expect("failed to convert list output from utf8");
        test.compare_to_file(&json_testfilename, &actual);
    } else {
        test.unused_file(&json_testfilename);
    }
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
        "bad1bad2-bad3-bad4-bad5-bad6bad7bad8",
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
    let setup = |test: &Rc<TestEnvironment>| {
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

    // test list with the Get Attributes callout
    test_list_helper(
        "active-callout",
        Expect::Pass,
        false,
        false,
        None,
        None,
        |test| {
            setup(test);
            test.populate_callout_script("good-json.sh");
        },
    );
    // if a script returns an ill-formatted JSON, then then the output should be ignored
    test_list_helper(
        "active-callout-bad-json",
        Expect::Pass,
        false,
        false,
        None,
        None,
        |test| {
            setup(test);
            test.populate_callout_script("bad-json.sh");
        },
    );

    // tests with broken active mdev are below
    test_list_helper(
        "active-verbose-broken-active-mdev-type",
        Expect::Pass, // broken mdev type link contains mdev type!
        false,
        true,
        None,
        None,
        |test| {
            setup(test);
            test.populate_broken_active_device_links(UUID[5], PARENT[0], MDEV_TYPE[1], false, true);
        },
    );
    test_list_helper(
        "active-verbose-removed-active-mdev-type",
        Expect::Pass, // removed mdev type file
        false,
        true,
        None,
        None,
        |test| {
            setup(test);
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[0],
                MDEV_TYPE[1],
                false,
                true,
            );
        },
    );
    test_list_helper(
        "active-verbose-broken-active-parent",
        Expect::Pass,
        false,
        true,
        None,
        None,
        |test| {
            setup(test);
            test.populate_broken_active_device_links(UUID[5], PARENT[0], MDEV_TYPE[1], true, false);
        },
    );
    test_list_helper(
        "active-verbose-removed-active-parent",
        Expect::Fail(Some("Device must have a defined mdev_type")),
        false,
        true,
        None,
        None,
        |test| {
            setup(test);
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[0],
                MDEV_TYPE[1],
                true,
                false,
            );
        },
    );
    test_list_helper(
        "active-parent-verbose-broken-active-mdev-type",
        Expect::Pass, // broken mdev missing (two reg with parent and one returned)
        false,
        true,
        None,
        Some(PARENT[0].to_string()),
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[0], MDEV_TYPE[1], false, true);
            setup(test);
        },
    );
    test_list_helper(
        "active-parent-verbose-removed-active-mdev-type",
        Expect::Pass, // removed mdev type file (two reg with parent and one returned)
        false,
        true,
        None,
        Some(PARENT[0].to_string()),
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[0],
                MDEV_TYPE[1],
                false,
                true,
            );
            setup(test);
        },
    );
    test_list_helper(
        "active-parent-verbose-broken-active-parent",
        Expect::Pass, // broken mdev missing (two reg with parent and one returned)
        false,
        true,
        None,
        Some(PARENT[0].to_string()),
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[0], MDEV_TYPE[1], true, false);
            setup(test);
        },
    );
    test_list_helper(
        "active-parent-verbose-removed-active-parent",
        Expect::Pass, // removed mdev type file
        false,
        true,
        None,
        Some(PARENT[0].to_string()),
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[0],
                MDEV_TYPE[1],
                true,
                false,
            );
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-verbose-broken-active-mdev-type",
        Expect::Pass, // broken link still provides mdev type (one reg with uuid and one returned)
        false,
        true,
        Some(UUID[5].to_string()),
        None,
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[1], MDEV_TYPE[1], false, true);
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-verbose-removed-active-mdev-type",
        Expect::Pass, // removed mdev type file
        false,
        true,
        Some(UUID[5].to_string()),
        None,
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[1],
                MDEV_TYPE[1],
                false,
                true,
            );
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-verbose-broken-active-parent",
        Expect::Pass, // broken link still provides mdev type (one reg with uuid and one returned)
        false,
        true,
        Some(UUID[5].to_string()),
        None,
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[1], MDEV_TYPE[1], true, false);
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-verbose-removed-active-parent",
        Expect::Fail(Some("Device must have a defined mdev_type")), // removed mdev type file
        false,
        true,
        Some(UUID[5].to_string()),
        None,
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[1],
                MDEV_TYPE[1],
                true,
                false,
            );
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-parent-verbose-broken-active-mdev-type",
        Expect::Pass, // broken mdev missing (one reg and none returned)
        false,
        true,
        Some(UUID[5].to_string()),
        Some(PARENT[1].to_string()),
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[1], MDEV_TYPE[1], false, true);
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-parent-verbose-removed-active-mdev-type",
        Expect::Pass, // removed mdev type file (one reg and none returned)
        false,
        true,
        Some(UUID[5].to_string()),
        Some(PARENT[1].to_string()),
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[1],
                MDEV_TYPE[1],
                false,
                true,
            );
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-parent-verbose-broken-active-parent",
        Expect::Pass,
        false,
        true,
        Some(UUID[5].to_string()),
        Some(PARENT[1].to_string()),
        |test| {
            test.populate_broken_active_device_links(UUID[5], PARENT[1], MDEV_TYPE[1], true, false);
            setup(test);
        },
    );
    test_list_helper(
        "active-uuid-parent-verbose-removed-active-parent",
        Expect::Pass, // removed mdev type (one reg and none returned)
        false,
        true,
        Some(UUID[5].to_string()),
        Some(PARENT[1].to_string()),
        |test| {
            test.populate_removed_active_device_attributes(
                UUID[5],
                PARENT[1],
                MDEV_TYPE[1],
                true,
                false,
            );
            setup(test);
        },
    );
}
