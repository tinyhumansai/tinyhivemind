//! Shell children, the pids they report, and deadlines instead of sleeps.
use crate::process::{Instance, InstanceDir, Launch, StopPolicy};
use anyhow::{Context as _, Result};
use rustix::process::Pid;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Longest any condition may take on a loaded machine.
pub(super) const PATIENCE: Duration = Duration::from_secs(30);

/// Bounds that end in SIGKILL soon, for children that ignore SIGTERM.
pub(super) const SHORT: StopPolicy = StopPolicy {
    cancel: Duration::from_millis(100),
    terminate: Duration::from_millis(200),
    kill: PATIENCE,
};

/// A temporary instance, and where its children report.
pub(super) struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            root: tempfile::tempdir()?,
        })
    }
    pub(super) fn dir(&self) -> Result<InstanceDir> {
        Ok(InstanceDir::create(self.path("instance"))?)
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    /// `/bin/sh -c script`, with `$OUT` naming the report directory.
    pub(super) fn shell(&self, script: &str) -> Result<Launch> {
        Ok(Launch::new("/bin/sh", self.dir()?)
            .arg("-c")
            .arg(script)
            .env("OUT", self.root.path())
            .env("PATH", "/usr/bin:/bin"))
    }
    /// A script children can run as `sh $OUT/name`.
    pub(super) fn script(&self, name: &str, body: &str) -> Result<()> {
        Ok(fs::write(self.path(name), body)?)
    }
    pub(super) async fn start(&self, script: &str) -> Result<Instance> {
        Ok(Instance::start(self.shell(script)?).await?)
    }
    /// The pid a child wrote to `$OUT/name`.
    pub(super) async fn pid(&self, name: &str) -> Result<u32> {
        let mut text = String::new();
        until(|| {
            text = self.read(name);
            text.ends_with('\n')
        })
        .await;
        Ok(text.trim().parse()?)
    }
    /// What was written to `$OUT/name`, if anything was.
    pub(super) fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap_or_default()
    }
}

/// Poll `condition` until it holds.
pub(super) async fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "the condition never held");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Whether `pid` runs. A zombie nobody reaped yet does not.
pub(super) fn running(pid: u32) -> bool {
    let listed = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output();
    assert!(listed.is_ok(), "{listed:?}");
    let state = listed.map(|output| output.stdout).unwrap_or_default();
    !state.trim_ascii().is_empty() && !state.trim_ascii().starts_with(b"Z")
}

/// The process behind `pid`, to signal or wait for.
pub(super) fn process(pid: u32) -> Result<Pid> {
    Pid::from_raw(i32::try_from(pid)?).context("pid 0")
}

pub(super) fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}
