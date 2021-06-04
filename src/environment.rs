//! A filesystem environment for mdevctl

use std::path::{Path, PathBuf};

/// A trait which provides filesystem paths for certain system resources.
///
/// The main purpose of this trait is to enable testability of the mdevctl commands by abstracting
/// out the filesystem locations. Tests can implement [`Environment`] and provide filesystem paths
/// within a mock filesystem environment that will not affect the system.
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

    fn callout_script_base(&self) -> PathBuf {
        self.persist_base().join("scripts.d/callouts")
    }
}

/// A default implementation of the Environment trait which uses '/' as the filesystem root.
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
