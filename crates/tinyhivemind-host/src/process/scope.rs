//! The child itself, and which processes stopping it covers.
#[cfg(target_os = "linux")]
use super::tree::{Identity, Tree};
use super::{Containment, Error, Result};
use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
use std::collections::BTreeSet;
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
    /// after that the group id may be someone else's.
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

/// The processes of one instance.
pub(super) enum Scope {
    /// The whole tree.
    #[cfg(target_os = "linux")]
    Tree {
        tree: Tree,
        /// Found by the last look.
        live: Vec<Identity>,
        /// Already sent SIGTERM.
        asked: BTreeSet<Identity>,
    },
    /// The process group, and whether it was sent SIGTERM.
    Group { asked: bool },
}

impl Scope {
    /// What `containment` covers of the child `root`, which was started in a
    /// group of its own and with `marker` in its environment.
    #[cfg_attr(not(target_os = "linux"), expect(unused_variables))]
    pub(super) fn of(containment: Containment, marker: &str, root: &Root) -> io::Result<Self> {
        match containment {
            #[cfg(target_os = "linux")]
            Containment::Tree => Ok(Self::Tree {
                tree: Tree::new(marker, root.id())?,
                live: Vec::new(),
                asked: BTreeSet::new(),
            }),
            _ => Ok(Self::Group { asked: false }),
        }
    }

    /// Remember descendants while their parents still show whose they are.
    pub(super) fn sample(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree { tree, .. } => tree.sample(),
            Self::Group { .. } => {}
        }
    }

    /// The child was reaped: its pid proves nothing from here on.
    pub(super) fn release(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree { tree, .. } => tree.release(),
            Self::Group { .. } => {}
        }
    }

    /// Who is running, given whether the child exited. A child that has not
    /// is always among them.
    pub(super) fn members(&mut self, root: &Root, exited: bool) -> Result<Vec<u32>> {
        let mut members: Vec<u32> = match self {
            #[cfg(target_os = "linux")]
            Self::Tree { tree, live, .. } => {
                *live = tree.scan().map_err(Error::Table)?;
                live.iter().map(|member| member.pid).collect()
            }
            // The child's zombie counts as its group's, so whether the rest is
            // gone shows only once it is reaped, and before that the group is
            // the only thing that can be signalled.
            Self::Group { .. } if exited => {
                root.signal_group(Signal::KILL);
                root.release();
                match test_kill_process_group(root.pid()) {
                    Err(rustix::io::Errno::SRCH) => Vec::new(),
                    _ => vec![root.id()],
                }
            }
            Self::Group { .. } => Vec::new(),
        };
        if !exited && !members.contains(&root.id()) {
            members.push(root.id());
        }
        Ok(members)
    }

    /// Ask everyone found by the last look to end, once each.
    pub(super) fn terminate(&mut self, root: &Root) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree { tree, live, asked } => {
                for member in live.iter().filter(|member| asked.insert(**member)) {
                    // Refused means it outlives the grace period, and shows up
                    // again when it cannot be killed either.
                    let _ = tree.signal(*member, Signal::TERM);
                }
            }
            Self::Group { asked } => {
                if !std::mem::replace(asked, true) {
                    root.signal_group(Signal::TERM);
                }
            }
        }
    }

    /// Kill everyone found by the last look.
    pub(super) fn kill(&self, root: &Root) {
        // The group first, and at once: a signalled process cannot fork.
        root.signal_group(Signal::KILL);
        match self {
            #[cfg(target_os = "linux")]
            Self::Tree { tree, live, .. } => {
                for member in live {
                    // One that cannot be signalled is still there next look.
                    let _ = tree.signal(*member, Signal::KILL);
                }
            }
            Self::Group { .. } => {}
        }
    }
}
