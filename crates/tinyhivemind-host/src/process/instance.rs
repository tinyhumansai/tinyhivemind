//! One supervised child, from launch to proven stop.
use super::pipe::{JsonLines, Tail, drain};
use super::scope::{Root, Scope, containment};
use super::supervisor::{Request, Supervisor};
use super::types::{MARKER, Streams, TEMPORARY};
use super::{Containment, Error, Exit, Launch, Result, StopPolicy, directory};
use rustix::process::Signal;
use std::fmt;
use std::future::Future;
use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::sync::{oneshot, watch};

/// One supervised child and everything it spawned. See [`super`].
pub struct Instance {
    root: Arc<Root>,
    containment: Containment,
    pub(super) requests: mpsc::Sender<Request>,
    exit: watch::Receiver<Option<Exit>>,
    hurry: Arc<AtomicBool>,
    lines: Option<JsonLines>,
    stdout: Tail,
    stderr: Tail,
    pumps: Vec<tokio::task::AbortHandle>,
}

impl Instance {
    /// Start `launch` in a process group of its own. Must be called within a
    /// Tokio runtime, which drains the child's pipes.
    ///
    /// # Errors
    /// [`Error::Launch`] for a launch that is not acceptable,
    /// [`Error::Directory`] if its temporary directory cannot be made,
    /// [`Error::Spawn`] if the program cannot be started, and
    /// [`Error::Table`] if the process table cannot be read. Nothing is left
    /// running after an error.
    pub async fn start(launch: Launch) -> Result<Self> {
        Self::contained(launch, containment()).await
    }

    /// [`Self::start`] with the containment chosen.
    pub(super) async fn contained(launch: Launch, containment: Containment) -> Result<Self> {
        launch.check()?;
        let token = directory::token()?;
        let temporary = launch.dir.temporary(&token)?;
        let started = Self::spawn(&launch, containment, &token, &temporary).await;
        if started.is_err() {
            let _ = std::fs::remove_dir_all(&temporary);
        }
        started
    }

