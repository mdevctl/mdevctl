use super::*;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
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
    live: bool,
    defined: bool,
    jsonfile: Option<PathBuf>,
    force: bool,
    setupfn: F,
) where
    F: Fn(Rc<TestEnvironment>),
{
    use crate::modify_command;
    let test = TestEnvironment::new("modify", testname);
    let env: Rc<dyn Environment> = test.clone();

    // load the jsonfile from the test path.
    let jsonfile = jsonfile.map(|f| test.datapath.join(f));

    setupfn(test.clone());

    let uuid = Uuid::parse_str(uuid).unwrap();
    let result = modify_command(
        env.clone(),
        uuid,
        parent.clone(),
        mdev_type,
        addattr,
        delattr,
        index,
        value,
        auto,
        manual,
        live,
        defined,
        jsonfile,
        force,
    );

    let testfilename = format!("{}.expected", testname);
    if test.assert_result(result, expect, None).is_err() {
        test.unused_file(&testfilename);
        return;
    }

    let def = test
        .clone()
        .get_defined_device(uuid, parent.as_ref())
        .expect("Couldn't find defined device");
    let path = def.persist_path().unwrap();
    assert!(path.exists());
    assert!(def.is_defined());
    let filecontents = fs::read_to_string(&path).unwrap();
    test.clone().compare_to_file(&testfilename, &filecontents);
}

#[allow(clippy::too_many_arguments)]
fn test_modify_defined_active_helper<F>(
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
    live: bool,
    defined: bool,
    jsonfile: Option<PathBuf>,
    force: bool,
    setupfn: F,
) where
    F: Fn(Rc<TestEnvironment>),
{
    use crate::modify_command;
    let test = TestEnvironment::new("modify", testname);
    let env: Rc<dyn Environment> = test.clone();

    // load the jsonfile from the test path.
    let jsonfile = jsonfile.map(|f| test.datapath.join(f));

    setupfn(test.clone());

    let uuid = Uuid::parse_str(uuid).unwrap();
    let result = modify_command(
        env.clone(),
        uuid,
        parent.clone(),
        mdev_type,
        addattr,
        delattr,
        index,
        value,
        auto,
        manual,
        live,
        defined,
        jsonfile,
        force,
    );
    let active_expect_testfilename = format!("{}.active.expected", testname);
    let defined_expect_testfilename = format!("{}.defined.expected", testname);
    if test
        .assert_result(result, expect, Some("modify command"))
        .is_err()
    {
        let active_expect_testfilename = format!("{}.active.expected", testname);
        test.unused_file(&active_expect_testfilename);
        test.unused_file(&defined_expect_testfilename);
        return;
    }

    let def_active = test
        .clone()
        .get_active_device(uuid, parent.as_ref())
        .expect("Couldn't find defined device");
    assert!(def_active.active);
    let def_json = serde_json::to_string_pretty(
        &def_active
            .to_json(false)
            .expect("Couldn't get json from active device"),
    )
    .expect("Couldn't get json from active device");
    test.clone()
        .compare_to_file(&active_expect_testfilename, &def_json);

    let def = test
        .clone()
        .get_defined_device(uuid, parent.as_ref())
        .expect("Couldn't find defined device");
    let path = def.persist_path().unwrap();
    assert!(path.exists());
    assert!(def.is_defined());
    let filecontents = fs::read_to_string(&path).unwrap();
    test.clone()
        .compare_to_file(&defined_expect_testfilename, &filecontents);
}

