//! What to launch, how long stopping may take, and what was observed.
use super::{Error, InstanceDir, Limits, Result};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Environment entry naming the instance. See [`super`] for what it proves.
pub(super) const MARKER: &str = "TINYHIVEMIND_INSTANCE";
/// Environment entry naming the private temporary directory.
pub(super) const TEMPORARY: &str = "TMPDIR";

/// How much of a child's process tree this platform can stop, and prove gone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Containment {
    /// The whole tree, as [`super`] defines it, including processes in their
    /// own session and ones whose parent died.
    Tree,
    /// Only the process group. A descendant that calls `setsid` or `setpgid`
    /// is neither stopped nor noticed, so a runner must refuse any
    /// configuration whose children do.
    ProcessGroup,
}

/// How a child ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Exit {
    /// The exit code, unless a signal ended it.
    pub code: Option<i32>,
    /// The signal that ended it.
    pub signal: Option<i32>,
}

impl Exit {
    /// Whether the child exited with code 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.code, self.signal) {
            (Some(code), _) => write!(f, "exit code {code}"),
            (None, Some(signal)) => write!(f, "signal {signal}"),
            (None, None) => f.write_str("unknown status"),
        }
    }
}

/// How long each step of [`super::Instance::stop`] may take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StopPolicy {
    /// For the caller's own cancellation.
    pub cancel: Duration,
    /// From SIGTERM until SIGKILL. Every member is sent SIGTERM at once, as
    /// systemd does, and this ends early when they are all gone.
    pub terminate: Duration,
    /// From SIGKILL until the tree must be proven gone.
    pub kill: Duration,
}

impl Default for StopPolicy {
    /// 2 s to cancel, 5 s to terminate and 10 s to prove the kill.
    fn default() -> Self {
        Self {
            cancel: Duration::from_secs(2),
            terminate: Duration::from_secs(5),
            kill: Duration::from_secs(10),
        }
    }
}

/// What the child's stdin and stdout are for. Stderr is always a log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Streams {
    /// A null stdin, and stdout kept as a log.
    Logs,
    /// One JSON value per line each way.
    Lines(Limits),
}

/// A child to start: its program, arguments, environment and directories.
///
/// The child gets exactly the environment given here, plus `TMPDIR` (a private
/// directory of its own) and `TINYHIVEMIND_INSTANCE`; nothing is inherited.
#[derive(Clone)]
pub struct Launch {
    pub(super) program: PathBuf,
    pub(super) arguments: Vec<OsString>,
    pub(super) environment: Vec<(OsString, OsString)>,
    pub(super) dir: InstanceDir,
    pub(super) current_dir: Option<PathBuf>,
    pub(super) streams: Streams,
    pub(super) log_limit: usize,
}

impl Launch {
    /// Run `program`, an absolute path, with its state in `dir`.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>, dir: InstanceDir) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
            environment: Vec::new(),
            dir,
            current_dir: None,
            streams: Streams::Logs,
            log_limit: 64 * 1024,
        }
    }
    /// Append one argument. Arguments are visible to every local user, so a
    /// secret belongs in a file or the environment instead.
    #[must_use]
    pub fn arg(mut self, argument: impl AsRef<OsStr>) -> Self {
        self.arguments.push(argument.as_ref().into());
        self
    }
    /// Add one environment entry.
    #[must_use]
    pub fn env(mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.environment
            .push((name.as_ref().into(), value.as_ref().into()));
        self
    }
    /// Run in `directory` rather than the instance's state directory.
    #[must_use]
    pub fn current_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(directory.into());
        self
    }
    /// Speak JSON lines over stdin and stdout, with the default [`Limits`].
    #[must_use]
    pub fn json_lines(self) -> Self {
        self.json_lines_within(Limits::default())
    }
    /// Speak JSON lines over stdin and stdout, within `limits`.
    #[must_use]
    pub fn json_lines_within(mut self, limits: Limits) -> Self {
        self.streams = Streams::Lines(limits);
        self
    }
    /// Keep the last `bytes` of each log instead of 64 KiB.
    #[must_use]
    pub fn log_limit(mut self, bytes: usize) -> Self {
        self.log_limit = bytes;
        self
    }

    pub(super) fn check(&self) -> Result<()> {
        if !self.program.is_absolute() {
            return Err(Error::Launch("the program is not an absolute path"));
        }
        for (name, _) in &self.environment {
            if name.is_empty() || name.as_encoded_bytes().contains(&b'=') {
                return Err(Error::Launch("an environment name is empty or holds '='"));
            }
            if name == MARKER || name == TEMPORARY {
                return Err(Error::Launch("an environment name is reserved"));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Launch {
    /// Argument and environment values may be secret, so neither is shown.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.environment.iter().map(|(name, _)| name).collect();
        f.debug_struct("Launch")
            .field("program", &self.program)
            .field("arguments", &self.arguments.len())
            .field("environment", &names)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}
