use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::mdev::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Pre,
    Post,
    Notify,
    Get,
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
            Event::Pre => {
                write!(f, "pre")
            }
            Event::Post => {
                write!(f, "post")
            }
            Event::Notify => {
                write!(f, "notify")
            }
            Event::Get => {
                write!(f, "get")
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum Action {
    Start,
    Stop,
    Define,
    Undefine,
    Modify,
    Attributes,
    Test, // used for tests only
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
            Action::Test => write!(f, "test"),
        }
    }
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

pub struct Callout<'a, 'b> {
    state: State,
    script: Option<PathBuf>,
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

    pub fn invoke<F>(&mut self, action: Action, force: bool, func: F) -> Result<()>
    where
        F: Fn(&mut Self) -> Result<()>,
    {
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
        match self.callout(Event::Get, Action::Attributes, None)? {
            Some(output) => {
                if output.status.success() {
                    debug!("Get attributes successfully from callout script");
                    let mut st = String::from_utf8_lossy(&output.stdout).to_string();

                    if st.is_empty() {
                        return Ok(serde_json::Value::Null);
                    }

                    if &st == "[{}]" {
                        debug!(
                            "Attribute field for {} is empty",
                            self.dev.uuid.hyphenated().to_string()
                        );
                        st = "[]".to_string();
                    }

                    serde_json::from_str(st.trim_end_matches('\0'))
                        .with_context(|| "Invalid JSON received from callout script")
                } else {
                    let path = self.script.as_ref().unwrap();
                    self.print_err(&output, path);

                    Err(invocation_failure(path, output.status.code()))
                }
            }
            None => Ok(serde_json::Value::Null),
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
                let output = self.invoke_script(s, event, action, stdin)?;
                self.print_err(&output, s);
                match output.status.code() {
                    None | Some(0) => Ok(Some(output)),
                    Some(n) => Err(invocation_failure(self.script.as_ref().unwrap(), Some(n))),
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
                            match o.status.code() {
                                Some(0) => {
                                    self.script = Some(p);
                                    Ok(Some(o))
                                }
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
