use anyhow::{Context, Result};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;

use crate::mdev::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    Pre,
    Post,
    Live,
    Notify,
    Get,
    #[serde(skip_serializing)]
    #[serde(other)]
    Unknown, // used for forward compatibility to newer callout scripts
}

#[derive(Debug)]
enum CalloutError {
    NoMatchingScript,
    InvocationFailure(PathBuf, Option<i32>),
    InvalidJSON(serde_json::Error),
    NoSupportedVersion,
    ActionNotSupported(PathBuf, Action),
    EventNotSupported(PathBuf, Event),
}

impl Display for CalloutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalloutError::NoMatchingScript => write!(f, "No matching script for device found"),
            CalloutError::InvocationFailure(p, i) => write!(
                f,
                "Script {:?} failed with status '{}'",
                p,
                match i {
                    Some(i) => i.to_string(),
                    None => "unknown".to_string(),
                }
            ),
            CalloutError::InvalidJSON(_) => {
                write!(f, "Invalid JSON received from callout script")
            }
            CalloutError::NoSupportedVersion => write!(f, "No supported version found for script"),
            CalloutError::ActionNotSupported(p, a) => {
                write!(f, "Script {p:?} does not support action '{a}'")
            }
            CalloutError::EventNotSupported(p, e) => {
                write!(f, "Script {p:?} does not support event '{e}'")
            }
        }
    }
}

impl std::error::Error for CalloutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CalloutError::InvalidJSON(e) => Some(e),
            _ => None,
        }
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Event::Pre => write!(f, "pre"),
            Event::Post => write!(f, "post"),
            Event::Live => write!(f, "live"),
            Event::Notify => write!(f, "notify"),
            Event::Get => write!(f, "get"),
            Event::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Start,
    Stop,
    Define,
    Undefine,
    Modify,
    Attributes,
    Capabilities,
    #[serde(skip_serializing)]
    Test, // used for tests only
    #[serde(skip_serializing)]
    #[serde(other)]
    Unknown, // used for forward compatibility to newer callout scripts
}

impl Display for Action {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Action::Start => write!(f, "start"),
            Action::Stop => write!(f, "stop"),
            Action::Define => write!(f, "define"),
            Action::Undefine => write!(f, "undefine"),
            Action::Modify => write!(f, "modify"),
            Action::Attributes => write!(f, "attributes"),
            Action::Capabilities => write!(f, "capabilities"),
            Action::Test => write!(f, "test"),
            Action::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct CalloutVersion {
    version: Cow<'static, str>,
    actions: Cow<'static, [Action]>,
    events: Cow<'static, [Event]>,
}

impl CalloutVersion {
    pub const fn new_const(
        version: &'static str,
        actions: &'static [Action],
        events: &'static [Event],
    ) -> Self {
        Self {
            version: Cow::Borrowed(version),
            actions: Cow::Borrowed(actions),
            events: Cow::Borrowed(events),
        }
    }

    pub const V_1_0_0: CalloutVersion = CalloutVersion::new_const(
        "1.0.0",
        &[
            Action::Start,
            Action::Stop,
            Action::Define,
            Action::Undefine,
            Action::Modify,
            Action::Attributes,
        ],
        &[Event::Pre, Event::Post, Event::Notify, Event::Get],
    );

    #[allow(dead_code)]
    pub const V_1_1_0: CalloutVersion = CalloutVersion::new_const(
        "1.1.0",
        &[
            Action::Start,
            Action::Stop,
            Action::Define,
            Action::Undefine,
            Action::Modify,
            Action::Attributes,
            Action::Capabilities,
        ],
        &[Event::Pre, Event::Post, Event::Notify, Event::Get],
    );

    pub const V_1_2_0: CalloutVersion = CalloutVersion::new_const(
        "1.2.0",
        &[
            Action::Start,
            Action::Stop,
            Action::Define,
            Action::Undefine,
            Action::Modify,
            Action::Attributes,
            Action::Capabilities,
        ],
        &[
            Event::Pre,
            Event::Post,
            Event::Notify,
            Event::Get,
            Event::Live,
        ],
    );

    pub fn has_action(&self, action: Action) -> bool {
        self.actions.contains(&action)
    }

