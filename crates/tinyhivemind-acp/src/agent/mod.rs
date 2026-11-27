//! What differs between ACP agents, as data.
//!
//! An [`Agent`] says how to start one agent process for a seat and how to
//! open its session. Strings in [`Agent::args`], [`Agent::env`],
//! [`Agent::files`] and [`Agent::session_meta`] are templates; each `{name}`
//! below is replaced when a seat starts:
//!
//! | Name | Value |
//! | --- | --- |
//! | `{state}` | the instance's private state directory |
//! | `{home}` | `{state}/home`, the process's `HOME` |
//! | `{endpoint}` | the seat's model endpoint, as the package declares it |
//! | `{model}` | the seat's model name |
//! | `{server}` | the host's MCP server name |
//! | `{program_dir}` | the directory of [`Agent::program`] |
//! | `{seat}` | the seat's id |
//! | any key of [`Agent::vars`] | its value |
//!
//! In `session_meta`, a string that is exactly `{allowed_tools}` becomes the
//! array of the host's tools under the names [`Agent::tool_name`] gives them.
//!
//! A template is read once, left to right: a value is never itself expanded,
//! so a path holding `{model}` stays that path. In a file whose name ends in
//! `.toml` each value is escaped for a TOML basic string, which the template
//! puts it in.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use tinyhivemind_lang::HarnessKind;

/// One ACP agent, as a seat runs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    /// The kind of seat it runs, as the package names it.
    pub kind: HarnessKind,
    /// The program, an absolute path: the agent, or the runtime of a script.
    pub program: PathBuf,
    /// Its arguments, templated.
    pub args: Vec<String>,
    /// The version `initialize` must report as `agentInfo.version`.
    pub version: String,
    /// The environment, templated. Nothing else is inherited.
    pub env: Vec<(String, String)>,
    /// Files written into the state directory before each start: a path
    /// relative to it, and templated content (TOML-escaped for a `.toml`
    /// path).
    pub files: Vec<(PathBuf, String)>,
    /// The `_meta` of `session/new` and `session/load`, templated.
    pub session_meta: Option<Value>,
    /// The session mode set after `session/new` and `session/load`.
    pub mode: Option<String>,
    /// Whether `session/new` and `session/load` pass the host's tool server.
    /// An agent that refuses servers per session, such as `OpenClaw`, is
    /// configured with it elsewhere and opens its sessions with none.
    pub session_mcp: bool,
    /// The name the agent's model sees for one of the host's tools, from
    /// `{server}`, `{seat}` and `{tool}`.
    pub tool_name: String,
    /// Further template values, such as a file the host generated.
    pub vars: BTreeMap<String, String>,
}

/// The key every key-free provider is given, so an agent that insists on one
/// finds it and a stray request is plainly not a real one.
pub const NO_KEY: &str = "thm-no-key";

/// The environment every agent gets: a private home and a minimal path.
pub(crate) fn base_env() -> Vec<(String, String)> {
    vec![
        ("HOME".into(), "{home}".into()),
        ("PATH".into(), "{program_dir}:/usr/bin:/bin".into()),
    ]
}

impl Agent {
    /// Claude Code through `@agentclientprotocol/claude-agent-acp`, run by
    /// `node` from `adapter`, its `dist/index.js`.
    ///
    /// Its model is the seat's `anthropic` endpoint, the API root without
    /// `/v1`. Built-in tools, settings files and deferred tool search are off,
    /// and the host's tools are allowed by name, so the model sees those
    /// tools and nothing else and is never asked to approve them.
    #[must_use]
    pub fn claude_code(
        node: impl Into<PathBuf>,
        adapter: impl Into<PathBuf>,
        version: impl Into<String>,
    ) -> Self {
        let mut env = base_env();
        env.extend(
            [
                ("CLAUDE_CONFIG_DIR", "{state}/claude"),
                ("ANTHROPIC_BASE_URL", "{endpoint}"),
                ("ANTHROPIC_API_KEY", NO_KEY),
                ("ANTHROPIC_MODEL", "{model}"),
                ("ANTHROPIC_SMALL_FAST_MODEL", "{model}"),
                ("ENABLE_TOOL_SEARCH", "false"),
                ("DISABLE_TELEMETRY", "1"),
                ("DISABLE_ERROR_REPORTING", "1"),
                ("DISABLE_AUTOUPDATER", "1"),
                ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ]
            .map(|(name, value)| (name.to_owned(), value.to_owned())),
        );
        let meta = serde_json::json!({"claudeCode": {"options": {
            "tools": [],
            "settingSources": [],
            "allowedTools": "{allowed_tools}",
        }}});
        Self {
            kind: HarnessKind::ClaudeCode,
            program: node.into(),
            args: vec![adapter.into().to_string_lossy().into_owned()],
            version: version.into(),
            env,
            files: Vec::new(),
            session_meta: Some(meta),
            mode: None,
            session_mcp: true,
            tool_name: "mcp__{server}__{tool}".into(),
            vars: BTreeMap::new(),
        }
    }

