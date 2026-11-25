//! The thread that watches one child and stops its tree.
//!
//! A thread rather than a task, so that a stop survives the future that asked
//! for it, and a drop needs no runtime. It owns the only right to reap the
//! child: until the tree is proven gone the child stays a zombie, which keeps
//! its pid, and so its process group id, from being given to anyone else.
#[cfg(target_os = "linux")]
use super::tree::{Identity, Tree};
use super::{Containment, Error, Exit, Result, StopPolicy};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, watch};

/// How often the child is checked for exit.
const TICK: Duration = Duration::from_millis(20);
/// How often the tree is sampled while the child runs.
const SAMPLE: Duration = Duration::from_millis(100);
/// The pause between looks at the tree while stopping it.
const POLL: Duration = Duration::from_millis(5);
/// Scans in a row that must find nobody. One scan is not proof: it is not
/// atomic, and a process in the middle of `exec` shows no environment.
const PROOF: u32 = 3;
/// How long a dropped instance's tree is pursued.
const DROPPED: Duration = Duration::from_secs(30);

/// What an instance asks of its supervisor.
pub(super) enum Request {
    /// Who is running now?
    Members(oneshot::Sender<Result<Vec<u32>>>),
    /// Terminate, then kill, then prove.
    Stop(StopPolicy, oneshot::Sender<Result<Exit>>),
    /// The instance was dropped: kill.
    Abandon,
}

/// The child itself, until it is reaped.
pub(super) struct Root {
    pid: Pid,
    child: Mutex<Option<Child>>,
}

impl Root {
    pub(super) fn new(child: Child) -> Self {
        Self {
            pid: Pid::from_child(&child),
            child: Mutex::new(Some(child)),
        }
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid.as_raw_nonzero().get().unsigned_abs()
    }

    /// Signal the child's process group, all of it at once.
    ///
    /// Only while the child is unreaped, under the lock that reaping takes:
    /// after that the group id may be someone else's.
    pub(super) fn signal_group(&self, signal: Signal) {
        let child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if child.is_some() {
            // Failure means the group is empty or not ours to signal; the scan
            // that follows says which.
            let _ = kill_process_group(self.pid, signal);
        }
    }

    /// Reap the child if it has exited.
    fn reap(&self) {
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let running = child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if !running {
            *child = None;
        }
    }
}

/// Which processes stopping covers.
enum Scope {
    #[cfg(target_os = "linux")]
    Tree(Tree),
    Group,
}

pub(super) struct Supervisor {
    pub(super) root: Arc<Root>,
    scope: Scope,
    pub(super) requests: Receiver<Request>,
    pub(super) exit: watch::Sender<Option<Exit>>,
    /// Set by a drop: skip the rest of the grace period.
    pub(super) hurry: Arc<AtomicBool>,
    /// Set once nothing is left.
    pub(super) proven: Arc<AtomicBool>,
    pub(super) temporary: PathBuf,
}

/// The scope `containment` names, for the running `root`.
#[cfg(target_os = "linux")]
fn scope(containment: Containment, marker: &str, root: &Root) -> std::io::Result<Scope> {
    Ok(match containment {
        Containment::Tree => Scope::Tree(Tree::new(marker, root.pid())?),
        Containment::ProcessGroup => Scope::Group,
    })
}

impl Supervisor {
    /// Watch `root` on a thread of its own.
    pub(super) fn start(self) -> std::io::Result<()> {
        thread::Builder::new()
            .name("hivemind-instance".into())
            .spawn(move || self.run())?;
        Ok(())
    }

    fn run(mut self) {
        let mut sampled = Instant::now();
        loop {
            self.observe_exit();
            match self.requests.recv_timeout(TICK) {
                Ok(Request::Members(reply)) => {
                    let _ = reply.send(self.members());
                }
                Ok(Request::Stop(policy, reply)) => {
                    let stopped = self.stop(&policy);
                    let done = stopped.is_ok();
                    let _ = reply.send(stopped);
                    if done {
                        return;
                    }
                }
                Ok(Request::Abandon) | Err(RecvTimeoutError::Disconnected) => {
                    if self.kill(DROPPED).is_err() {
                        self.root.reap();
                    }
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if sampled.elapsed() >= SAMPLE {
                self.sample();
                sampled = Instant::now();
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn sample(&mut self) {
        if let Scope::Tree(tree) = &mut self.scope {
            tree.sample();
        }
    }
    #[cfg(not(target_os = "linux"))]
    fn sample(&mut self) {}

    /// Note the child's exit without reaping it.
    fn observe_exit(&mut self) -> bool {
        if self.exit.borrow().is_some() {
            return true;
        }
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG;
        let exit = match waitid(WaitId::Pid(self.root.pid), options) {
            Ok(None) | Err(rustix::io::Errno::INTR) => return false,
            Ok(Some(status)) => Exit {
                code: status.exit_status(),
                signal: status.terminating_signal(),
            },
            // The host reaped a child that was not its to reap, or ignores
            // SIGCHLD. The pid is no longer held, so nothing may rest on it.
            Err(_) => {
                self.lose_root();
                Exit {
                    code: None,
                    signal: None,
                }
            }
        };
        self.exit.send_replace(Some(exit));
        true
    }
}
