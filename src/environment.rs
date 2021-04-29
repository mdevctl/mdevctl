use std::path::PathBuf;

#[derive(Debug)]
pub struct Environment {
    rootdir: PathBuf,
}

impl Environment {
    pub fn new(path: &str) -> Environment {
        Environment {
            rootdir: PathBuf::from(path),
        }
    }

    pub fn mdev_base(&self) -> PathBuf {
        self.rootdir.join("sys/bus/mdev/devices")
    }

    pub fn persist_base(&self) -> PathBuf {
        self.rootdir.join("etc/mdevctl.d")
    }

    pub fn parent_base(&self) -> PathBuf {
        self.rootdir.join("sys/class/mdev_bus")
    }
}
