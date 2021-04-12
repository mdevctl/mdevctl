#[cfg(test)]
mod tests {
    use crate::MdevInfo;
    use anyhow::Result;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    const TEST_DATA_DIR: &str = "tests";

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    // get a data path for the given testname. This can be used to construct input and output
    // filenames, etc.
    fn test_path(testname: &str, testcase: &str) -> PathBuf {
        [TEST_DATA_DIR, testname, testcase].iter().collect()
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

    fn compare_to_file(filename: &PathBuf, actual: &str) {
        let flag = get_flag("MDEVCTL_TEST_REGENERATE_OUTPUT");
        if flag {
            regen(filename, actual).expect("Failed to regenerate expected output");
        }
        let expected = fs::read_to_string(filename).expect("Failed to read expected output");

        assert_eq!(expected, actual);
    }

    fn load_from_json(uuid: &str, parent: &str, filename: &PathBuf) -> Result<MdevInfo> {
        let uuid = Uuid::parse_str(uuid);
        assert!(uuid.is_ok());
        let uuid = uuid.unwrap();
        let mut dev = MdevInfo::new(uuid);

        let jsonstr = fs::read_to_string(filename)?;
        let jsonval: serde_json::Value = serde_json::from_str(&jsonstr)?;
        dev.load_from_json(parent.to_string(), &jsonval)?;

        Ok(dev)
    }

    fn test_load_json_helper(uuid: &str, parent: &str) {
        let datapath = test_path("load-json", uuid);
        let infile = datapath.join(format!("{}.in", uuid));
        let outfile = datapath.join(format!("{}.out", uuid));

        let dev = load_from_json(uuid, parent, &infile).unwrap();
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