#[test]
fn test_modify() {
    init();

    const UUID: &str = "976d8cc2-4bfc-43b9-b9f9-f4af2de91ab9";
    const PARENT: &str = "0000:00:03.0";
    test_modify_helper(
        "device-not-defined",
        Expect::Fail(None),
        UUID,
        None,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        None,
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
        false,
        false,
        None,
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
        false,
        false,
        None,
        false,
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
        false,
        false,
        None,
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
        false,
        false,
        None,
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
        false,
        false,
        None,
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
        false,
        false,
        None,
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
        false,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    test_modify_helper(
        "multiple-noparent",
        Expect::Fail(None),
        UUID,
        None,
        None,
        None,
        false,
        None,
        None,
        true,
        false,
        false,
        false,
        None,
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
        false,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_defined_device(UUID, "0000:00:02.0", "defined.json");
        },
    );
    test_modify_helper(
        "auto-manual",
        Expect::Fail(None),
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        true,
        true,
        false,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    // specifying via jsonfile properly
    test_modify_helper(
        "jsonfile",
        Expect::Pass,
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
        },
    );
    // callouts for device succeed
    test_modify_helper(
        "callout-pass",
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
        false,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh");
        },
    );
    // callouts for device fail
    test_modify_helper(
        "callout-fail",
        Expect::Fail(None),
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        true,
        false,
        false,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );
    // override a callout failure
    test_modify_helper(
        "callout-fail-force",
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
        false,
        false,
        None,
        true,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh");
        },
    );

    // test modify with versioning callouts
    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_VER: &str = "11111111-1111-0000-0000-000000000000";
    test_modify_helper(
        "modify-jsonfile-with-version-callout-all-pass",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_modify_helper(
        "modify-jsonfile-with-version-callout-all-fail",
        Expect::Fail(None),
        UUID_VER,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );
    test_modify_helper(
        "modify-jsonfile-with-version-callout-multiple-with-version-pass",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_modify_helper(
        "modify-jsonfile-with-version-callout-multiple-with-version-pass2",
        Expect::Pass,
        UUID_VER,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc1.sh"); // no versioning error
            test.populate_callout_script("ver-rc0.sh"); // versioning
        },
    );
    test_modify_helper(
        "modify-jsonfile-with-version-callout-multiple-with-version-fail",
        Expect::Fail(None),
        UUID_VER,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        false,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_VER, PARENT, "defined.json");
            test.populate_callout_script("rc0.sh"); // no versioning
            test.populate_callout_script("ver-rc1.sh"); // versioning error
        },
    );

    // uuid=11111111-1111-0000-0000-000000000000 has a supported version
    const UUID_NO_LIVE: &str = "11111111-1111-0000-0000-000000000000";
    const UUID_LIVE: &str = "11111111-1111-1111-0000-000000000000";

    test_modify_helper(
        "live-event-supported",
        Expect::Pass,
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("live-rc0.sh");
        },
    );
    test_modify_helper(
        "live-event-unsupported-by-callout",
        Expect::Fail(None),
        UUID_NO_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_NO_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_NO_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("live-rc0.sh");
        },
    );
    test_modify_helper(
        "live-unsupported-script-without-version-support",
        Expect::Fail(Some("'live' option must be used with 'jsonfile' option")),
        UUID,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_active_device(UUID, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("live-rc0.sh");
        },
    );
    test_modify_helper(
        "live-supported-but-fails",
        Expect::Fail(None),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("live-rc1.sh");
        },
    );
    test_modify_helper(
        "live-fail-without-jsonfile",
        Expect::Fail(Some(
            "'live' option must be used with 'jsonfile' option"
                .to_string()
                .as_str(),
        )),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("live-rc0.sh");
        },
    );

    test_modify_defined_active_helper(
        "live-defined-supported",
        Expect::Pass,
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        true,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("modify-active.sh");
        },
    );
    test_modify_defined_active_helper(
        "live-defined-live-event-unsupported",
        Expect::Fail(None),
        UUID_NO_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        true,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_NO_LIVE, PARENT, "defined.json");
            test.populate_active_device(UUID_NO_LIVE, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("modify-active.sh");
        },
    );
    test_modify_defined_active_helper(
        "defined-only",
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
        false,
        true,
        None,
        false,
        |test| {
            test.populate_defined_device(UUID, PARENT, "defined.json");
            test.populate_active_device(UUID, PARENT, "vfio_ap-passthrough");
            test.populate_callout_script("modify-active.sh");
        },
    );

    // tests with active broken mdev are below
    test_modify_helper(
        "live-event-supported-broken-active-mdev-type",
        Expect::Fail(Some(
            format!("Mediated device {PARENT}/{UUID_LIVE} is not active").as_str(),
        )),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_broken_active_device_links(
                UUID_LIVE,
                PARENT,
                "vfio_ap-passthrough",
                false,
                true,
            );
            test.populate_callout_script("live-rc0.sh");
        },
    );
    test_modify_helper(
        "live-event-supported-removed-active-mdev-type",
        Expect::Fail(Some(
            format!("Mediated device {PARENT}/{UUID_LIVE} is not active").as_str(),
        )),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_removed_active_device_attributes(
                UUID_LIVE,
                PARENT,
                "vfio_ap-passthrough",
                false,
                true,
            );
            test.populate_callout_script("live-rc0.sh");
        },
    );
    test_modify_defined_active_helper(
        "live-defined-supported-broken-active-parent",
        Expect::Fail(Some(
            format!("Mediated device {PARENT}/{UUID_LIVE} is not active").as_str(),
        )),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        true,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_broken_active_device_links(
                UUID_LIVE,
                PARENT,
                "vfio_ap-passthrough",
                true,
                false,
            );
            test.populate_callout_script("modify-active.sh");
        },
    );
    test_modify_defined_active_helper(
        "live-defined-supported-removed-active-parent",
        Expect::Fail(Some(
            format!("Mediated device {PARENT}/{UUID_LIVE} is not active").as_str(),
        )),
        UUID_LIVE,
        Some(PARENT.to_string()),
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        true,
        Some(PathBuf::from("modified.json")),
        false,
        |test| {
            test.populate_defined_device(UUID_LIVE, PARENT, "defined.json");
            test.populate_removed_active_device_attributes(
                UUID_LIVE,
                PARENT,
                "vfio_ap-passthrough",
                true,
                false,
            );
            test.populate_callout_script("modify-active.sh");
        },
    );
}
