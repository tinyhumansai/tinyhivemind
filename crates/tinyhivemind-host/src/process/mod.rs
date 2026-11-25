//! Supervised child processes.
mod directory;
mod error;
mod pipe;
#[cfg(target_os = "linux")]
mod tree;
mod types;

pub use directory::InstanceDir;
pub use error::{Error, Result};
pub use pipe::{JsonLines, LineError, LineReceiver, LineSender, Limits};
pub use types::{Containment, Exit, Launch, StopPolicy};

#[cfg(test)]
mod test;