    /// Codex through `@agentclientprotocol/codex-acp`, run by `node` from
    /// `adapter`, its `dist/index.js`, with the model catalog at `catalog`
    /// (see [`write_codex_catalog`]).
    ///
    /// Its model is the seat's `openai` endpoint, spoken to with the
    /// Responses API. The session runs in the adapter's `read-only` mode: the
    /// default mode has a model review every tool call, which would run
    /// unseen and could refuse the host's tools.
    #[must_use]
    pub fn codex(
        node: impl Into<PathBuf>,
        adapter: impl Into<PathBuf>,
        version: impl Into<String>,
        catalog: impl AsRef<Path>,
    ) -> Self {
        let mut env = base_env();
        env.extend(
            [("CODEX_HOME", "{state}/codex"), ("THM_NO_KEY", NO_KEY)]
                .map(|(name, value)| (name.to_owned(), value.to_owned())),
        );
        let catalog = catalog.as_ref().to_string_lossy().into_owned();
        Self {
            kind: HarnessKind::Codex,
            program: node.into(),
            args: vec![adapter.into().to_string_lossy().into_owned()],
            version: version.into(),
            env,
            files: vec![("codex/config.toml".into(), CODEX_CONFIG.into())],
            session_meta: None,
            mode: Some("read-only".into()),
            session_mcp: true,
            tool_name: "mcp__{server}/{tool}".into(),
            vars: BTreeMap::from([("catalog".to_owned(), catalog)]),
        }
    }
}

/// Codex's `config.toml`: the seat's endpoint as its only provider, and
/// everything that would run, fetch or remember on its own turned off. As
/// the 2026-11-25 spike's, with the provider renamed.
const CODEX_CONFIG: &str = r#"model_catalog_json = "{catalog}"
model = "{model}"
model_provider = "thm"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"
check_for_update_on_startup = false
project_doc_max_bytes = 0
cli_auth_credentials_store = "file"
include_permissions_instructions = false
include_apps_instructions = false
include_environment_context = false
[model_providers.thm]
name = "thm"
base_url = "{endpoint}"
env_key = "THM_NO_KEY"
wire_api = "responses"
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
[analytics]
enabled = false
[feedback]
enabled = false
[otel]
exporter = "none"
trace_exporter = "none"
metrics_exporter = "none"
[history]
persistence = "none"
[skills]
include_instructions = false
[skills.bundled]
enabled = false
[agents]
enabled = false
[features]
view_image = false
multi_agent_v2 = false
shell_tool = false
unified_exec = false
multi_agent = false
goals = false
apps = false
plugins = false
remote_plugin = false
plugin_sharing = false
tool_suggest = false
skill_search = false
skill_mcp_dependency_install = false
image_generation = false
browser_use = false
browser_use_external = false
computer_use = false
in_app_browser = false
sleep_tool = false
code_mode_host = false
hooks = false
shell_snapshot = false
workspace_dependencies = false
daemon_auto_start = false
memories = false
fast_mode = false
api_key_model_discovery = false
[tools]
[tools.experimental_request_user_input]
enabled = false
"#;

