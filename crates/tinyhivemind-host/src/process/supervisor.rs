//! The thread that watches one child and stops its tree.
//!
//! A thread rather than a task, so that a stop survives the future that asked
//! for it and a drop needs no runtime. It has the only right to reap the
//! child: until the tree is proven gone the child stays a zombie, which keeps
//! its pid, and so its process group id, from being given to anyone else.
use super::directory::Temporary;
use super::scope::{Root, Scope};
use super::{Error, Exit, Result, StopPolicy};
use rustix::process::{Signal, WaitId, WaitIdOptions, waitid};
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
pub(super) const QUERY: Duration = Duration::from_secs(5);
/// How long a dropped instance's tree is pursued.
const DROPPED: Duration = Duration::from_secs(30);

/// What an instance asks of its supervisor, and whom to tell that it is taken
/// up and what came of it.
pub(super) enum Request {
    /// Who is running now?
    Members(Ticket<Vec<u32>>),
    /// Terminate, then kill, then prove.
    Stop(StopPolicy, Ticket<Exit>),
    /// The instance was dropped: kill.
    Abandon,
}

/// Where the answer to a request goes.
pub(super) struct Ticket<T> {
    pub(super) taken: oneshot::Sender<()>,
    pub(super) answer: oneshot::Sender<Result<T>>,
}

impl<T> Ticket<T> {
    /// A ticket, and where its taking up and its answer arrive.
    pub(super) fn new() -> (Self, oneshot::Receiver<()>, oneshot::Receiver<Result<T>>) {
        let (taken, up) = oneshot::channel();
        let (answer, answered) = oneshot::channel();
        (Self { taken, answer }, up, answered)
    }
}

/// What the supervisor is doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Busy {
    /// Waiting for a request, and watching.
    Idle,
    /// Working on a request that is due by then, if ever.
    Until(Option<Instant>),
}

/// The thread's state.
pub(super) struct Supervisor {
    pub(super) root: Arc<Root>,
    pub(super) scope: Scope,
    pub(super) requests: Receiver<Request>,
    pub(super) exit: watch::Sender<Option<Exit>>,
    pub(super) busy: watch::Sender<Busy>,
    /// Set by a drop: skip the rest of the grace period.
    pub(super) hurry: Arc<AtomicBool>,
    /// Removed once the tree is proven gone.
    pub(super) temporary: Option<Temporary>,
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
                Ok(Request::Members(ticket)) => {
                    self.take_up(ticket.taken, QUERY);
                    let _ = ticket.answer.send(self.members());
                }
                Ok(Request::Stop(policy, ticket)) => {
                    self.take_up(ticket.taken, policy.terminate.saturating_add(policy.kill));
                    let stopped = self.stop(&policy);
                    let done = stopped.is_ok();
                    let _ = ticket.answer.send(stopped);
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
            self.busy.send_replace(Busy::Idle);
            if sampled.elapsed() >= SAMPLE {
                self.scope.sample();
                sampled = Instant::now();
            }
        }
    }

    /// Say that a request is taken up, and by when it is to be done.
    fn take_up(&self, taken: oneshot::Sender<()>, bound: Duration) {
        self.busy
            .send_replace(Busy::Until(Instant::now().checked_add(bound)));
        let _ = taken.send(());
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

    /// Who is running now. Asked again until the answer is complete.
    fn members(&mut self) -> Result<Vec<u32>> {
        let started = Instant::now();
        let expired = || started.elapsed() >= QUERY;
        loop {
            let exited = self.exited();
            let look = self.scope.look(&self.root, exited, None, &expired);
            if let Some(failure) = look.failure {
                return Err(Error::Table(failure));
            }
            if look.complete {
                return Ok(look.live);
            }
            if expired() {
                return Err(Error::Unproven);
            }
            thread::sleep(POLL);
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
                if let Some(mut temporary) = self.temporary.take() {
                    temporary.in_use(false);
                }
                return Ok(exit);
            }
            thread::sleep(POLL);
        }
    }
}
