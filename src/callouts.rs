use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::mdev::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    Pre,
    Post,
    Notify,
    Get,
    #[serde(skip_serializing)]
    #[serde(other)]
    Unknown, // used for forward compatibility to newer callout scripts
}

fn invocation_failure(path: &PathBuf, code: Option<i32>) -> anyhow::Error {
    anyhow!(
        "Script '{:?}' failed with status '{}'",
        path,
        match code {
            Some(i) => i.to_string(),
            None => "unknown".to_string(),
        }
    )
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Event::Pre => write!(f, "pre"),
            Event::Post => write!(f, "post"),
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
    version: Cow<'static, u32>,
    actions: Cow<'static, [Action]>,
    events: Cow<'static, [Event]>,
}

impl CalloutVersion {
    pub const fn new_const(
        version: &'static u32,
        actions: &'static [Action],
        events: &'static [Event],
    ) -> Self {
        Self {
            version: Cow::Borrowed(version),
            actions: Cow::Borrowed(actions),
            events: Cow::Borrowed(events),
        }
    }

    pub const V_1: CalloutVersion = CalloutVersion::new_const(
        &1,
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

    pub const V_2: CalloutVersion = CalloutVersion::new_const(
        &2,
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

impl std::fmt::Debug for CalloutScript {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("CalloutScript")
            .field("mdev_type", &self.mdev_type)
            .field("script", &self.path)
            .finish()
    }
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

    fn supports_event_action(&self, event: Event, action: Action) -> Result<()> {
        if !self.supports_action(action) {
            debug!(
                "Callout script {:?} does not support action '{:?}'",
                self.path.clone(),
                action
            );
            return Err(anyhow!(
                "Script {:?} does not support action '{:?}'",
                self.path.clone(),
                action
            ));
        }
        if !self.supports_event(event) {
            debug!(
                "Callout script {:?} does not support event '{:?}'",
                self.path.clone(),
                event
            );
            return Err(anyhow!(
                "Script {:?} does not support event '{:?}'",
                self.path.clone(),
                event
            ));
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

impl CalloutScripts {
    pub const fn new() -> Self {
        CalloutScripts {
            callouts: Vec::new(),
        }
    }

    fn parse_script_output(&self, output: Output) -> Option<CalloutVersion> {
        let stdout = String::from_utf8(output.stdout).unwrap();
        match serde_json::from_str::<CalloutExchange>(stdout.trim_end_matches('\0')) {
            Ok(ce) => match ce.supports {
                Some(cv) => Some(cv),
                None => {
                    debug!(" Callout script does not provide version supported");
                    None
                }
            },
            Err(e) => {
                debug!(
                    " Callout script has no version support (unparsable stdout): {:?}",
                    e
                );
                None
            }
        }
    }

    // For testing purposes a reset is required
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.callouts.clear();
    }

    fn lockup_callout_script(&self, mdev_type: &str) -> Option<CalloutScript> {
        for cs in self.callouts.iter() {
            if cs.mdev_type.eq_ignore_ascii_case(mdev_type) {
                return Some(cs.clone());
            }
        }
        None
    }

    pub fn find_script(&mut self, dev: &MDev) -> Option<CalloutScript> {
        // check already found scripts
        let mut dev = dev.clone();
        let callout = callout(&mut dev);
        let mdev_type = callout
            .dev
            .mdev_type()
            .expect("mdev_type is required on device");
        debug!("Looking up callout script for mdev type '{:?}'", mdev_type);
        match self.lockup_callout_script(mdev_type) {
            Some(cs) => {
                debug!(
                    "Looked up callout script for mdev type '{:?}': {:?}",
                    mdev_type, cs.path
                );
                return Some(cs);
            }
            None => {
                debug!(
                    "Lookup failed starting to search for mdev type '{:?}'",
                    mdev_type
                );
            }
        }

        let ce_ver = CalloutExchange {
            provides: Some(CalloutVersion::V_2),
            supports: None,
        };
        let json_ce_ver =
            serde_json::to_string(&ce_ver).expect("CalloutVersion JSON could not be generated");

        for dir in callout.dev.env.callout_dirs() {
            debug!(
                "Searching in directory {:?} for mdev type '{:?}'",
                mdev_type, dir
            );
            if !dir.is_dir() {
                continue;
            }

            let mut sorted_paths = dir
                .read_dir()
                .ok()?
                .filter_map(|k| k.ok().map(|e| e.path()))
                .collect::<Vec<_>>();
            sorted_paths.sort();

            for path in sorted_paths {
                match callout.invoke_script(
                    &path,
                    Event::Get,
                    Action::Capabilities,
                    Some(&json_ce_ver),
                ) {
                    Ok(output) => {
                        callout.print_err(&output, &path);
                        match output.status.code() {
                            None => {
                                warn!("callout script {:?} was terminated by a signal", path);
                                continue;
                            }
                            Some(2) => {
                                // RC 2 == unsupported
                                debug!(
                                    "Callout script {:?} does not support mdev type {:?}",
                                    path, mdev_type
                                );
                                continue;
                            }
                            _ => {
                                debug!(
                                    "Found callout script {:?} supporting mdev type {:?}",
                                    path, mdev_type
                                );
                                match self.parse_script_output(output) {
                                    Some(cv) => {
                                        debug!(" Script supports versioning: {:?}", cv);
                                        if cv.has_action(Action::Unknown) {
                                            warn!(
                                                "Callout script {:?} provides unknown Action type",
                                                dir
                                            );
                                        }
                                        if cv.has_event(Event::Unknown) {
                                            warn!(
                                                "Callout script {:?} provides unknown Event type",
                                                dir
                                            );
                                        }
                                        let cs = CalloutScript::new(path, mdev_type.clone(), cv);
                                        self.callouts.push(cs);
                                        return self.lockup_callout_script(mdev_type);
                                    }
                                    None => {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        debug!("failed to execute callout script {:?}: {:?}", path, e);
                        continue;
                    }
                };
            }
        }
        None
    }
}

pub struct Callout<'a, 'b> {
    state: State,
    script: Option<CalloutScript>,
    pub dev: &'b mut MDev<'a>,
}

pub fn callout<'a, 'b>(dev: &'b mut MDev<'a>) -> Callout<'a, 'b> {
    Callout::new(dev)
}

impl<'a, 'b> Callout<'a, 'b> {
    pub fn new(dev: &'b mut MDev<'a>) -> Callout<'a, 'b> {
        if dev.mdev_type.is_none() {
            panic!("Device dev must have a defined mdev_type!")
        }
        Callout {
            state: State::None,
            script: None,
            dev,
        }
    }

    fn find_callout_script(&self) -> Option<CalloutScript> {
        self.dev.env.find_script(self.dev)
    }

    pub fn invoke<F>(&mut self, action: Action, force: bool, func: F) -> Result<()>
    where
        F: Fn(&mut Self) -> Result<()>,
    {
        self.script = self.find_callout_script();
        if self.script.is_none() {
            debug!("No callout script with version support found");
        }

        let conf = self.dev.to_json(false)?.to_string();
        let res = self
            .callout(Event::Pre, action, Some(&conf))
            .map(|_output| ()) // can ignore output for general callouts
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
                let tmp_res = func(self);
                self.state = match tmp_res {
                    Ok(_) => State::Success,
                    Err(_) => State::Failure,
                };

                let post_res = self.callout(Event::Post, action, Some(&conf));
                if post_res.is_err() {
                    debug!("Error occurred when executing post callout script");
                }

                tmp_res
            });

        self.notify(action);
        res
    }

    pub fn get_attributes(&mut self) -> Result<serde_json::Value> {
        self.script = self.find_callout_script();
        if self.script.is_none() {
            debug!("No callout script with version support found");
        }

        match self.callout(Event::Get, Action::Attributes, None)? {
            Some(output) => {
                if output.status.success() {
                    debug!("Get attributes successfully from callout script");
                    let mut st = String::from_utf8_lossy(&output.stdout).to_string();

                    if st.is_empty() {
                        debug!(
                            "Script output for {} is empty",
                            self.dev.uuid.hyphenated().to_string()
                        );
                        return Ok(serde_json::Value::Null);
                    }

                    if &st == "[{}]" {
                        debug!(
                            "Attribute field for {} is empty",
                            self.dev.uuid.hyphenated().to_string()
                        );
                        st = "[]".to_string();
                    }
                    debug!(
                        "Script output for {} is: '{}'",
                        self.dev.uuid.hyphenated().to_string(),
                        st
                    );
                    serde_json::from_str(st.trim_end_matches('\0'))
                        .with_context(|| "Invalid JSON received from callout script")
                } else {
                    let path = &self.script.as_ref().unwrap().path;
                    self.print_err(&output, path);

                    Err(invocation_failure(path, output.status.code()))
                }
            }
            None => {
                debug!(
                    "Script execution for {} returned without error but also without output",
                    self.dev.uuid.hyphenated().to_string()
                );
                Ok(serde_json::Value::Null)
            }
        }
    }

    fn invoke_script<P: AsRef<Path>>(
        &self,
        script: P,
        event: Event,
        action: Action,
        stdin: Option<&str>,
    ) -> Result<Output> {
        debug!(
            "{}-{}: executing {:?}",
            event,
            action,
            script.as_ref().as_os_str()
        );

        let mut cmd = Command::new(script.as_ref().as_os_str());

        cmd.arg("-t")
            .arg(self.dev.mdev_type()?)
            .arg("-e")
            .arg(event.to_string())
            .arg("-a")
            .arg(action.to_string())
            .arg("-s")
            .arg(self.state.to_string())
            .arg("-u")
            .arg(self.dev.uuid.to_string())
            .arg("-p")
            .arg(self.dev.parent()?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        if let Some(input) = stdin {
            if let Some(mut child_stdin) = child.stdin.take() {
                child_stdin
                    .write_all(input.as_bytes())
                    .with_context(|| "Failed to write to stdin of command")?;
            }
        }

        child.wait_with_output().map_err(anyhow::Error::from)
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
        dir: P,
        event: Event,
        action: Action,
        stdin: Option<&str>,
    ) -> Option<(PathBuf, Output)> {
        debug!(
            "{}-{}: looking for a matching callout script for dev type '{}' in {:?}",
            event,
            action,
            self.dev.mdev_type.as_ref()?,
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
            match self.invoke_script(&path, event, action, stdin) {
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
                            self.dev.mdev_type().ok()?
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

    fn callout(
        &mut self,
        event: Event,
        action: Action,
        stdin: Option<&str>,
    ) -> Result<Option<Output>> {
        match self.script {
            Some(ref s) => {
                self.script
                    .as_ref()
                    .unwrap()
                    .supports_event_action(event, action)?;
                let output = self.invoke_script(s, event, action, stdin)?;
                self.print_err(&output, s);
                match output.status.code() {
                    None | Some(0) => Ok(Some(output)),
                    Some(n) => Err(invocation_failure(
                        &self.script.as_ref().unwrap().path,
                        Some(n),
                    )),
                }
            }
            None => {
                let mut res = Ok(None);
                for dir in self.dev.env.callout_dirs() {
                    if !dir.is_dir() {
                        continue;
                    }
                    let r = match self.invoke_first_matching_script(dir, event, action, stdin) {
                        Some((p, o)) => {
                            self.print_err(&o, &p);
                            self.script = Some(CalloutScript::new(
                                p.clone(),
                                self.dev.mdev_type().unwrap().to_string(),
                                CalloutVersion::V_1,
                            ));
                            match o.status.code() {
                                Some(0) => Ok(Some(o)),
                                Some(n) => Err(invocation_failure(&p, Some(n))),
                                None => continue,
                            }
                        }
                        None => continue,
                    };

                    res = r;
                    break;
                }
                res
            }
        }
    }

    fn notify(&mut self, action: Action) {
        let event = Event::Notify;
        let dirs = self.dev.env.notification_dirs();
        debug!(
            "{}-{}: executing notification scripts for device {}",
            event, action, self.dev.uuid
        );

        for dir in dirs {
            if !dir.is_dir() {
                continue;
            }

            if let Ok(readdir) = dir.read_dir() {
                for path in readdir.filter_map(|x| x.ok().map(|y| y.path())) {
                    match self.invoke_script(&path, event, action, None) {
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