/// Why [`write_codex_catalog`] failed.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// `codex` could not be started.
    #[error("codex could not be run: {0}")]
    Launch(#[source] std::io::Error),
    /// `codex debug models` failed.
    #[error("codex debug models failed ({status}): {stderr}")]
    Exit {
        /// How it exited.
        status: std::process::ExitStatus,
        /// The end of what it wrote to stderr.
        stderr: String,
    },
    /// Its output was not JSON.
    #[error("codex's catalog is not JSON: {0}")]
    Json(#[source] serde_json::Error),
    /// Its catalog has no `models` array.
    #[error("codex's catalog has no models")]
    NoModels,
    /// `out` could not be written.
    #[error("the catalog could not be written: {0}")]
    Write(#[source] std::io::Error),
}

/// Write the model catalog Codex needs to `out`, from `codex`'s own bundled
/// one: every model calls tools directly, with no shell, patch tool, tool
/// search or sub-agents, and `model` is added with the first model's
/// metadata if the catalog lacks it. Without this, models hid MCP tools or
/// routed them through code mode (`docs/experiments/2026-11-25-process-harnesses`).
///
/// # Errors
/// A [`CatalogError`]: `codex` could not be run or failed, its catalog was
/// not JSON or had no models, or `out` could not be written.
pub fn write_codex_catalog(codex: &Path, out: &Path, model: &str) -> Result<(), CatalogError> {
    let home = out.parent().unwrap_or(Path::new("."));
    let output = std::process::Command::new(codex)
        .args(["debug", "models", "--bundled"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("CODEX_HOME", home)
        .output()
        .map_err(CatalogError::Launch)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .char_indices()
            .rev()
            .nth(4095)
            .map_or(0, |(at, _)| at);
        return Err(CatalogError::Exit {
            status: output.status,
            stderr: stderr[tail..].trim().to_owned(),
        });
    }
    let mut catalog: Value = serde_json::from_slice(&output.stdout).map_err(CatalogError::Json)?;
    let models = catalog["models"]
        .as_array_mut()
        .ok_or(CatalogError::NoModels)?;
    for entry in models.iter_mut() {
        let Some(entry) = entry.as_object_mut() else {
            continue;
        };
        entry.insert("tool_mode".into(), "direct".into());
        entry.insert("shell_type".into(), "disabled".into());
        entry.insert("apply_patch_tool_type".into(), Value::Null);
        entry.insert("supports_search_tool".into(), false.into());
        for (key, value) in [
            ("multi_agent_version", Value::Null),
            ("use_responses_lite", false.into()),
            ("supports_reasoning_effort_updates", false.into()),
            ("experimental_supported_tools", Value::Array(Vec::new())),
        ] {
            if entry.contains_key(key) {
                entry.insert(key.into(), value);
            }
        }
    }
    if !models.iter().any(|entry| entry["slug"] == model)
        && let Some(first) = models.first().cloned()
    {
        let mut added = first;
        added["slug"] = model.into();
        added["display_name"] = model.into();
        models.insert(0, added);
    }
    let bytes = serde_json::to_vec(&catalog).map_err(CatalogError::Json)?;
    std::fs::write(out, bytes).map_err(CatalogError::Write)
}

/// The values one seat's templates are filled from.
#[derive(Clone, Debug, Default)]
pub(crate) struct Values {
    pub(crate) values: BTreeMap<String, String>,
    pub(crate) allowed_tools: Vec<String>,
}

impl Values {
    /// Fill `template`'s `{name}`s.
    pub(crate) fn fill(&self, template: &str) -> String {
        self.fill_with(template, str::to_owned)
    }

    /// Fill `template`'s `{name}`s with values escaped for a TOML basic
    /// string.
    pub(crate) fn fill_toml(&self, template: &str) -> String {
        self.fill_with(template, toml_escape)
    }

    /// Fill the file at `path`: TOML-escaped when it is a `.toml` file.
    pub(crate) fn fill_file(&self, path: &Path, template: &str) -> String {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            self.fill_toml(template)
        } else {
            self.fill(template)
        }
    }

    /// One pass over `template`: each known `{name}` becomes its value as
    /// `escape` writes it, and nothing a value brings in is read again.
    fn fill_with(&self, template: &str, escape: impl Fn(&str) -> String) -> String {
        let mut filled = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            filled.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let known = after.find('}').and_then(|close| {
                let value = self.values.get(&after[..close])?;
                Some((close, value))
            });
            if let Some((close, value)) = known {
                filled.push_str(&escape(value));
                rest = &after[close + 1..];
            } else {
                filled.push('{');
                rest = after;
            }
        }
        filled.push_str(rest);
        filled
    }

    /// Fill every string of `template`, turning an exact `{allowed_tools}`
    /// into the allowed tools.
    pub(crate) fn fill_json(&self, template: &Value) -> Value {
        match template {
            Value::String(text) if text == "{allowed_tools}" => {
                Value::from(self.allowed_tools.clone())
            }
            Value::String(text) => Value::String(self.fill(text)),
            Value::Array(items) => items.iter().map(|item| self.fill_json(item)).collect(),
            Value::Object(fields) => fields
                .iter()
                .map(|(key, value)| (key.clone(), self.fill_json(value)))
                .collect(),
            other => other.clone(),
        }
    }
}

/// `value` as the inside of a TOML basic string: quotes, backslashes and
/// control characters escaped.
fn toml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            control if control.is_control() => {
                let _ = write!(escaped, "\\u{:04X}", u32::from(control));
            }
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod test;
