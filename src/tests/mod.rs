use anyhow::{anyhow, Result};
use log::info;
use nix::sys::wait::waitpid;
use nix::unistd::{fork, ForkResult};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use tempfile::Builder;
use tempfile::TempDir;
use uuid::Uuid;

use crate::callouts::*;
use crate::environment::Environment;
use crate::logger::logger;
use crate::mdev::MDev;

// additional tests
mod callouts;
mod define;
mod list;
mod modify;
mod startstop;
mod types;

const TEST_DATA_DIR: &str = "testdata";

fn init() {
    let _ = logger().is_test(true).try_init();
}

#[derive(PartialEq, Clone, Copy)]
enum Expect<'a> {
    Pass,
    Fail(Option<&'a str>),
}

#[derive(Debug)]
struct TestEnvironment {
    datapath: PathBuf,
    scratch: TempDir,
    name: String,
    case: String,
    callout_scripts: Mutex<CalloutScriptCache>,
}

impl Environment for TestEnvironment {
    fn root(&self) -> &Path {
        self.scratch.path()
    }

    fn find_script(&self, dev: &MDev) -> Option<CalloutScriptInfo> {
        return self
            .callout_scripts
            .lock()
            .unwrap()
            .find_versioned_script(dev);
    }

    fn as_env(self: Rc<Self>) -> Rc<dyn Environment> {
        self.clone()
    }
}

impl TestEnvironment {
    pub fn new(testname: &str, testcase: &str) -> Rc<TestEnvironment> {
        let path: PathBuf = [TEST_DATA_DIR, testname].iter().collect();
        let scratchdir = Builder::new().prefix("mdevctl-test").tempdir().unwrap();
        let test = TestEnvironment {
            datapath: path,
            scratch: scratchdir,
            name: testname.to_owned(),
            case: testcase.to_owned(),
            callout_scripts: Mutex::new(CalloutScriptCache::new()),
        };
        // populate the basic directories in the environment
        fs::create_dir_all(test.mdev_base()).expect("Unable to create mdev_base");
        fs::create_dir_all(test.config_base()).expect("Unable to create config_base");
        fs::create_dir_all(test.parent_base()).expect("Unable to create parent_base");
        for dir in test.callout_dirs() {
            fs::create_dir_all(&dir)
                .expect(format!("Unable to create callout_dir {:?}", &dir).as_str());
        }
        for dir in test.notification_dirs() {
            fs::create_dir_all(&dir)
                .expect(format!("Unable to create notification_dir '{:?}'", &dir).as_str());
        }
        info!("---- Running test '{}/{}' ----", testname, testcase);
        Rc::new(test)
    }

    // set up a few files in the test environment to simulate an defined mediated device
    fn populate_defined_device(&self, uuid: &str, parent: &str, filename: &str) {
        let jsonfile = self.datapath.join(filename);
        let parentdir = self.config_base().join(parent);
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

    // set up a script in the test environment to simulate a callout
    fn populate_callout_script(&self, filename: &str) {
        self.populate_callout_script_full(filename, None, true)
    }

    // set up a script in the test environment to simulate a callout
    fn populate_callout_script_full(
        &self,
        filename: &str,
        destname: Option<&str>,
        default_dir: bool,
    ) {
        let calloutscriptdir: PathBuf = [TEST_DATA_DIR, "callouts"].iter().collect();
        let calloutscript = calloutscriptdir.join(filename);
        let dest = match default_dir {
            true => self.callout_dir(),
            false => self.old_callout_dir(),
        }
        .join(destname.unwrap_or(filename));
        assert!(calloutscript.exists());

        /* Because the test suite is multi-threaded, we end up having the same flaky failures
         * described in this bug: https://github.com/golang/go/issues/22315. When we copy the
         * callout script into the test environment, another thread might be in the middle of
         * forking. This fork would then inherit the open writable file descriptor from the parent.
         * If that child process file descriptor stays open until we try to execute this callout
         * script, the script will fail to run and we'll get an ETXTBSY error from the OS. In order
         * to avoid this, we need to avoid the possibility of having any open writable file
         * descriptors to executable files in the parent process that could be inherited by forks
         * in other threads. Copying executable files in a child process avoid this. */
        match unsafe { fork() }.expect("failed to fork") {
            ForkResult::Parent { child } => {
                waitpid(child, None).expect("Failed to wait for child");
            }
            ForkResult::Child => {
                fs::copy(calloutscript, &dest).expect("Unable to copy callout script");
                unsafe {
                    libc::_exit(0);
                }
            }
        }
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
        if get_flag(REGEN_FLAG) {
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

    fn load_from_json(self: &Rc<Self>, uuid: &str, parent: &str, filename: &str) -> Result<MDev> {
        let path = self.datapath.join(filename);
        let uuid = Uuid::parse_str(uuid);
        assert!(uuid.is_ok());
        let uuid = uuid.unwrap();
        let mut dev = MDev::new(self.clone(), uuid);

        let jsonstr = fs::read_to_string(path)?;
        let jsonval: serde_json::Value = serde_json::from_str(&jsonstr)?;
        dev.load_from_json(parent.to_string(), &jsonval)?;

        Ok(dev)
    }

    fn assert_result<T: std::fmt::Debug>(
        &self,
        res: Result<T>,
        expect: Expect,
        msg: Option<&str>,
    ) -> Result<T> {
        let mut testname = format!("{}/{}", self.name, self.case);
        if let Some(msg) = msg {
            testname = format!("{}/{}", testname, msg);
        }
        match expect {
            Expect::Fail(msg) => {
                let e = res.expect_err(format!("Expected {} to fail", testname).as_str());
                if let Some(msg) = msg {
                    assert_eq!(msg, e.to_string());
                }
                Err(anyhow!(e))
            }
            Expect::Pass => Ok(res.expect(format!("Expected {} to pass", testname).as_str())),
        }
    }
}

fn get_flag(varname: &str) -> bool {
    env::var(varname).map_or(false, |s| match s.trim().parse::<i32>() {
        Ok(n) if n > 0 => true,
        _ => false,
    })
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
    let test: Rc<TestEnvironment> = TestEnvironment::new("load-json", uuid);

    let res = test.load_from_json(uuid, parent, &format!("{}.in", uuid));
    if let Ok(dev) = test.assert_result(res, expect, None) {
        let jsonval = dev.to_json(false).unwrap();
        let jsonstr = serde_json::to_string_pretty(&jsonval).unwrap();

        test.compare_to_file(&format!("{}.out", uuid), &jsonstr);
        assert_eq!(uuid, dev.uuid.hyphenated().to_string());
        assert_eq!(Some(parent.to_string()), dev.parent);
    }
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
        Expect::Fail(None),
    );
    // json file has malformed attributes - an array of strings
    test_load_json_helper(
        "fe7a39db-973b-47b4-9b77-1d7b97267d59",
        "0000:00:03.0",
        Expect::Fail(None),
    );
    // json file has malformed attributes - no array
    test_load_json_helper(
        "37ccb149-a0ce-49e3-8391-a952ef07bdc2",
        "0000:00:03.0",
        Expect::Fail(None),
    );
}
