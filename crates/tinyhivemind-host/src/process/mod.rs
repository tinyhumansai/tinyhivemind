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
//!   whatever session it is in and whoever its parent is now;
//! - its parent belongs, or did when the tree was last sampled, which happens
//!   every 100 ms.
//!
//! Stopping signals every such process and repeats until three looks at the
//! process table in a row find none. One look lists the table again until it
//! shows nothing that was not examined, so a process forked while it was
//! looking is found. No other process is ever signalled: a process is
//! known by its pid *and* start time, the signal goes through a pidfd opened
//! before that identity is checked, and the group is signalled only while the
//! unreaped child keeps its id from being reused.
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
//! daemon that clears its environment. Work handed to a process that was never
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
//! membership stops counting as evidence, but stopping still works.
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
