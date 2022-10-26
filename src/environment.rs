//! A filesystem environment for mdevctl

use anyhow::{anyhow, Result};
use log::debug;
use std::env;
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

    fn config_base(&self) -> PathBuf {
        self.root().join("etc/mdevctl.d")
    }

    fn parent_base(&self) -> PathBuf {
        self.root().join("sys/class/mdev_bus")
    }

    fn scripts_base(&self) -> PathBuf {
        self.config_base().join("scripts.d")
    }

    fn callout_dir(&self) -> PathBuf {
        self.scripts_base().join("callouts")
    }

    fn notification_dir(&self) -> PathBuf {
        self.scripts_base().join("notifiers")
    }

    fn self_check(&self) -> Result<()> {
        debug!("checking that the environment is sane");
        // ensure required system dirs exist. Generally distro packages or 'make install' should
        // create these dirs.
        for dir in vec![
            self.config_base(),
            self.callout_dir(),
            self.notification_dir(),
        ] {
            if !dir.exists() {
                return Err(anyhow!("Required directory {:?} doesn't exist. This may indicate a packaging or installation error", dir));
            }
        }
        Ok(())
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
            .field("config_base", &self.config_base())
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
        let root = match env::var("MDEVCTL_ENV_ROOT") {
            Ok(d) => d,
            _ => "/".to_string(),
        };
        DefaultEnvironment {
            rootpath: PathBuf::from(root),
        }
    }
}
