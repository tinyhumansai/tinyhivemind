//! The declaration of an executable tool: a program plus an argument schema.
//!
//! The calling convention is in `docs/specs/executable-tools.md`; this crate
//! only declares and checks it. `tinyhivemind-exec` runs it.
use super::SecretRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// What a tool's stdin receives.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStdin {
    /// The call's argument object, as JSON.
    #[default]
    Json,
    /// Nothing; stdin is closed.
    None,
}

/// How a tool's stdout becomes its result.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutput {
    /// Stdout verbatim.
    #[default]
    Text,
    /// Stdout parsed as JSON; unparsable output fails the call.
    Json,
}

/// What a call may change, which the permission gate classifies it by.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffects {
    /// Reads only.
    Read,
    /// Changes local state.
    #[default]
    Write,
    /// Reaches the network.
    Network,
}

/// The directory a tool runs in.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolDirectory {
    /// The seat's workspace.
    #[default]
    Workspace,
    /// The package root.
    Package,
}

/// An executable tool declared in `hive.json`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecTool {
    /// The name the model calls; `[a-z0-9_]+`, unique in the package.
    pub id: String,
    /// What the model is told the tool does.
    pub description: String,
    /// Program and fixed leading arguments. A program starting with `tools/`
    /// is a package file; any other is found on `PATH` by the host.
    pub command: Vec<String>,
    /// JSON Schema of the argument object; a plain object schema.
    #[serde(default = "object_schema")]
    pub args: Value,
    /// Extra arguments: literals, `{name}` placeholders, and `{name?}`
    /// placeholders dropped when the argument is absent.
    #[serde(default)]
    pub argv: Vec<String>,
    /// What stdin receives.
    #[serde(default)]
    pub stdin: ToolStdin,
    /// How stdout becomes the result.
    #[serde(default)]
    pub output: ToolOutput,
    /// Wall clock in milliseconds; nonzero.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Extra environment, every value a reference.
    #[serde(default)]
    pub env: BTreeMap<String, SecretRef>,
    /// What a call may change.
    #[serde(default)]
    pub side_effects: SideEffects,
    /// The directory the tool runs in.
    #[serde(default)]
    pub cwd: ToolDirectory,
}

impl ExecTool {
    /// The package file the program names, when it is one.
    #[must_use]
    pub fn package_program(&self) -> Option<&str> {
        self.command
            .first()
            .map(String::as_str)
            .filter(|program| program.starts_with("tools/"))
    }

    /// Every argument name the `argv` template refers to, with whether the
    /// placeholder is optional (`{name?}`).
    #[must_use]
    pub fn placeholders(&self) -> Vec<(&str, bool)> {
        self.argv
            .iter()
            .filter_map(|part| placeholder(part))
            .collect()
    }
}

/// The argument a whole-element `{name}` or `{name?}` template names.
#[must_use]
pub fn placeholder(part: &str) -> Option<(&str, bool)> {
    let inner = part.strip_prefix('{')?.strip_suffix('}')?;
    Some(match inner.strip_suffix('?') {
        Some(name) => (name, true),
        None => (inner, false),
    })
}

fn object_schema() -> Value {
    serde_json::json!({"type": "object"})
}

fn default_timeout() -> u64 {
    60_000
}
