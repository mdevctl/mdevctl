use std::path::PathBuf;

use thiserror::Error;
use uuid::Uuid;

use crate::{Action, Event};

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
    #[error("Operation unsupported: {0}")]
    Unsupported(String),
    #[error("System error: {0}")]
    System(String),
    #[error("Insufficient resources: {0}")]
    InsufficientResources(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Callout script {0:?} does not support Action '{1:?}'")]
    CalloutUnsupportedAction(PathBuf, Action),
    #[error("Callout script {0:?} does not support Event '{1:?}'")]
    CalloutUnsupportedEvent(PathBuf, Event),
    #[error("Script {0:?} failed with status '{code}", code = .1.map(|v| v.to_string()).unwrap_or("unknown".to_string()))]
    CalloutInvocationFailure(PathBuf, Option<i32>),
    #[error("Callout script {0:?} returned unexpected output: {1}")]
    CalloutUnexpectedOutput(PathBuf, String),
    #[error(transparent)]
    CalloutPrimaryCommand(anyhow::Error),
}
