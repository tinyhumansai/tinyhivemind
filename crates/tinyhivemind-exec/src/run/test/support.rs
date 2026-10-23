//! Shared fixtures: the fixture package, a context over it, and secrets.
use crate::{Error, ExecContext, Result, Secrets};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tinyhivemind_lang::{ExecTool, SecretRef};

/// The package root holding `tools/*.sh`.
pub(super) fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/run/test/fixtures")
}

/// A context over the fixture package with no workspace.
pub(super) fn context() -> ExecContext {
    ExecContext {
        package_root: fixtures(),
        workspace: None,
        hive: "acme".into(),
        seat: "critic".into(),
        features: BTreeMap::new(),
    }
}

/// A tool declaration from the fields `extra` sets over a minimal one.
pub(super) fn tool(command: &[&str], extra: Value) -> ExecTool {
    let mut declaration = json!({
        "id": "probe",
        "description": "probe",
        "command": command,
    });
    if let (Some(base), Value::Object(fields)) = (declaration.as_object_mut(), extra) {
        base.extend(fields);
    }
    serde_json::from_value(declaration).expect("a valid declaration")
}

/// Resolves every `env` reference to `value-of-<NAME>`; refuses `store`.
#[derive(Debug)]
pub(super) struct FixedSecrets;

impl Secrets for FixedSecrets {
    fn resolve(&self, reference: &SecretRef) -> Result<String> {
        match reference {
            SecretRef::Env(name) => Ok(format!("value-of-{name}")),
            SecretRef::Store(_) => Err(Error::UnresolvedSecret {
                reference: reference.clone(),
            }),
        }
    }
}
