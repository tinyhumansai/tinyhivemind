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
use rustix::io::Errno;
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One process, for as long as it exists.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Identity {
    pub(super) pid: u32,
    /// Start time in clock ticks since boot.
    pub(super) start: u64,
}

/// What one reading of `/proc/<pid>/stat` says.
struct Stat {
    pid: u32,
    parent: u32,
    group: u32,
    start: u64,
    /// Exited and waiting to be reaped, with no thread left.
    dead: bool,
}

impl Stat {
    /// `None` if there is no such process or the line is not understood.
    fn read(proc: &Path, pid: u32) -> Option<Self> {
        let line = fs::read_to_string(proc.join(pid.to_string()).join("stat")).ok()?;
        // The command name may itself hold spaces and parentheses.
        let (_, rest) = line.rsplit_once(')')?;
        let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
        let number = |index: usize| fields.get(index)?.parse::<u64>().ok();
        let exited = matches!(*fields.first()?, "Z" | "X");
        Some(Self {
            pid,
            parent: u32::try_from(number(1)?).ok()?,
            group: u32::try_from(number(2)?).ok()?,
            // A leader that exited before its threads reads as a zombie.
            dead: exited && number(17)? <= 1,
            start: number(19)?,
        })
    }

    fn identity(&self) -> Identity {
        Identity {
            pid: self.pid,
            start: self.start,
        }
    }
}

/// The processes of one instance.
pub(super) struct Tree {
    proc: PathBuf,
    /// The whole `NAME=value` environment entry.
    marker: Vec<u8>,
    root: Identity,
    /// Members seen alive, kept after a parent exits.
    known: BTreeSet<Identity>,
}

impl Tree {
    /// The tree under `root`, which must be running and not yet reaped, and
    /// must stay unreaped for as long as the tree is used.
    pub(super) fn new(marker: &str, root: u32) -> io::Result<Self> {
        Self::at(Path::new("/proc"), marker, root)
    }

    fn at(proc: &Path, marker: &str, root: u32) -> io::Result<Self> {
        let root = Stat::read(proc, root)
            .ok_or(io::ErrorKind::NotFound)?
            .identity();
        Ok(Self {
            proc: proc.to_path_buf(),
            marker: marker.as_bytes().to_vec(),
            root,
            known: BTreeSet::from([root]),
        })
    }

    /// Remember the present descendants of every known member.
    ///
    /// Cheap enough to repeat while the instance runs: it reads only the
    /// instance's own entries. A descendant seen here stays a member after its
    /// parents die, even if it cleared its environment and left the group.
    pub(super) fn sample(&mut self) {
        self.known.retain(|member| {
            Stat::read(&self.proc, member.pid).is_some_and(|stat| stat.identity() == *member)
        });
        let mut queue: Vec<Identity> = self.known.iter().copied().collect();
        while let Some(parent) = queue.pop() {
            for pid in self.listed_children(parent.pid) {
                if let Some(child) = Stat::read(&self.proc, pid)
                    && !self.known.contains(&child.identity())
                    && self.is_child(&child, parent)
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
    fn is_child(&self, child: &Stat, parent: Identity) -> bool {
        child.parent == parent.pid
            && Stat::read(&self.proc, parent.pid).is_some_and(|stat| stat.identity() == parent)
    }

    /// Every live member, by walking the whole process table.
    pub(super) fn scan(&mut self) -> io::Result<Vec<Identity>> {
        let mut stats = Vec::new();
        for entry in fs::read_dir(&self.proc)? {
            let name = entry?.file_name();
            let Some(pid) = name.to_str().and_then(|name| name.parse().ok()) else {
                continue;
            };
            // A descendant cannot be older than the root.
            if let Some(stat) = Stat::read(&self.proc, pid)
                && stat.start >= self.root.start
            {
                stats.push(stat);
            }
        }
        let mut members: BTreeMap<u32, Identity> = stats
            .iter()
            .filter(|stat| self.is_member(stat))
            .map(|stat| (stat.pid, stat.identity()))
            .collect();
        // Children of members, until nothing is added: the table is not
        // ordered by descent, and pids wrap.
        loop {
            let added: Vec<Identity> = stats
                .iter()
                .filter(|stat| !members.contains_key(&stat.pid))
                .filter(|stat| {
                    members.get(&stat.parent).is_some_and(|parent| {
                        Stat::read(&self.proc, stat.pid)
                            .is_some_and(|now| now.start == stat.start && self.is_child(&now, *parent))
                    })
                })
                .map(Stat::identity)
                .collect();
            if added.is_empty() {
                break;
            }
            members.extend(added.into_iter().map(|member| (member.pid, member)));
        }
        self.known.clone_from(&members.into_values().collect());
        self.known.insert(self.root);
        Ok(stats
            .iter()
            .filter(|stat| !stat.dead && self.known.contains(&stat.identity()))
            .map(Stat::identity)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    /// Whether `stat` is a member on its own evidence.
    fn is_member(&self, stat: &Stat) -> bool {
        self.known.contains(&stat.identity())
            || stat.group == self.root.pid
            || (self.is_marked(stat.pid)
                // The environment was that of the process `stat` describes.
                && Stat::read(&self.proc, stat.pid).is_some_and(|now| now.start == stat.start))
    }

    fn is_marked(&self, pid: u32) -> bool {
        fs::read(self.proc.join(pid.to_string()).join("environ"))
            .is_ok_and(|environ| environ.split(|byte| *byte == 0).any(|e| e == self.marker))
    }

    /// Signal a member found by [`Self::scan`].
    pub(super) fn signal(&self, member: Identity, signal: Signal) -> rustix::io::Result<()> {
        self::signal(&self.proc, member, signal)
    }
}

/// Signal `member`, or do nothing if it is gone.
///
/// The pidfd is opened before the identity is checked, so it names either
/// `member` or a process that was already dead when a later owner of the pid
/// was read. Either way no other process can receive the signal.
fn signal(proc: &Path, member: Identity, signal: Signal) -> rustix::io::Result<()> {
    let Some(pid) = i32::try_from(member.pid).ok().and_then(Pid::from_raw) else {
        return Ok(());
    };
    let handle = match pidfd_open(pid, PidfdFlags::empty()) {
        // EINVAL: the pid is now a thread of some other process.
        Err(Errno::SRCH | Errno::INVAL) => return Ok(()),
        opened => opened?,
    };
    if !Stat::read(proc, member.pid).is_some_and(|stat| stat.identity() == member) {
        return Ok(());
    }
    match pidfd_send_signal(handle, signal) {
        Err(Errno::SRCH) => Ok(()),
        sent => sent,
    }
}

#[cfg(test)]
/// Whether `member` is running, for tests.
fn alive(proc: &Path, member: Identity) -> bool {
    Stat::read(proc, member.pid).is_some_and(|stat| !stat.dead && stat.identity() == member)
}

#[cfg(test)]
mod test;
