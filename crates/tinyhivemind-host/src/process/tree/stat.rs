//! One process, as `/proc` describes it.
use rustix::io::Errno;
use std::fs;
use std::io::{self, Read as _};
use std::path::Path;

/// The kernel's flag for a thread of its own, which has no environment.
const KERNEL_THREAD: u64 = 0x0020_0000;

/// One process, for as long as it exists.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::process) struct Identity {
    pub(in crate::process) pid: u32,
    /// Start time in clock ticks since boot.
    pub(in crate::process) start: u64,
}

/// What one reading of `/proc/<pid>/stat` says.
pub(super) struct Stat {
    pub(super) pid: u32,
    pub(super) parent: u32,
    pub(super) group: u32,
    pub(super) start: u64,
    /// Exited and waiting to be reaped, with no thread left.
    pub(super) dead: bool,
    /// A thread of the kernel.
    pub(super) kernel: bool,
}

impl Stat {
    /// The process `pid`, or `None` if there is none. Scans stand or fall with
    /// how long this takes, so it makes three system calls and no copy.
    pub(super) fn read(proc: &Path, pid: u32) -> io::Result<Option<Self>> {
        let path = proc.join(format!("{pid}/stat"));
        let Some(mut file) = present(fs::File::open(path))? else {
            return Ok(None);
        };
        // Longer than a line can be.
        let mut line = [0; 2048];
        let Some(length) = present(file.read(&mut line))? else {
            return Ok(None);
        };
        let stat = Self::parse(pid, line.get(..length).unwrap_or_default());
        // A line that cannot be read is not a process that is gone.
        stat.ok_or_else(|| io::Error::other(format!("strange stat of {pid}")))
    }

    /// The outer `None` is a line that is not understood.
    fn parse(pid: u32, line: &[u8]) -> Option<Option<Self>> {
        // The command name may itself hold spaces and parentheses.
        let name = line.iter().rposition(|byte| *byte == b')')?;
        let rest = std::str::from_utf8(line.get(name + 1..)?).ok()?;
        let mut fields = rest.split_ascii_whitespace();
        let exited = matches!(fields.next()?, "Z" | "X");
        let parent = fields.next()?.parse().ok()?;
        // Minus one once the kernel has let go of all but the task itself.
        let Ok(group) = u32::try_from(fields.next()?.parse::<i64>().ok()?) else {
            return Some(None);
        };
        let flags: u64 = fields.nth(3)?.parse().ok()?;
        let threads: u64 = fields.nth(10)?.parse().ok()?;
        let start = fields.nth(1)?.parse().ok()?;
        Some(Some(Self {
            pid,
            parent,
            group,
            start,
            // A leader that exited before its threads reads as a zombie.
            dead: exited && threads <= 1,
            kernel: flags & KERNEL_THREAD != 0,
        }))
    }

    pub(super) fn identity(&self) -> Identity {
        Identity {
            pid: self.pid,
            start: self.start,
        }
    }

    /// Whether `identity` is running, or a zombie, now.
    pub(super) fn is_now(proc: &Path, identity: Identity) -> io::Result<bool> {
        Ok(Self::read(proc, identity.pid)?.is_some_and(|stat| stat.identity() == identity))
    }
}

/// `None` if what was read is gone. Any other failure says nothing about that.
pub(super) fn present<T>(read: io::Result<T>) -> io::Result<Option<T>> {
    match read {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.raw_os_error() == Some(Errno::SRCH.raw_os_error()) => Ok(None),
        Err(error) => Err(error),
    }
}
