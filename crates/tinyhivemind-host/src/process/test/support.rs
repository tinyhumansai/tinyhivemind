//! Shell children, the pids they report, and deadlines instead of sleeps.
use crate::process::{Instance, InstanceDir, Launch, StopPolicy};
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
    pub(super) fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }
    pub(super) fn dir(&self) -> InstanceDir {
        InstanceDir::create(self.root.path().join("instance")).unwrap()
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    /// `/bin/sh -c script`, with `$OUT` naming the report directory.
    pub(super) fn shell(&self, script: &str) -> Launch {
        Launch::new("/bin/sh", self.dir())
            .arg("-c")
            .arg(script)
            .env("OUT", self.root.path())
            .env("PATH", "/usr/bin:/bin")
    }
    /// A script children can run as `sh $OUT/name`.
    pub(super) fn script(&self, name: &str, body: &str) {
        fs::write(self.path(name), body).unwrap();
    }
    pub(super) async fn start(&self, script: &str) -> Instance {
        Instance::start(self.shell(script)).await.unwrap()
    }
    /// The pid a child wrote to `$OUT/name`.
    pub(super) async fn pid(&self, name: &str) -> u32 {
        let path = self.path(name);
        let mut pid = None;
        until(|| {
            pid = fs::read_to_string(&path)
                .ok()
                .filter(|text| text.ends_with('\n'))
                .and_then(|text| text.trim().parse().ok());
            pid.is_some()
        })
        .await;
        pid.unwrap()
    }
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
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    !state.is_empty() && !state.starts_with('Z')
}

pub(super) fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}
