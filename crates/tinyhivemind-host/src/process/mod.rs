//! Supervised child processes.
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
pub use pipe::{JsonLines, LineError, LineReceiver, LineSender, Limits};
pub use scope::containment;
pub use types::{Containment, Exit, Launch, StopPolicy};

#[cfg(test)]
mod test;
