use std::path::{Path, PathBuf};

pub trait Environment {
    fn root(&self) -> &Path;

    fn mdev_base(&self) -> PathBuf {
        self.root().join("sys/bus/mdev/devices")
    }

    fn persist_base(&self) -> PathBuf {
        self.root().join("etc/mdevctl.d")
    }

    fn parent_base(&self) -> PathBuf {
        self.root().join("sys/class/mdev_bus")
    }
}

#[derive(Debug)]
pub struct DefaultEnvironment {
    rootpath: PathBuf,
}

impl std::fmt::Debug for &dyn Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("mdev_base", &self.mdev_base())
            .field("persist_base", &self.persist_base())
            .field("parent_base", &self.parent_base())
            .finish()
    }
}

impl Environment for DefaultEnvironment {
    fn root(&self) -> &Path {
        self.rootpath.as_path()
    }
}

impl DefaultEnvironment {
    pub fn new() -> DefaultEnvironment {
        DefaultEnvironment {
            rootpath: PathBuf::from("/"),
        }
    }
}
