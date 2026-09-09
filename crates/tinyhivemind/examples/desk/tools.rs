//! Rendering the room's tool surface, and running one call against it.
//!
//! [`tinyhivemind::speech`] states the four tools once, as data. This module
//! turns that data into the two shapes this host needs — a JSON Schema for the
//! MCP server, and an implementation of [`tinytools::Tool`] for a host that
//! runs an agent loop in its own process — and gives both the same
//! [`invoke`] underneath.
//!
//! Two renderers over one statement is the whole point. A seat sees the same
//! four tools, with the same descriptions and the same bounds, whichever way
//! the host reached it, and neither renderer restates anything.

use serde_json::Value;
use tinyhivemind::speech::{
    CallArguments, ParameterKind, ToolCall, ToolSpec, Utterance, interpret, tool_specs,
};
use tinytools::{Tool, ToolResult};

use crate::mcp::{self, Serving};

/// The prefix a seat sees on these tools, matching the MCP server's own name.
///
/// The MCP server is registered as `desk`, and the agent CLI presents its
/// `post` as `desk_post`. A host running the loop in-process has no server to
/// namespace it, so it applies the same prefix here — otherwise the house
/// rules a seat is briefed with would name tools it cannot find.
const PREFIX: &str = "desk_";

/// One tool spec as JSON Schema, for a caller that speaks JSON Schema.
pub(crate) fn parameters_schema(spec: &ToolSpec) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();
    for parameter in spec.parameters {
        let mut schema = match parameter.kind {
            ParameterKind::Text => serde_json::json!({ "type": "string" }),
            ParameterKind::TextList => {
                serde_json::json!({ "type": "array", "items": { "type": "string" } })
            }
            ParameterKind::Count { .. } => serde_json::json!({ "type": "integer" }),
        };
        if let Some(description) = parameter.description
            && let Some(object) = schema.as_object_mut()
        {
            object.insert("description".into(), description.into());
        }
        properties.insert(parameter.name.to_string(), schema);
        if parameter.required {
            required.push(parameter.name.into());
        }
    }
    serde_json::json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": Value::Array(required),
    })
}

/// Run one call against the room, whatever asked for it.
///
/// The `name` is bare — [`PREFIX`] is stripped by the caller that applied it —
/// and the arguments are the JSON object the seat produced. What comes back is
/// the sentence the seat reads: an acknowledgement, or the reason it was
/// refused.
///
/// # Errors
///
/// Returns the refusal to hand back to the seat: a malformed call, or a `dm`
/// naming somebody who cannot receive one.
pub(crate) fn invoke(
    name: &str,
    arguments: &Value,
    serving: &Serving,
) -> Result<String, String> {
    let to = string_list(arguments, "to");
    let interpreted = interpret(
        name,
        &CallArguments {
            message: arguments.get("message").and_then(Value::as_str),
            to: &to,
            limit: arguments.get("limit").and_then(Value::as_u64),
        },
    )
    // The refusal is the seat's to read, so it travels as the sentence the
    // library wrote for it rather than as a code this host invents.
    .map_err(|rejection| rejection.to_string())?;
    match interpreted {
        ToolCall::Read { limit } => Ok(mcp::recent(&serving.transcript, limit)),
        ToolCall::Speak(utterance) => {
            let refusal = mcp::price(&utterance, serving)?;
            let acknowledgement = match (&utterance, refusal) {
                (Utterance::Post { .. }, _) => "posted to the desk".to_string(),
                (Utterance::Close { .. }, _) => {
                    "posted to the desk; the desk will close after this turn".to_string()
                }
                (Utterance::Dm { to, .. }, None) => format!("sent to @{}", to.join(", @")),
                (Utterance::Dm { .. }, Some(reason)) => format!(
                    "your aside was refused ({reason}) and the message goes to the whole desk \
                     instead; say it as you would in the open, or post it and move on"
                ),
            };
            mcp::append(&serving.outbox, &utterance)?;
            Ok(acknowledgement)
        }
    }
}

/// The `to` argument as owned strings, before the library normalizes it.
fn string_list(arguments: &Value, key: &str) -> Vec<String> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One of the room's tools, as a host that runs its own agent loop sees it.
pub(crate) struct DeskTool {
    spec: &'static ToolSpec,
    name: String,
    serving: Serving,
}

impl DeskTool {
    /// Wrap one spec against the room it speaks to.
    pub(crate) fn new(spec: &'static ToolSpec, serving: &Serving) -> Self {
        Self {
            spec,
            name: format!("{PREFIX}{}", spec.name),
            serving: serving.clone(),
        }
    }
}

#[async_trait::async_trait]
impl Tool for DeskTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        self.spec.description
    }

    fn parameters_schema(&self) -> Value {
        parameters_schema(self.spec)
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        // A refusal is a `ToolResult::error`, not an `Err`: the tool ran, and
        // the reason is for the seat to read and act on. `Err` would mean the
        // room could not be reached at all.
        Ok(match invoke(self.spec.name, &args, &self.serving) {
            Ok(text) => ToolResult::success(text),
            Err(reason) => ToolResult::error(reason),
        })
    }
}

/// Every tool the room serves, for a host that runs the loop itself.
pub(crate) fn desk_tools(serving: &Serving) -> Vec<DeskTool> {
    tool_specs()
        .iter()
        .map(|spec| DeskTool::new(spec, serving))
        .collect()
}

#[cfg(test)]
mod test;
