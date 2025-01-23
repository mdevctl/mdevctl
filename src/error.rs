use thiserror::Error;
use uuid::Uuid;

fn format_dev(uuid: &Uuid, parent: Option<&String>) -> String {
    match parent {
        Some(parent) => format!("{parent}/{uuid}"),
        _ => uuid.to_string(),
    }
}

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("I/O Error: {0}")]
    #[allow(clippy::enum_variant_names)]
    IOError(String, std::io::Error),
    #[error("Invalid JSON file: {0}")]
    InvalidJSON(#[from] serde_json::Error),
    #[error("Invalid format for device definition: {0}")]
    DeviceFormat(String),
    #[error("Device state error: {0} [{dev}]", dev = format_dev(.1, .2.as_ref()))]
    DeviceState(String /*msg*/, Uuid, Option<String> /*parent*/),
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
    #[error("Callout error: {0}")]
    Callout(#[source] anyhow::Error),
}