    pub fn has_event(&self, event: Event) -> bool {
        self.events.contains(&event)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CalloutExchange {
    #[serde(skip_serializing_if = "Option::is_none")]
    provides: Option<CalloutVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supports: Option<CalloutVersion>,
}

#[derive(Clone, Copy)]
pub enum State {
    None,
    Success,
    Failure,
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            State::None => write!(f, "none"),
            State::Success => write!(f, "success"),
            State::Failure => write!(f, "failure"),
        }
    }
}

#[derive(Clone)]
pub struct CalloutScript {
    path: PathBuf,
    mdev_type: String,
    supports: CalloutVersion,
}

impl CalloutScript {
    fn new(path: PathBuf, mdev_type: String, supports: CalloutVersion) -> CalloutScript {
        CalloutScript {
            path,
            mdev_type,
            supports,
        }
    }

    fn supports_action(&self, action: Action) -> bool {
        self.supports.has_action(action)
    }

    fn supports_event(&self, event: Event) -> bool {
        self.supports.has_event(event)
    }

    fn supports_event_action(&self, event: Event, action: Action) -> Result<(), CalloutError> {
        if !self.supports_action(action) {
            debug!(
                "Callout script {:?} does not support action '{:?}'",
                self.path.clone(),
                action
            );
            return Err(CalloutError::ActionNotSupported(self.path.clone(), action));
        }
        if !self.supports_event(event) {
            debug!(
                "Callout script {:?} does not support event '{:?}'",
                self.path.clone(),
                event
            );
            return Err(CalloutError::EventNotSupported(self.path.clone(), event));
        }
        Ok(())
    }
}

impl AsRef<Path> for CalloutScript {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

pub struct CalloutScripts {
    callouts: Vec<CalloutScript>,
}

static CALLOUT_SCRIPTS: Mutex<CalloutScripts> = Mutex::new(CalloutScripts::new());

fn find_callout_script(dev: &mut MDev) -> Result<CalloutScript, CalloutError> {
    return CALLOUT_SCRIPTS.lock().unwrap().find_script(dev);
}

// For testing purposes a reset is required
#[cfg(test)]
pub fn reset_callout_scripts() {
    return CALLOUT_SCRIPTS.lock().unwrap().reset();
}

impl CalloutScripts {
    pub const fn new() -> Self {
        CalloutScripts {
            callouts: Vec::new(),
        }
    }

    // For testing purposes a reset is required
    #[cfg(test)]
    fn reset(&mut self) {
        self.callouts.clear();
    }

    fn find_script(&mut self, dev: &mut MDev) -> Result<CalloutScript, CalloutError> {
        // check already found scripts
        let mdev_type = dev.mdev_type().expect("mdev_type is required on device");
        debug!("Looking up callout script for mdev type '{:?}'", mdev_type);
        for cs in &self.callouts {
            if cs.mdev_type.eq_ignore_ascii_case(mdev_type) {
                debug!(
                    " Looked up callout script for mdev type '{:?}': {:?}",
                    mdev_type, cs.path
                );
                return Ok(cs.clone());
            }
        }
        debug!(
            "Lookup failed starting to search for mdev type '{:?}'",
            mdev_type
        );
        // search directories for mdev_type parent tuple
        for dir in dev.env.callout_dirs() {
            debug!(
                "Searching in directory {:?} for mdev type '{:?}'",
                mdev_type, dir
            );
            match self.callout_dir_search(&mut dev.clone(), dir.clone()) {
                Ok(cs) => {
                    debug!(
                        " Found callout script for mdev type '{:?}': {:?}",
                        mdev_type, cs.path
                    );
                    self.callouts.push(cs.clone());
                    return Ok(cs);
                }
                Err(CalloutError::NoMatchingScript) => {
                    debug!(" Search returned without match... continue");
                    continue;
                }
                Err(e) => {
                    debug!(" Search returned with error {:?}", e);
                    return Err(e);
                }
            }
        }
        // at this point no script was found
        debug!(
            "Searching callout script for mdev type '{:?}' ended without result",
            mdev_type
        );
        Err(CalloutError::NoMatchingScript)
    }

    fn callout_dir_search(
        &mut self,
        dev: &mut MDev,
        dir: PathBuf,
    ) -> Result<CalloutScript, CalloutError> {
        if !dir.is_dir() {
            return Err(CalloutError::NoMatchingScript);
        }
        match self.invoke_script_capability(dev, dir) {
            Some((path, cv)) => Ok(CalloutScript::new(
                path,
                dev.mdev_type().unwrap().to_string(),
                cv,
            )),
            None => Err(CalloutError::NoMatchingScript),
        }
    }

