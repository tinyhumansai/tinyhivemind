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
}

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, Error>;

fn describe(reference: &SecretRef) -> String {
    match reference {
        SecretRef::Env(name) => format!("env:{name}"),
        SecretRef::Store(key) => format!("store:{key}"),
    }
}
