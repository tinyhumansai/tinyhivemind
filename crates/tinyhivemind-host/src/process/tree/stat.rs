//! One process, as `/proc` describes it.
use rustix::io::Errno;
use std::fs;
use std::io;
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
    /// The process `pid`, or `None` if there is none.
    pub(super) fn read(proc: &Path, pid: u32) -> io::Result<Option<Self>> {
        let Some(line) = present(fs::read(proc.join(pid.to_string()).join("stat")))? else {
            return Ok(None);
        };
        let stat = Self::parse(pid, &String::from_utf8_lossy(&line));
        // A line that cannot be read is not a process that is gone.
        let stat = stat.ok_or_else(|| io::Error::other(format!("strange stat of {pid}")));
        stat.map(Some)
    }

    fn parse(pid: u32, line: &str) -> Option<Self> {
        // The command name may itself hold spaces and parentheses.
        let (_, rest) = line.rsplit_once(')')?;
        let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
        let number = |index: usize| fields.get(index)?.parse::<u64>().ok();
        let exited = matches!(*fields.first()?, "Z" | "X");
        Some(Self {
            pid,
            parent: u32::try_from(number(1)?).ok()?,
            group: u32::try_from(number(2)?).ok()?,
            kernel: number(6)? & KERNEL_THREAD != 0,
            // A leader that exited before its threads reads as a zombie.
            dead: exited && number(17)? <= 1,
            start: number(19)?,
        })
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
