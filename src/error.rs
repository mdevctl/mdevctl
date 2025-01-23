use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("I/O Error: {0}")]
    #[allow(clippy::enum_variant_names)]
    IOError(String, std::io::Error),
    #[error("Invalid JSON file: {0}")]
    InvalidJSON(#[from] serde_json::Error),
    #[error("Invalid format for device definition: {0}")]
    DeviceFormat(String),
    #[error("Device state error: {0}")]
    DeviceState(String),
    #[error("Unable to find parent device: {0}")]
    ParentNotFound(String),
    #[error("Device already exists: {0}")]
    DeviceExists(String),
    #[error("Unsupported configuration: {0}")]
    Unsupported(String),
    #[error("System error: {0}")]
    System(String),
    #[error("Insufficient resources: {0}")]
    InsufficientResources(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}
