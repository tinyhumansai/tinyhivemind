//! The child itself, and which processes stopping it covers.
#[cfg(target_os = "linux")]
use super::tree::{Scan, Tree};
use super::{Containment, Error};
use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
use std::io;
use std::process::Child;
use std::sync::{Mutex, PoisonError};

/// What stopping covers on this platform.
///
/// [`Containment::Tree`] needs Linux 5.3 or later with a readable `/proc`;
/// everywhere else it is [`Containment::ProcessGroup`], and a runner whose
/// harness spawns sessions of its own must refuse to start.
#[must_use]
pub fn containment() -> Containment {
    #[cfg(target_os = "linux")]
    if super::tree::supported() {
        return Containment::Tree;
    }
    Containment::ProcessGroup
}

/// The child, until it is reaped.
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

    pub(super) fn pid(&self) -> Pid {
        self.pid
    }

    pub(super) fn id(&self) -> u32 {
        self.pid.as_raw_nonzero().get().unsigned_abs()
    }

    /// Signal the child's process group, all of it at once.
    ///
    /// Only while the child is unreaped, under the lock that reaping takes:
    /// after that the group id may be someone else's. A host that reaps the
    /// child behind the supervisor's back breaks this, until that is noticed
    /// within a tick, 20 ms. In between, the id could have gone to a new group
    /// only if the pids wrapped around in that time.
    pub(super) fn signal_group(&self, signal: Signal) {
        let child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if child.is_some() {
            // A failure means nobody there can be signalled; the look at the
            // members that follows says who remains.
            let _ = kill_process_group(self.pid, signal);
        }
    }

    /// Give up the pid, reaping the child if it exited. A child that has not
    /// is left to the system to reap when the host ends.
    pub(super) fn release(&self) {
        let child = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(mut child) = child {
            let _ = child.try_wait();
        }
    }
}

impl Drop for Root {
    /// A child nobody released, because its instance failed to start, is
    /// killed, and reaped on a thread of its own: this may run on an async
    /// worker, which must not wait.
    fn drop(&mut self) {
        self.signal_group(Signal::KILL);
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);
        if let Some(mut child) = child.take() {
            // Without a thread it stays a zombie until the host ends.
            let _ = std::thread::Builder::new().spawn(move || child.wait());
        }
    }
}

/// What one look found.
pub(super) struct Look {
    /// The pids of the members alive when they were looked at.
    pub(super) live: Vec<u32>,
    /// Whether nobody was alive, for certain.
    pub(super) settled: bool,
    /// Why they may not be all, if something could not be read.
    pub(super) failure: Option<io::Error>,
}

impl Look {
    /// Whether nobody is left.
    pub(super) fn is_proof(&self) -> bool {
        self.live.is_empty() && self.settled
    }

    /// Why this does not prove that nobody is left.
    pub(super) fn into_error(self) -> Error {
        match self.failure {
            _ if !self.live.is_empty() => Error::Survivors { pids: self.live },
            Some(failure) => Error::Table(failure),
            None => Error::Unproven,
        }
    }
}

/// The processes of one instance.
pub(super) enum Scope {
    /// The whole tree.
    #[cfg(target_os = "linux")]
    Tree(Tree),
    /// The process group, and whether it was sent SIGTERM.
    Group { asked: bool },
}

impl Scope {
    /// What `containment` covers of the child `root`, which was started in a
    /// group of its own and with `marker` in its environment.
    pub(super) fn of(containment: Containment, marker: &str, root: &Root) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        if containment == Containment::Tree {
            return Ok(Self::Tree(Tree::new(marker, root.id())?));
        }
        // Elsewhere a group is all there is, whoever its members are.
        let _ = (containment, marker, root);
        Ok(Self::Group { asked: false })
    }

    /// Remember descendants while their parents still show whose they are.
    pub(super) fn sample(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree(tree) => tree.sample(),
            Self::Group { .. } => {}
        }
    }

    /// The child was reaped: its pid proves nothing from here on.
    pub(super) fn release(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree(tree) => tree.release(),
            Self::Group { .. } => {}
        }
    }

    /// Who is running, given whether the child exited. A child that has not is
    /// always among them. Each is sent `order` as soon as it is found, and the
    /// child's group before anyone is looked for.
    pub(super) fn look(
        &mut self,
        root: &Root,
        exited: bool,
        order: Option<Signal>,
        expired: &dyn Fn() -> bool,
    ) -> Look {
        let mut look = match self {
            #[cfg(target_os = "linux")]
            Self::Tree(tree) => {
                if order == Some(Signal::KILL) {
                    // All at once: a signalled process cannot fork.
                    root.signal_group(Signal::KILL);
                }
                let Scan {
                    live,
                    settled,
                    failure,
                } = tree.scan(order, expired);
                Look {
                    live: live.iter().map(|member| member.pid).collect(),
                    settled,
                    failure,
                }
            }
            Self::Group { asked } => {
                // A group answers at once.
                let _ = expired;
                // Once each, like everyone in a tree.
                match order {
                    Some(Signal::TERM) if std::mem::replace(asked, true) => {}
                    Some(order) => root.signal_group(order),
                    None => {}
                }
                // An unreaped child counts, so this cannot be empty before
                // `settle`.
                let gone = test_kill_process_group(root.pid()) == Err(rustix::io::Errno::SRCH);
                Look {
                    live: if gone { Vec::new() } else { vec![root.id()] },
                    settled: true,
                    failure: None,
                }
            }
        };
        if !exited && !look.live.contains(&root.id()) {
            look.live.push(root.id());
        }
        look
    }

    /// Whether members can be told apart from the child's own zombie.
    pub(super) fn sees_past_the_child(&self) -> bool {
        !matches!(self, Self::Group { .. })
    }

    /// The child exited and is to be killed with its tree: do what must come
    /// before looking for what is left.
    pub(super) fn settle(&self, root: &Root) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree(_) => {}
            // The child's zombie counts as its group's, so whether the rest is
            // gone shows only once it is reaped, and the group can be
            // signalled only before that.
            Self::Group { .. } => {
                root.signal_group(Signal::KILL);
                root.release();
            }
        }
    }
}
