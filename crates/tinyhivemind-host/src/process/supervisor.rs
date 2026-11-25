//! The thread that watches one child and stops its tree.
//!
//! A thread rather than a task, so that a stop survives the future that asked
//! for it and a drop needs no runtime. It has the only right to reap the
//! child: until the tree is proven gone the child stays a zombie, which keeps
//! its pid, and so its process group id, from being given to anyone else.
use super::scope::{Root, Scope};
use super::{Error, Exit, Result, StopPolicy};
use rustix::process::{Signal, WaitId, WaitIdOptions, waitid};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, watch};

/// How often the child is checked for exit.
const TICK: Duration = Duration::from_millis(20);
/// How often the tree is sampled while the child runs.
const SAMPLE: Duration = Duration::from_millis(100);
/// The pause between looks at the tree while stopping it.
const POLL: Duration = Duration::from_millis(5);
/// Looks in a row that must settle on nobody. One would do but for what cannot
/// be seen, such as a process kept waiting inside `exec` for very long.
const PROOF: u32 = 2;
/// Longest a question about who is running may take.
const QUERY: Duration = Duration::from_secs(10);
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

/// The thread's state.
pub(super) struct Supervisor {
    pub(super) root: Arc<Root>,
    pub(super) scope: Scope,
    pub(super) requests: Receiver<Request>,
    pub(super) exit: watch::Sender<Option<Exit>>,
    /// Set by a drop: skip the rest of the grace period.
    pub(super) hurry: Arc<AtomicBool>,
    /// Removed once the tree is proven gone.
    pub(super) temporary: PathBuf,
}

impl Supervisor {
    /// Watch the child on a thread of its own.
    pub(super) fn start(self) -> std::io::Result<()> {
        thread::Builder::new()
            .name("hivemind-instance".into())
            .spawn(move || self.run())?;
        Ok(())
    }

    fn run(mut self) {
        let mut sampled = Instant::now();
        loop {
            self.exited();
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
                    // Nobody is left to tell, and nothing more to try.
                    if self.kill(DROPPED).is_err() {
                        self.root.release();
                    }
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if sampled.elapsed() >= SAMPLE {
                self.scope.sample();
                sampled = Instant::now();
            }
        }
    }

    /// Whether the child exited, without reaping it. Publishes how.
    fn exited(&mut self) -> bool {
        if self.exit.borrow().is_some() {
            return true;
        }
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG;
        let exit = match waitid(WaitId::Pid(self.root.pid()), options) {
            Ok(None) | Err(rustix::io::Errno::INTR) => return false,
            Ok(Some(status)) => Exit {
                code: status.exit_status(),
                signal: status.terminating_signal(),
            },
            // The host reaped a child that was not its to reap, or ignores
            // SIGCHLD. The pid is no longer held, so nothing may rest on it.
            Err(_) => {
                self.root.release();
                self.scope.release();
                Exit {
                    code: None,
                    signal: None,
                }
            }
        };
        self.exit.send_replace(Some(exit));
        true
    }

    fn stop(&mut self, policy: &StopPolicy) -> Result<Exit> {
        self.terminate(policy.terminate);
        self.kill(policy.kill)
    }

    /// Who is running now.
    fn members(&mut self) -> Result<Vec<u32>> {
        let exited = self.exited();
        let started = Instant::now();
        let expired = || started.elapsed() >= QUERY;
        let look = self.scope.look(&self.root, exited, None, &expired);
        match look.failure {
            Some(failure) => Err(Error::Table(failure)),
            None if look.complete => Ok(look.live),
            None => Err(Error::Unproven),
        }
    }

    /// Ask every member to end, for as long as `bound` allows.
    fn terminate(&mut self, bound: Duration) {
        let started = Instant::now();
        let hurry = self.hurry.clone();
        let expired = || started.elapsed() >= bound || hurry.load(Ordering::Acquire);
        while !expired() {
            let exited = self.exited();
            let order = Some(Signal::TERM);
            let look = self.scope.look(&self.root, exited, order, &expired);
            if look.is_proof() || (exited && !self.scope.sees_past_the_child()) {
                return;
            }
            thread::sleep(POLL);
        }
    }

    /// Kill every member until nobody is found, and give up the child.
    fn kill(&mut self, bound: Duration) -> Result<Exit> {
        let started = Instant::now();
        let expired = || started.elapsed() >= bound;
        let mut proof = 0;
        loop {
            let exited = self.exited();
            if exited {
                self.scope.settle(&self.root);
            }
            let order = Some(Signal::KILL);
            let look = self.scope.look(&self.root, exited, order, &expired);
            if look.is_proof() {
                proof += 1;
            } else {
                proof = 0;
                if expired() {
                    return Err(look.into_error());
                }
            }
            let exit = *self.exit.borrow();
            if let Some(exit) = exit
                && proof >= PROOF
            {
                self.root.release();
                // A leftover directory is private and harmless.
                let _ = std::fs::remove_dir_all(&self.temporary);
                return Ok(exit);
            }
            thread::sleep(POLL);
        }
    }
}
