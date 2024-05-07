use super::*;

fn test_types_helper(
    test: &Rc<TestEnvironment>,
    subtest: &str,
    expect: Expect,
    parent: Option<String>,
) {
    use crate::types_command;
    let env: Rc<dyn Environment> = test.clone();

    // test text output
    let mut outbuf: Vec<u8> = Default::default();
    let res = types_command(env.clone(), parent.clone(), false, &mut outbuf);
    if test
        .clone()
        .assert_result(res, expect, Some("text"))
        .is_ok()
    {
        test.compare_to_file(
            &format!("{}.text", subtest),
            &String::from_utf8(outbuf).expect("invalid utf8 output"),
        );
    }

    // test JSON output
    let mut outbuf: Vec<u8> = Default::default();
    let res = types_command(env.clone(), parent.clone(), true, &mut outbuf);
    if test
        .clone()
        .assert_result(res, expect, Some("json"))
        .is_ok()
    {
        test.compare_to_file(
            &format!("{}.json", subtest),
            &String::from_utf8(outbuf).expect("invalid utf8 output"),
        );
    }
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
