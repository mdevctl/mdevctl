use anyhow::{anyhow, Context, Result};
use log::{debug, warn};
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::mdev::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Event {
    Pre,
    Post,
    Notify,
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
        }
    }
}

#[derive(Clone, Copy)]
pub enum Action {
    Start,
    Stop,
    Define,
    Undefine,
    Modify,
}

impl Display for Action {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Action::Start => write!(f, "start"),
            Action::Stop => write!(f, "stop"),
            Action::Define => write!(f, "define"),
            Action::Undefine => write!(f, "undefine"),
            Action::Modify => write!(f, "modify"),
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

pub struct Callout {
    state: State,
    script: Option<PathBuf>,
}

impl Callout {
    fn new() -> Callout {
        Callout {
            state: State::None,
            script: None,
        }
    }

    pub fn invoke<F>(dev: &mut MDev, action: Action, func: F) -> Result<()>
    where
        F: Fn(&mut MDev) -> Result<()>,
    {
        let mut c = Callout::new();

        let res = c.callout(dev, Event::Pre, action).and_then(|_| {
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

        let _ = c.notify(dev, action);
        res
    }

    fn invoke_script<P: AsRef<Path>>(
        &self,
        dev: &mut MDev,
        script: P,
        event: Event,
        action: Action,
    ) -> Result<Output> {
        debug!(
            "{} callout: executing {:?}",
            event.to_string(),
            script.as_ref().as_os_str()
        );

        let mut cmd = Command::new(script.as_ref().as_os_str());

        cmd.arg("-t")
            .arg(dev.mdev_type()?)
            .arg("-e")
            .arg(event.to_string())
            .arg("-a")
            .arg(action.to_string())
            .arg("-s")
            .arg(self.state.to_string())
            .arg("-u")
            .arg(dev.uuid.to_string())
            .arg("-p")
            .arg(dev.parent()?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        let conf = dev.to_json(false)?.to_string();
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin
                .write_all(conf.as_bytes())
                .with_context(|| "Failed to write to stdin of command")?;
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

    fn invoke_first_matching_script<P: AsRef<Path>>(
        &self,
        dev: &mut MDev,
        dir: P,
        event: Event,
        action: Action,
    ) -> Option<(PathBuf, Output)> {
        if dir.as_ref().read_dir().ok()?.count() == 0 {
            return None;
        }

        for s in dir.as_ref().read_dir().ok()? {
            let path = s.ok()?.path();

            match self.invoke_script(dev, &path, event, action).ok() {
                Some(res) => {
                    if res.status.code().is_none() {
                        warn!("Script was terminated by a signal");
                        continue;
                    } else if res.status.code() != Some(2) {
                        return Some((path, res));
                    } else {
                        debug!(
                            "Device type {} unmatched by callout script",
                            dev.mdev_type().ok()?
                        );
                    }
                }
                _ => {
                    debug!("failed to execute callout script {:?}", path);
                    continue;
                }
            }
        }
        None
    }

    fn callout(&mut self, dev: &mut MDev, event: Event, action: Action) -> Result<()> {
        let rc = match self.script {
            Some(ref s) => self
                .invoke_script(dev, s, event, action)
                .ok()
                .and_then(|output| {
                    self.print_err(&output, s);
                    output.status.code()
                }),
            _ => {
                let dir = dev.env.callout_script_base();

                if !dir.is_dir() {
                    return Ok(());
                }
                self.invoke_first_matching_script(dev, dir, event, action)
                    .and_then(|(path, output)| {
                        self.print_err(&output, &path);
                        self.script = Some(path);
                        output.status.code()
                    })
            }
        };

        match rc {
            Some(0) | None => Ok(()),
            Some(n) => Err(anyhow!(
                "callout script {:?} failed with return code {}",
                self.script.as_ref().unwrap(),
                n
            )),
        }
    }

    fn notify(&mut self, dev: &mut MDev, action: Action) -> Result<()> {
        let event = Event::Notify;
        let dir = dev.env.callout_notification_base();

        if !dir.is_dir() {
            return Ok(());
        }

        for s in dir.read_dir()? {
            let path = s?.path();

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

        Ok(())
    }
}