    fn parse_script_output(&self, output: Output) -> Result<CalloutVersion, CalloutError> {
        let stdout = String::from_utf8(output.stdout).unwrap();
        let ce: CalloutExchange =
            match serde_json::from_str(&stdout).map_err(CalloutError::InvalidJSON) {
                Ok(ce) => ce,
                Err(e) => return Err(e),
            };
        ce.supports.ok_or(CalloutError::NoSupportedVersion)
    }

    fn invoke_script_capability<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        dev: &mut MDev,
        dir: P,
    ) -> Option<(PathBuf, CalloutVersion)> {
        let event: Event = Event::Get;
        let action: Action = Action::Capabilities;
        let mdev_type = dev.mdev_type.as_ref()?;
        debug!(
            "{}-{}: looking for a matching callout script for dev type '{:?}' in {:?}",
            event, action, mdev_type, dir
        );

        let mut sorted_paths = dir
            .as_ref()
            .read_dir()
            .ok()?
            .filter_map(|k| k.ok().map(|e| e.path()))
            .collect::<Vec<_>>();

        sorted_paths.sort();

        for path in sorted_paths {
            let ce_ver: CalloutExchange = CalloutExchange {
                provides: Some(CalloutVersion::V_1_2_0),
                supports: None,
            };
            let json_ce_ver: String =
                serde_json::to_string(&ce_ver).expect("CalloutVersion JSON could not be generated");
            match invoke_callout_script(
                &path,
                mdev_type.clone(),
                dev.uuid.to_string(),
                dev.parent().unwrap().to_string(),
                Event::Get,
                Action::Capabilities,
                State::None,
                json_ce_ver,
            ) {
                Ok(res) => {
                    match res.status.code() {
                        None => {
                            warn!("callout script {:?} was terminated by a signal", path);
                        }
                        Some(2) => {
                            // RC 2 == unsupported
                            debug!(
                                "Callout script {:?} does not support mdev type {:?}",
                                path, mdev_type
                            );
                        }
                        _ => {
                            debug!(
                                "Found callout script {:?} supporting mdev type {:?}",
                                path, mdev_type
                            );
                            match self.parse_script_output(res) {
                                Ok(cv) => {
                                    debug!(" Script supports versioning: {:?}", cv);
                                    if cv.has_action(Action::Unknown) {
                                        warn!(
                                            "Callout script {:?} provides unknown Action type",
                                            path
                                        );
                                    }
                                    if cv.has_event(Event::Unknown) {
                                        warn!(
                                            "Callout script {:?} provides unknown Event type",
                                            path
                                        );
                                    }
                                    return Some((path, cv));
                                }
                                Err(CalloutError::InvalidJSON(e)) => {
                                    debug!(" Callout script has no version support (unparsable stdout): {:?}", e);
                                }
                                Err(CalloutError::NoSupportedVersion) => {
                                    debug!(" Callout script does not provide version supported");
                                }
                                Err(e) => {
                                    debug!(" Callout script output parsing error: {:?}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to execute callout script {:?}: {:?}", path, e);
                }
            }
        }
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn invoke_callout_script(
    script: &Path,
    mdev_type: String,
    uuid: String,
    parent: String,
    event: Event,
    action: Action,
    state: State,
    stdin: String,
) -> Result<Output> {
    debug!(
        "{}-{}: executing {:?} (mdev_type={}, uuid={}, parent={}, state={})",
        event,
        action,
        script.as_os_str(),
        mdev_type,
        uuid,
        parent,
        state.to_string()
    );

    let mut cmd = Command::new(script.as_os_str());

    cmd.arg("-t")
        .arg(mdev_type)
        .arg("-e")
        .arg(event.to_string())
        .arg("-a")
        .arg(action.to_string())
        .arg("-s")
        .arg(state.to_string())
        .arg("-u")
        .arg(uuid)
        .arg("-p")
        .arg(parent)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin
            .write_all(stdin.as_bytes())
            .context("Failed to write to stdin of command")?;
    }

    child.wait_with_output().map_err(anyhow::Error::from)
}

pub struct Callout {
    state: State,
    script: Option<CalloutScript>,
}

impl Callout {
    fn new() -> Callout {
        Callout {
            state: State::None,
            script: None,
        }
    }

    pub fn invoke_modify_live(dev: &mut MDev) -> Result<()> {
        let mut c = Callout::new();

        c.script = find_callout_script(dev).ok();
        if c.script.is_none() {
            // live is only supported when script with versioning exists
            debug!("No callout script with version support found");
            return Err(CalloutError::NoMatchingScript).map_err(anyhow::Error::from);
        }

        let mut res = Ok(());
        let mut existing = MDev::new(dev.env, dev.uuid);
        if existing.load_from_sysfs().is_ok() && existing.active {
            if existing.parent != dev.parent {
                debug!("Device exists under different parent - cannot run live update");
            } else if existing.mdev_type != dev.mdev_type {
                debug!("Device exists with different type - cannot run live update");
            } else {
                let cs = c.script.clone().unwrap();
                cs.supports_event_action(Event::Live, Action::Modify)?;
                res = c.callout(dev, Event::Live, Action::Modify);
                c.notify(dev, Action::Modify);
            }
        } // else mdev is not active
        res
    }

    pub fn invoke<F>(dev: &mut MDev, action: Action, force: bool, func: F) -> Result<()>
    where
        F: Fn(&mut MDev) -> Result<()>,
    {
        let mut c = Callout::new();

        c.script = find_callout_script(dev).ok();
        if c.script.is_none() {
            debug!("No callout script with version support found");
        }

        let res = c
            .callout(dev, Event::Pre, action)
            .or_else(|e| {
                force
                    .then(|| {
                        warn!(
                            "Forcing operation '{}' despite callout failure. Error was: {}",
                            action, e
                        );
                    })
                    .ok_or(e)
            })
            .and_then(|_| {
                let tmp_res = func(dev);
                c.state = match tmp_res {
                    Ok(_) => State::Success,
                    Err(_) => State::Failure,
                };

                let post_res = c.callout(dev, Event::Post, action);
                if post_res.is_err() {
                    debug!("Error occurred when executing post callout script");
                }

                tmp_res
            });

        c.notify(dev, action);
        res
    }

    fn parse_attribute_output(
        &self,
        dev: &mut MDev,
        path: &PathBuf,
        output: Output,
    ) -> Result<serde_json::Value, CalloutError> {
        if output.status.success() {
            debug!("Get attributes successfully from callout script");
            let mut st = String::from_utf8_lossy(&output.stdout).to_string();

            if st.is_empty() {
                return Ok(serde_json::Value::Null);
            }

            if &st == "[{}]" {
                debug!(
                    "Attribute field for {} is empty",
                    dev.uuid.hyphenated().to_string()
                );
                st = "[]".to_string();
            }

            serde_json::from_str(&st).map_err(CalloutError::InvalidJSON)
        } else {
            self.print_err(&output, path);
            Err(CalloutError::InvocationFailure(
                path.clone(),
                output.status.code(),
            ))
        }
    }

    fn get_attributes_dir(dev: &mut MDev, dir: PathBuf) -> Result<serde_json::Value, CalloutError> {
        let event = Event::Get;
        let action = Action::Attributes;
        let mut c = Callout::new();

        c.script = find_callout_script(dev).ok();
        if c.script.is_some() {
            let cs = c.script.clone().unwrap();
            cs.supports_event_action(event, action)?;
            match c.invoke_script(dev, &cs, event, action) {
                Ok(output) => {
                    return c.parse_attribute_output(dev, &cs.path, output);
                }
                Err(e) => {
                    debug!(
                        "Invocation of callout script {} failed for type {} with error: {}",
                        cs.path
                            .file_name()
                            .unwrap_or_else(|| OsStr::new("unknown script name"))
                            .to_string_lossy(),
                        dev.mdev_type.as_ref().unwrap(),
                        e
                    );
                }
            }
        } else {
            debug!("No callout script with version support found");
        };

        match c.invoke_first_matching_script(dev, dir, event, action) {
            Some((path, output)) => c.parse_attribute_output(dev, &path, output),
            None => {
                debug!(
                    "Device type {} unmatched by callout script",
                    dev.mdev_type.as_ref().unwrap()
                );
                Err(CalloutError::NoMatchingScript)
            }
        }
    }

    pub fn get_attributes(dev: &mut MDev) -> Result<serde_json::Value> {
        for dir in dev.env.callout_dirs() {
            if dir.is_dir() {
                let res = Self::get_attributes_dir(dev, dir);
                if let Err(CalloutError::NoMatchingScript) = res {
                    continue;
                }

                return res.map_err(anyhow::Error::from);
            }
        }
        Ok(serde_json::Value::Null)
    }

    fn invoke_script<P: AsRef<Path>>(
        &self,
        dev: &mut MDev,
        script: P,
        event: Event,
        action: Action,
    ) -> Result<Output> {
        debug!(
            "{}-{}: executing {:?}",
            event,
            action,
            script.as_ref().as_os_str()
        );

        let stdin = match event {
            Event::Get => String::new(),
            _ => dev.to_json(false)?.to_string(),
        };

        invoke_callout_script(
            script.as_ref(),
            dev.mdev_type().unwrap().to_string(),
            dev.uuid.to_string(),
            dev.parent().unwrap().to_string(),
            event,
            action,
            self.state,
            stdin,
        )
    }

    fn print_err<P: AsRef<Path>>(&self, output: &Output, script: P) {
        let sname = script
            .as_ref()
            .file_name()
            .unwrap_or_else(|| OsStr::new("unknown script name"))
            .to_string_lossy();

        let st = String::from_utf8_lossy(&output.stderr);
        if !st.is_empty() {
            eprint!("{}: {}", &sname, st);
        }
    }

    fn invoke_first_matching_script<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        dev: &mut MDev,
        dir: P,
        event: Event,
        action: Action,
    ) -> Option<(PathBuf, Output)> {
        debug!(
            "{}-{}: looking for a matching callout script for dev type '{}' in {:?}",
            event,
            action,
            dev.mdev_type.as_ref()?,
            dir
        );

        let mut sorted_paths = dir
            .as_ref()
            .read_dir()
            .ok()?
            .filter_map(|k| k.ok().map(|e| e.path()))
            .collect::<Vec<_>>();

        sorted_paths.sort();

        for path in sorted_paths {
            match self.invoke_script(dev, &path, event, action) {
                Ok(res) => {
                    if res.status.code().is_none() {
                        warn!("callout script {:?} was terminated by a signal", path);
                        continue;
                    } else if res.status.code() != Some(2) {
                        debug!("found callout script {:?}", path);
                        return Some((path, res));
                    } else {
                        debug!(
                            "device type {} unmatched by callout script",
                            dev.mdev_type().ok()?
                        );
                    }
                }
                Err(e) => {
                    debug!("failed to execute callout script {:?}: {:?}", path, e);
                    continue;
                }
            }
        }
        None
    }

    fn callout_dir(
        &mut self,
        dev: &mut MDev,
        event: Event,
        action: Action,
        dir: PathBuf,
    ) -> Result<(), CalloutError> {
        let rc = match self.script {
            Some(ref s) => self
                .invoke_script(dev, s, event, action)
                .ok()
                .and_then(|output| {
                    self.print_err(&output, s);
                    output.status.code()
                }),
            _ => {
                if !dir.is_dir() {
                    return Err(CalloutError::NoMatchingScript);
                }
                self.invoke_first_matching_script(dev, dir, event, action)
                    .and_then(|(path, output)| {
                        self.print_err(&output, &path);
                        self.script = Some(CalloutScript::new(
                            path,
                            dev.mdev_type().unwrap().to_string(),
                            CalloutVersion::V_1_0_0,
                        ));
                        output.status.code()
                    })
            }
        };

        match rc {
            Some(0) => Ok(()),
            Some(rc) => Err(CalloutError::InvocationFailure(
                self.script.as_ref().unwrap().as_ref().to_path_buf(),
                Some(rc),
            )),
            None => Err(CalloutError::NoMatchingScript),
        }
    }

    fn callout(&mut self, dev: &mut MDev, event: Event, action: Action) -> Result<()> {
        if self.script.is_some() {
            self.script
                .as_ref()
                .unwrap()
                .supports_event_action(event, action)
                .map_err(anyhow::Error::from)?;
        }

        for dir in dev.env.callout_dirs() {
            let res = self.callout_dir(dev, event, action, dir);

            if let Err(CalloutError::NoMatchingScript) = res {
                continue;
            }

            return res.map_err(anyhow::Error::from);
        }
        Ok(())
    }

    fn notify(&mut self, dev: &mut MDev, action: Action) {
        let event = Event::Notify;
        let dirs = dev.env.notification_dirs();
        debug!(
            "{}-{}: executing notification scripts for device {}",
            event, action, dev.uuid
        );

        for dir in dirs {
            if !dir.is_dir() {
                continue;
            }

            if let Ok(readdir) = dir.read_dir() {
                for path in readdir.filter_map(|x| x.ok().map(|y| y.path())) {
                    match self.invoke_script(dev, &path, event, action) {
                        Ok(output) => {
                            if !output.status.success() {
                                debug!("Error occurred when executing notify script {:?}", path);
                            }
                        }
                        _ => {
                            debug!("Failed to execute callout script {:?}", path);
                            continue;
                        }
                    }
                }
            }
        }
    }
}
