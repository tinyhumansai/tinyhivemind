//! Where and for whom a call runs, and how its secret references resolve.
//!
//! [`ExecContext`] carries the facts the calling convention exports to a
//! program: the package root `tools/…` programs resolve against, the seat's
//! workspace, the hive and seat identities, and the feature flags.
//! [`Secrets`] turns a declared `env` reference into its value at call time,
//! so a credential is read when the call runs and never stored here.
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tinyhivemind_lang::SecretRef;

/// Where and for whom a call runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecContext {
    /// The package directory; `tools/…` programs resolve against it and it is
    /// exported as `THM_PACKAGE_ROOT`.
    pub package_root: PathBuf,
    /// The seat's workspace, where a `cwd: workspace` tool runs; `None` runs
    /// it in the package root.
    pub workspace: Option<PathBuf>,
    /// The hive identity, exported as `THM_HIVE`.
    pub hive: String,
    /// The seat identity, exported as `THM_SEAT`.
    pub seat: String,
    /// Feature flags, each exported as `THM_FEATURE_<NAME>=1|0`.
    pub features: BTreeMap<String, bool>,
}

/// Resolves secret references at call time.
pub trait Secrets: Send + Sync + std::fmt::Debug {
    /// The value `reference` names.
    ///
    /// # Errors
    ///
    /// [`Error::UnresolvedSecret`] when the reference has no value.
    fn resolve(&self, reference: &SecretRef) -> Result<String>;
}

/// Resolves `{"env": NAME}` from this process's environment.
///
/// `{"store": …}` needs a host secret store, which this resolver does not
/// have, so it is always [`Error::UnresolvedSecret`].
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvSecrets;

impl Secrets for EnvSecrets {
    fn resolve(&self, reference: &SecretRef) -> Result<String> {
        let unresolved = || Error::UnresolvedSecret {
            reference: reference.clone(),
        };
        match reference {
            SecretRef::Env(name) => std::env::var(name).map_err(|_| unresolved()),
            SecretRef::Store(_) => Err(unresolved()),
        }
    }
}

#[cfg(test)]
mod test;
