//! Which processes belong to one instance, and signalling only those.
//!
//! A process is a member on any one of these proofs:
//!
//! - it is the root, whose pid the supervisor holds by not reaping it;
//! - it is in the root's process group, whose id that same held pid reserves;
//! - its environment carries the instance's marker, a random value no other
//!   process was given;
//! - its parent is a member, or was when it was last seen.
//!
//! Membership is of an [`Identity`], a pid *and* its start time, so a reused
//! pid is never mistaken for the process that had it.
mod scan;
mod stat;

pub(super) use scan::Scan;
pub(super) use stat::Identity;

use rustix::io::Errno;
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use stat::{Stat, present};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Whether the kernel offers what a [`Tree`] needs: pidfds, and a `/proc` that
/// lists children.
pub(super) fn supported() -> bool {
    Path::new("/proc/thread-self/children").exists()
        && matches!(
            Stat::read(Path::new("/proc"), std::process::id()),
            Ok(Some(_))
        )
        && pidfd_open(rustix::process::getpid(), PidfdFlags::empty()).is_ok()
}

/// Whether any readable process was started with the environment `entry`.
pub(super) fn in_use(entry: &[u8]) -> io::Result<bool> {
    for process in fs::read_dir("/proc")? {
        let environ = fs::read(process?.path().join("environ"));
        if environ.is_ok_and(|environ| environ.split(|byte| *byte == 0).any(|e| e == entry)) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What a process shows on its own.
enum Evidence {
    /// It belongs to the instance.
    Member,
    /// Its environment is empty: so far, as it is inside `exec`, or for good.
    Bare,
    /// It exited, or is gone.
    Gone,
    /// It does not belong, or nothing says that it does.
    None,
}

/// The processes of one instance.
pub(super) struct Tree {
    proc: PathBuf,
    /// The whole `NAME=value` environment entry.
    marker: Vec<u8>,
    root: Identity,
    /// The root's parent, which is the host.
    host: u32,
    /// Whether the root is unreaped, which reserves its pid as a group id.
    held: bool,
    /// Members seen alive, kept after a parent exits.
    known: BTreeSet<Identity>,
    /// Since when a process has been bare.
    bare: BTreeMap<Identity, Instant>,
    /// Already sent SIGTERM.
    asked: BTreeSet<Identity>,
}

impl Tree {
    /// The tree under `root`, which must not be reaped without
    /// [`Self::release`].
    pub(super) fn new(marker: &str, root: u32) -> io::Result<Self> {
        Self::at(Path::new("/proc"), marker, root)
    }

    /// [`Self::new`], over the process table at `proc`.
    pub(super) fn at(proc: &Path, marker: &str, root: u32) -> io::Result<Self> {
        let root = Stat::read(proc, root)?.ok_or(io::ErrorKind::NotFound)?;
        Ok(Self {
            proc: proc.to_path_buf(),
            marker: marker.as_bytes().to_vec(),
            root: root.identity(),
            host: root.parent,
            held: true,
            known: BTreeSet::from([root.identity()]),
            bare: BTreeMap::new(),
            asked: BTreeSet::new(),
        })
    }

    /// The root was reaped, so its pid proves nothing from here on.
    pub(super) fn release(&mut self) {
        self.held = false;
    }

    /// Remember the present descendants of every known member.
    ///
    /// Cheap enough to repeat while the instance runs: it reads only the
    /// instance's own entries. A descendant seen here stays a member after its
    /// parents die, even if it cleared its environment and left the group.
    /// What cannot be read is left for the next time.
    pub(super) fn sample(&mut self) {
        self.known
            .retain(|member| Stat::is_now(&self.proc, *member).unwrap_or(true));
        let known = &self.known;
        self.bare.retain(|process, _| !known.contains(process));
        let mut queue: Vec<Identity> = self.known.iter().copied().collect();
        while let Some(parent) = queue.pop() {
            for pid in self.listed_children(parent.pid) {
                if let Ok(Some(child)) = Stat::read(&self.proc, pid)
                    && !self.known.contains(&child.identity())
                    && self.is_child(&child, parent).unwrap_or(false)
                {
                    self.known.insert(child.identity());
                    queue.push(child.identity());
                }
            }
        }
    }

    /// The pids the kernel lists as children of any thread of `pid`.
    fn listed_children(&self, pid: u32) -> Vec<u32> {
        let Ok(tasks) = fs::read_dir(self.proc.join(pid.to_string()).join("task")) else {
            return Vec::new();
        };
        tasks
            .flatten()
            .filter_map(|task| fs::read_to_string(task.path().join("children")).ok())
            .flat_map(|list| {
                list.split_ascii_whitespace()
                    .filter_map(|pid| pid.parse().ok())
                    .collect::<Vec<u32>>()
            })
            .collect()
    }

    /// Whether `child` is a child of the very process `parent` names.
    ///
    /// The order matters: `child` was read naming a parent *pid* that was live
    /// at that moment, and `parent`'s identity is confirmed afterwards. If the
    /// pid still has the start time it had before, it was that process all
    /// along.
    fn is_child(&self, child: &Stat, parent: Identity) -> io::Result<bool> {
        Ok(child.parent == parent.pid && Stat::is_now(&self.proc, parent)?)
    }

    /// What `stat` shows on its own.
    fn evidence(&self, stat: &Stat) -> io::Result<Evidence> {
        if self.known.contains(&stat.identity()) || (self.held && stat.group == self.root.pid) {
            return Ok(Evidence::Member);
        }
        if stat.dead {
            return Ok(Evidence::Gone);
        }
        // A descendant cannot be older than the root, nor part of the kernel.
        if stat.start < self.root.start || stat.kernel {
            return Ok(Evidence::None);
        }
        let directory = self.proc.join(stat.pid.to_string());
        let environ = match fs::read(directory.join("environ")) {
            // Another user's, or not to be read by anyone: no evidence.
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return Ok(Evidence::None);
            }
            read => present(read)?,
        };
        let Some(environ) = environ else {
            return Ok(Evidence::Gone);
        };
        if environ.is_empty() {
            return Ok(Evidence::Bare);
        }
        let mut entries = environ.split(|byte| *byte == 0);
        if !entries.any(|entry| entry == self.marker) {
            return Ok(Evidence::None);
        }
        // A pid that names a thread shows its process's environment.
        let Some(status) = present(fs::read_to_string(directory.join("status")))? else {
            return Ok(Evidence::Gone);
        };
        let leader = status.lines().find_map(|line| line.strip_prefix("Tgid:"));
        if leader.is_some_and(|leader| leader.trim() != stat.pid.to_string()) {
            return Ok(Evidence::None);
        }
        // It was the environment of the process `stat` describes.
        if Stat::is_now(&self.proc, stat.identity())? {
            Ok(Evidence::Member)
        } else {
            Ok(Evidence::Gone)
        }
    }

    /// Signal `member`: with SIGTERM once, with anything else every time.
    fn signal(&mut self, member: Identity, signal: Signal) {
        if signal == Signal::TERM && !self.asked.insert(member) {
            return;
        }
        // One that cannot be signalled is still there at the next look.
        let _ = self::signal(&self.proc, member, signal);
    }
}

/// Signal `member`, or do nothing if it is gone.
///
/// The pidfd is opened before the identity is checked, so it names either
/// `member` or a process that was already dead when a later owner of the pid
/// was read. Either way no other process can receive the signal.
fn signal(proc: &Path, member: Identity, signal: Signal) -> io::Result<()> {
    let Some(pid) = i32::try_from(member.pid).ok().and_then(Pid::from_raw) else {
        return Ok(());
    };
    let handle = match pidfd_open(pid, PidfdFlags::empty()) {
        // EINVAL: the pid is now a thread of some other process.
        Err(Errno::SRCH | Errno::INVAL) => return Ok(()),
        opened => opened?,
    };
    if !Stat::is_now(proc, member)? {
        return Ok(());
    }
    match pidfd_send_signal(handle, signal) {
        Err(Errno::SRCH) => Ok(()),
        sent => Ok(sent?),
    }
}

#[cfg(test)]
mod test;
