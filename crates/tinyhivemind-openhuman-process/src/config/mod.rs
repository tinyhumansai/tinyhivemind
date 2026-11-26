//! The `config.toml` a seat's instance runs with, and how its effective
//! config is compared with it.
//!
//! The config is built as one JSON tree from the experiment's
//! `docs/experiments/2026-11-25-process-harnesses/openhuman/config.toml.in`,
//! with the seat's provider, prompt and tools put in, and written as TOML.
//! The same tree is what [`Written::settings`] compares: every leaf that was
//! written, by its dotted path, plus the routes that must stay unset. The
//! binary silently falls back to its defaults on a config it cannot read, so
//! nothing else shows that it runs with this one.
//!
//! What is fixed, and why:
//!
//! - one cloud provider, `auth_style = "none"`, named by `primary_cloud` and by
//!   every workload route, so the hosted provider is never consulted;
//! - one MCP server, the host's, with `expose = "direct"` and the seat's
//!   bearer, so its tools are offered by name;
//! - one custom agent with an exact `tool_allowlist` and no denylist, because
//!   `"*"` with a denylist let `mcp_call_tool` post as another seat;
//! - `auto_approve` exactly the hive tools, whose policy is the host's;
//! - memory, recall, conversation memory, search, hooks, suggestions and
//!   token compression off, and tool-output compaction too: while it is on,
//!   0.64.15 adds `juice_retrieve` to every allowlist.
//!
//! The top-level `embeddings_provider` is not written: 0.64.15 rewrites any
//! value to its hosted default on load, and nothing embeds with memory off.
mod toml;

use crate::capability::SeatModel;
use serde_json::{Map, Value, json};

/// The custom agent every seat's instance runs as.
pub(crate) const AGENT_ID: &str = "thm_seat";
/// The provider's id and slug in the generated config.
const PROVIDER: &str = "thm";
/// Effective settings that must stay unset: any of them is another route.
const UNSET: [&str; 3] = ["api_key", "api_url", "inference_url"];

/// What a seat's config is made of.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Input<'a> {
    /// The provider route.
    pub(crate) model: &'a SeatModel,
    /// The seat's id, which names its agent.
    pub(crate) seat: &'a str,
    /// The seat's system prompt.
    pub(crate) prompt: &'a str,
    /// The host's MCP server name.
    pub(crate) server: &'a str,
    /// Its URL.
    pub(crate) url: &'a str,
    /// The seat's bearer for it.
    pub(crate) bearer: &'a str,
    /// The model-visible names of the hive tools, which the allowlist and
    /// `auto_approve` hold exactly.
    pub(crate) tools: &'a [String],
    /// How long the binary itself lets a turn run, in seconds.
    pub(crate) turn_secs: u64,
}

/// A config as written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Written {
    tree: Value,
}

impl Written {
    /// The config for `input`.
    pub(crate) fn new(input: &Input<'_>) -> Self {
        let model = input.model;
        let route = format!("{PROVIDER}:{}", model.model);
        let tree = json!({
            "schema_version": 13,
            "chat_provider": route,
            "reasoning_provider": route,
            "agentic_provider": route,
            "coding_provider": route,
            "primary_cloud": PROVIDER,
            "default_model": model.model,
            "memory": {
                "engine": "",
                "embedding_provider": "none",
                "recall": {"enabled": false},
                "conversations": {"enabled": false},
            },
            "search": {"enabled": false},
            "observability": {"analytics_enabled": false, "share_usage_data": false},
            "mcp_client": {
                "enabled": true,
                "servers": [{
                    "name": input.server,
                    "endpoint": input.url,
                    "expose": "direct",
                    "enabled": true,
                    "auth": {"kind": "bearer_token", "token": input.bearer},
                }],
            },
            "agent": {
                "chat_agent_id": AGENT_ID,
                "tool_dispatcher": "native",
                "agent_timeout_secs": input.turn_secs,
            },
            "autonomy": {"auto_approve": input.tools, "auto_approve_all": false},
            "tokenjuice": {
                "router_enabled": false,
                "ccr_enabled": false,
                "repl_handle_enabled": false,
            },
            "context": {"compaction_enabled": false},
            "hooks": {"enabled": false},
            "web_chat": {"suggestions_enabled": false},
            "agent_registry": {"entries": [{
                "id": AGENT_ID,
                "name": input.seat,
                "description": "hive seat",
                "source": "custom",
                "enabled": true,
                "system_prompt": input.prompt,
                "tool_allowlist": input.tools,
            }]},
            "cloud_providers": [{
                "id": PROVIDER,
                "slug": PROVIDER,
                "label": "tinyhivemind",
                "endpoint": model.endpoint,
                "auth_style": "none",
            }],
        });
        Self { tree }
    }

    /// The file's text.
    pub(crate) fn toml(&self) -> String {
        toml::write(&self.tree)
    }

    /// Every setting this config fixes, by dotted path, as written.
    pub(crate) fn settings(&self) -> Vec<(String, Value)> {
        let mut settings = Vec::new();
        leaves("", &self.tree, &mut settings);
        settings.extend(UNSET.map(|name| (name.to_owned(), Value::Null)));
        settings
    }

    /// The same settings as `effective`, a `config_get` snapshot's `config`,
    /// has them. A list of tables is cut down to the keys that were written
    /// in each of its entries; an entry beyond them is kept whole, so that it
    /// differs.
    pub(crate) fn observe(&self, effective: &Value) -> Vec<(String, Value)> {
        let settings = self.settings();
        let observed = settings.into_iter().map(|(path, written)| {
            let found = lookup(effective, &path).cloned().unwrap_or(Value::Null);
            let found = project(&written, found);
            (path, found)
        });
        observed.collect()
    }
}

/// Collect the leaves of `value` under `prefix`. A list is one leaf.
fn leaves(prefix: &str, value: &Value, into: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaves(&path, value, into);
            }
        }
        leaf => into.push((prefix.to_owned(), leaf.clone())),
    }
}

fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |value, key| value.get(key))
}

/// `found` cut down to the shape of `written`.
fn project(written: &Value, found: Value) -> Value {
    match (written, found) {
        (Value::Array(written), Value::Array(found)) => {
            let entries =
                found
                    .into_iter()
                    .enumerate()
                    .map(|(index, entry)| match written.get(index) {
                        Some(shape) => project(shape, entry),
                        None => entry,
                    });
            Value::Array(entries.collect())
        }
        (Value::Object(written), Value::Object(mut found)) => {
            let kept: Map<String, Value> = written
                .iter()
                .filter_map(|(key, shape)| {
                    let value = found.remove(key)?;
                    Some((key.clone(), project(shape, value)))
                })
                .collect();
            Value::Object(kept)
        }
        (_, found) => found,
    }
}

#[cfg(test)]
mod test;
