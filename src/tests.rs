#[cfg(test)]
mod tests {
    use crate::Environment;
    use crate::MdevInfo;
    use anyhow::Result;
    use log::info;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use tempdir::TempDir;
    use uuid::Uuid;

    const TEST_DATA_DIR: &str = "tests";

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[derive(Debug)]
    struct TestEnvironment {
        env: Environment,
        datapath: PathBuf,
        scratch: TempDir,
    }

    impl TestEnvironment {
        pub fn new(testname: &str, testcase: &str) -> TestEnvironment {
            let path: PathBuf = [TEST_DATA_DIR, testname, testcase].iter().collect();
            let scratchdir = TempDir::new(format!("mdevctl-{}", testname).as_str()).unwrap();
            let test = TestEnvironment {
                datapath: path,
                env: Environment::new(scratchdir.path().to_str().unwrap()),
                scratch: scratchdir,
            };
            // populate the basic directories in the environment
            fs::create_dir_all(test.env.mdev_base()).expect("Unable to create mdev_base");
            fs::create_dir_all(test.env.persist_base()).expect("Unable to create persist_base");
            fs::create_dir_all(test.env.parent_base()).expect("Unable to create parent_base");
            info!("---- Running test '{}/{}' ----", testname, testcase);
            test
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

    fn compare_to_file(filename: &PathBuf, actual: &str) {
        let flag = get_flag(REGEN_FLAG);
        if flag {
            regen(filename, actual).expect("Failed to regenerate expected output");
        }
        let expected = fs::read_to_string(filename).unwrap_or_else(|e| {
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

    fn load_from_json<'a>(
        env: &'a Environment,
        uuid: &str,
        parent: &str,
        filename: &PathBuf,
    ) -> Result<MdevInfo<'a>> {
        let uuid = Uuid::parse_str(uuid);
        assert!(uuid.is_ok());
        let uuid = uuid.unwrap();
        let mut dev = MdevInfo::new(env, uuid);

        let jsonstr = fs::read_to_string(filename)?;
        let jsonval: serde_json::Value = serde_json::from_str(&jsonstr)?;
        dev.load_from_json(parent.to_string(), &jsonval)?;

        Ok(dev)
    }

    fn test_load_json_helper(uuid: &str, parent: &str) {
        let test = TestEnvironment::new("load-json", uuid);
        let infile = test.datapath.join(format!("{}.in", uuid));
        let outfile = test.datapath.join(format!("{}.out", uuid));

        let dev = load_from_json(&test.env, uuid, parent, &infile).unwrap();
        let jsonval = dev.to_json(false).unwrap();
        let jsonstr = serde_json::to_string_pretty(&jsonval).unwrap();

        compare_to_file(&outfile, &jsonstr);
        assert_eq!(uuid, dev.uuid.to_hyphenated().to_string());
        assert_eq!(parent, dev.parent);
    }

    #[test]
    fn test_load_json() {
        init();

        test_load_json_helper("c07ab7b2-8aa2-427a-91c6-ffc949bb77f9", "0000:00:02.0");
        test_load_json_helper("783e6dbb-ea0e-411f-94e2-717eaad438bf", "0001:00:03.1");
        test_load_json_helper("5269fe7a-18d1-48ad-88e1-3fda4176f536", "0000:00:03.0");
    }
}
