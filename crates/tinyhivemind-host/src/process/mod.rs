//! Supervised child processes.
//!
//! An [`Instance`] is one long-lived child and everything it spawned. It gets
//! an explicit environment, private directories and drained pipes, and
//! [`Instance::stop`] ends with the whole tree shown to be gone, or an error.
//!
//! ```
//! use std::time::Duration;
//! use tinyhivemind_host::process::{Instance, InstanceDir, Launch, StopPolicy};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let parent = tempfile::tempdir()?;
//! let dir = InstanceDir::create(parent.path().join("seat"))?;
//! let launch = Launch::new("/bin/cat", dir).env("LANG", "C").json_lines();
//! let mut instance = Instance::start(launch).await?;
//! let mut lines = instance.take_lines().ok_or("launched with lines")?;
//!
//! let hello = serde_json::json!({"op": "hello"});
//! lines.sender.send(&hello).await?;
//! let probe = async { lines.receiver.recv().await.ok_or("closed") };
//! let answer = instance.ready(probe, Duration::from_secs(10)).await??;
//! assert_eq!(answer, hello);
//!
//! // Closing stdin is how this child is asked to end.
//! let exit = instance.stop(async { drop(lines) }, StopPolicy::default()).await?;
//! assert!(exit.success());
//! # Ok(()) }
//! ```
//!
//! # What stopping covers
//!
//! [`containment`] says what this platform can do, so that a runner can refuse
//! before it starts anything.
//!
//! With [`Containment::Tree`] (Linux) a process belongs to the instance if any
//! of these is true of it:
//!
//! - it is the child;
//! - it is in the child's process group;
//! - its environment holds the instance's random `TINYHIVEMIND_INSTANCE`,
//!   whatever session it is in, and its parent, unless the host or one of the
//!   host's ancestors, is not known to be someone else's process;
//! - its parent belongs, or did when the tree was last sampled, which happens
//!   every 100 ms.
//!
//! Stopping signals the group before it looks for anyone, and everyone else the
//! moment they are found. It ends when two scans in a row settle on nobody. A
//! scan walks the whole process table once and then follows the pids handed
//! out since, so it misses neither a process forked while it was looking nor
//! one that keeps forking and exiting. Processes that plainly belong to someone
//! else do not hold it up, however many come and go. If one scan after another
//! runs out of time the result is [`Error::Unproven`], with everyone found
//! killed; anything that could not be read makes it [`Error::Table`].
//!
//! No other process is ever signalled: a process is known by its pid *and*
//! start time, the signal goes through a pidfd opened before that identity is
//! checked, and the group is signalled only while the unreaped child keeps its
//! id from being reused.
//!
//! With [`Containment::ProcessGroup`] (any other Unix, or a Linux without
//! pidfds or `/proc`) only the child's process group is signalled and shown to
//! be empty. A child that calls `setsid` or `setpgid` is neither stopped nor
//! noticed, and the proof does not speak of it.
//!
//! # What can still escape
//!
//! With [`Containment::Tree`], a process is missed, and [`Instance::stop`]
//! reports success without it, if **all** of these hold: it left the process
//! group; its environment does not show the marker, because it was started
//! with another environment, overwrote its own, or runs where this user cannot
//! read it (a set-uid or non-dumpable process); and its parent died before a
//! sample saw it. Starting such a process within 100 ms takes intent, or a
//! daemon that clears its environment. The proof further takes for granted
//! that a process this user cannot see, as with `hidepid`, or whose entry it
//! may not read, is not a member: such a process may be another user's, and a
//! member that became one is out of reach anyway. (While one of the host's
//! ancestors cannot be seen, no process is excused by its parent, as that
//! ancestor may have adopted a member.) It also takes for granted that a
//! process is not kept inside `exec` for over 200 ms, where it shows no
//! environment, and that pids do not go all the way round within one scan.
//! Work handed to a process that was never
//! a descendant, such as a system service, a container daemon or a scheduler,
//! is out of reach altogether. This contains an untidy harness; it is not a
//! sandbox for a hostile one, which runs as the same user and could kill the
//! host.
//!
//! A member this user may not signal, or one stuck in the kernel, is not
//! missed: it is [`Error::Survivors`].
//!
//! The host must not reap the child itself, by waiting for any child or by
//! ignoring `SIGCHLD`. If it does, the exit status is lost and group
//! membership stops counting as evidence, but stopping still works. For up to
//! 20 ms, until that is noticed, the group could still be signalled by an id
//! that is no longer held.
//!
//! # What dropping guarantees
//!
//! Dropping an [`Instance`] cannot wait. Before it returns, the child's process
//! group has been sent SIGKILL. The rest of the tree is then killed, the child
//! reaped and its temporary directory removed on the supervisor's own thread,
//! which needs no async runtime and goes on for up to 30 seconds. Nothing
//! reports the result, and if the host process ends first, processes outside
//! the group are left running. A host that is itself killed runs no drop at
//! all; its children then see their pipes close and nothing else.
mod directory;
mod error;
mod instance;
mod pipe;
mod scope;
mod supervisor;
#[cfg(target_os = "linux")]
mod tree;
mod types;

pub use directory::InstanceDir;
pub use error::{Error, Result};
pub use instance::Instance;
pub use pipe::{JsonLines, Limits, LineError, LineReceiver, LineSender};
pub use scope::containment;
pub use types::{Containment, Exit, Launch, StopPolicy};

#[cfg(test)]
mod test;
