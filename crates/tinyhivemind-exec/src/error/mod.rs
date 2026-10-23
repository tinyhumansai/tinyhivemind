//! The crate's typed error: every way a call can fail before, while, or after
//! its program runs.
use tinyhivemind_lang::SecretRef;

/// Why a call failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A declared `env` reference has no value.
    #[error("secret {} could not be resolved", describe(reference))]
    UnresolvedSecret {
        /// The reference that did not resolve.
        reference: SecretRef,
    },
    /// A required `{name}` placeholder named an argument the call lacks.
    #[error("{tool} requires the argument {name}")]
    MissingArgument {
        /// The tool called.
        tool: String,
        /// The argument the placeholder names.
        name: String,
    },
    /// The declaration's `command` is empty.
    #[error("{tool} declares no program")]
    EmptyCommand {
        /// The tool called.
        tool: String,
    },
    /// The program could not be started: it does not exist, is not
    /// executable, or is not on `PATH`.
    #[error("could not start {program}: {source}")]
    Spawn {
        /// The program as declared.
        program: String,
        /// Why the operating system refused.
        #[source]
        source: std::io::Error,
    },
    /// Reading from or writing to the program's pipes failed.
    #[error("{tool} pipe failed: {source}")]
    Io {
        /// The tool called.
        tool: String,
        /// The pipe error.
        #[source]
        source: std::io::Error,
    },
    /// The program exited with a status other than 0.
    #[error("{tool} exited with {}: {stderr}", status(*code))]
    Exit {
        /// The tool called.
        tool: String,
        /// The exit code; `None` when a signal ended the program.
        code: Option<i32>,
        /// The last 4 KiB of its stderr.
        stderr: String,
    },
    /// The program outlived its `timeout_ms`; its process group was killed.
    #[error("{tool} timed out after {timeout_ms} ms")]
    Timeout {
        /// The tool called.
        tool: String,
        /// The deadline it missed.
        timeout_ms: u64,
    },
    /// An `output: json` tool printed something that is not JSON.
    #[error("{tool} printed invalid json: {source}")]
    InvalidOutput {
        /// The tool called.
        tool: String,
        /// The parse error.
        #[source]
        source: serde_json::Error,
    },
}

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, Error>;

fn describe(reference: &SecretRef) -> String {
    match reference {
        SecretRef::Env(name) => format!("env:{name}"),
        SecretRef::Store(key) => format!("store:{key}"),
    }
}

fn status(code: Option<i32>) -> String {
    code.map_or_else(|| "a signal".to_owned(), |code| format!("status {code}"))
}