    async fn spawn(
        launch: &Launch,
        containment: Containment,
        token: &str,
        temporary: &Path,
    ) -> Result<Self> {
        let failed = |source| Error::Spawn {
            program: launch.program.clone(),
            source,
        };
        let stdin = match launch.streams {
            Streams::Logs => Stdio::null(),
            Streams::Lines(_) => Stdio::piped(),
        };
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.arguments)
            .env_clear()
            .envs(launch.environment.iter().cloned())
            .env(MARKER, token)
            .env(TEMPORARY, temporary)
            .current_dir(launch.current_dir.as_deref().unwrap_or(launch.dir.state()))
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = spawn(&mut command).await.map_err(failed)?;
        // From here on nothing is awaited: a dropped start either never got a
        // child or has handed it to an instance.
        let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let root = Arc::new(Root::new(child));
        let (requests, received) = mpsc::channel();
        let (exit, observed) = watch::channel(None);
        let mut instance = Self {
            root: root.clone(),
            containment,
            requests,
            exit: observed,
            hurry: Arc::default(),
            lines: None,
            stdout: Tail::new(launch.log_limit),
            stderr: Tail::new(launch.log_limit),
            pumps: Vec::new(),
        };
        // An error below drops `instance`, which kills the group at once.
        let scope = Scope::of(containment, &format!("{MARKER}={token}"), &root);
        Supervisor {
            root,
            scope: scope.map_err(Error::Table)?,
            requests: received,
            exit,
            hurry: instance.hurry.clone(),
            temporary: temporary.to_path_buf(),
        }
        .start()
        .map_err(failed)?;
        instance.connect(launch.streams, pipes).map_err(failed)?;
        Ok(instance)
    }

    /// Drain the child's pipes.
    fn connect(
        &mut self,
        streams: Streams,
        (stdin, stdout, stderr): (
            Option<std::process::ChildStdin>,
            Option<std::process::ChildStdout>,
            Option<std::process::ChildStderr>,
        ),
    ) -> std::io::Result<()> {
        if let Some(stderr) = stderr {
            let stderr = tokio::process::ChildStderr::from_std(stderr)?;
            self.pump(drain(stderr, self.stderr.clone()));
        }
        let Some(stdout) = stdout else {
            return Ok(());
        };
        let stdout = tokio::process::ChildStdout::from_std(stdout)?;
        if let (Streams::Lines(limits), Some(stdin)) = (streams, stdin) {
            let stdin = tokio::process::ChildStdin::from_std(stdin)?;
            let (lines, writing, reading) = JsonLines::over(stdin, stdout, limits);
            self.pump(writing);
            self.pump(reading);
            self.lines = Some(lines);
        } else {
            self.pump(drain(stdout, self.stdout.clone()));
        }
        Ok(())
    }

    fn pump(&mut self, pump: impl Future<Output = ()> + Send + 'static) {
        self.pumps.push(tokio::spawn(pump).abort_handle());
    }

    /// The child's pid, which is also its process group's id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.root.id()
    }

    /// What stopping this instance covers.
    #[must_use]
    pub fn containment(&self) -> Containment {
        self.containment
    }

    /// The child's JSON lines, once, if it was launched with them.
    pub fn take_lines(&mut self) -> Option<JsonLines> {
        self.lines.take()
    }

    /// The end of what the child wrote to stdout, unless that is JSON lines.
    /// It may hold anything the child chose to print.
    #[must_use]
    pub fn stdout(&self) -> String {
        self.stdout.text()
    }

    /// The end of what the child wrote to stderr. It may hold anything the
    /// child chose to print.
    #[must_use]
    pub fn stderr(&self) -> String {
        self.stderr.text()
    }

    /// Why a log ends before the child's output does, if one does. Nothing
    /// drains that pipe any more, so the child may block on it.
    #[must_use]
    pub fn log_failure(&self) -> Option<Error> {
        let logs = [("stdout", &self.stdout), ("stderr", &self.stderr)];
        logs.into_iter().find_map(|(stream, log)| {
            let kind = log.failure()?;
            Some(Error::Log { stream, kind })
        })
    }

    /// How the child ended, if it did.
    #[must_use]
    pub fn exit(&self) -> Option<Exit> {
        *self.exit.borrow()
    }

    /// Wait for the child itself to exit, however that happens. Its tree may
    /// live on: [`Self::stop`] is still due.
    pub fn exited(&self) -> impl Future<Output = Exit> + Send + use<> {
        let mut exit = self.exit.clone();
        async move {
            loop {
                if let Some(exit) = *exit.borrow_and_update() {
                    return exit;
                }
                if exit.changed().await.is_err() {
                    // The supervisor ended without seeing an exit: after a
                    // drop that could not prove the kill.
                    return Exit {
                        code: None,
                        signal: None,
                    };
                }
            }
        }
    }

    /// Wait for `probe`, which should keep trying until the child answers.
    ///
    /// # Errors
    /// [`Error::Exited`] if the child ends first, [`Error::NotReady`] if the
    /// probe fails and [`Error::ReadyTimeout`] after `deadline`. The instance
    /// is left running for its output to be read; stop or drop it.
    pub async fn ready<T, E: fmt::Display>(
        &self,
        probe: impl Future<Output = std::result::Result<T, E>>,
        deadline: Duration,
    ) -> Result<T> {
        tokio::select! {
            biased;
            exit = self.exited() => Err(Error::Exited(exit)),
            answer = tokio::time::timeout(deadline, probe) => match answer {
                Ok(answer) => answer.map_err(|error| Error::NotReady(error.to_string())),
                Err(_) => Err(Error::ReadyTimeout { deadline }),
            },
        }
    }

    /// The pids of every member running now, the child among them.
    ///
    /// # Errors
    /// [`Error::Table`] if the process table cannot be read,
    /// [`Error::Unproven`] if it cannot be read fast enough to find everyone,
    /// and [`Error::Unsupervised`] once the instance is stopped.
    pub async fn members(&self) -> Result<Vec<u32>> {
        let (reply, members) = oneshot::channel();
        // Either end being gone shows below.
        let _ = self.requests.send(Request::Members(reply));
        members.await.unwrap_or(Err(Error::Unsupervised))
    }

    /// Stop the whole tree and prove it gone.
    ///
    /// Runs `cancel`, the caller's own way to end work and close streams, for
    /// at most `policy.cancel`; then SIGTERM to every member, for at most
    /// `policy.terminate`; then SIGKILL until nobody is found. Each step is
    /// skipped once nothing is left to do. Stopping a stopped instance returns
    /// the same exit. Once `cancel` is over the stop runs to its end even if
    /// this future is dropped.
    ///
    /// # Errors
    /// When `policy.kill` has passed: [`Error::Survivors`] if members are still
    /// running, [`Error::Table`] if the process table could not be read, and
    /// [`Error::Unproven`] if it was not read fast enough to tell. Everyone
    /// found has been sent SIGKILL by then, but none of these means stopped;
    /// calling again tries again.
    pub async fn stop(&self, cancel: impl Future<Output = ()>, policy: StopPolicy) -> Result<Exit> {
        tokio::select! {
            _ = self.exited() => {}
            _ = tokio::time::timeout(policy.cancel, cancel) => {}
        }
        let (reply, stopped) = oneshot::channel();
        // A closed channel means an earlier stop already proved it.
        let _ = self.requests.send(Request::Stop(policy, reply));
        match stopped.await {
            Ok(stopped) => stopped,
            Err(_) => self.exit().ok_or(Error::Unsupervised),
        }
    }
}

impl Drop for Instance {
    /// Kill, without waiting: the process group is signalled before this
    /// returns, and the rest of the tree follows on the supervisor's thread.
    fn drop(&mut self) {
        self.root.signal_group(Signal::KILL);
        self.hurry.store(true, Ordering::Release);
        // Each ends with the tree, unless something that escaped it holds the
        // other end of the pipe.
        for pump in &self.pumps {
            pump.abort();
        }
        // Already stopped, if the supervisor is gone.
        let _ = self.requests.send(Request::Abandon);
    }
}

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance")
            .field("pid", &self.pid())
            .field("containment", &self.containment)
            .field("exit", &self.exit())
            .finish_non_exhaustive()
    }
}

/// Start `command`, retrying briefly while its file is open for writing.
///
/// Exec fails with "text file busy" while any process holds the program open
/// for writing, including a child this process forked between another thread
/// writing the file and closing it. Ten attempts back off for 450 ms in all.
async fn spawn(command: &mut Command) -> std::io::Result<Child> {
    const ATTEMPTS: u64 = 10;
    let mut attempt = 1;
    loop {
        match command.spawn() {
            Err(error) if error.kind() == ErrorKind::ExecutableFileBusy && attempt < ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(10 * attempt)).await;
                attempt += 1;
            }
            spawned => return spawned,
        }
    }
}
