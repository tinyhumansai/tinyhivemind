//! Wire defaults for `hive.json` sections a manifest may omit.
//!
//! Plain `#[serde(default)]` covers the lang-owned fields. The three cases
//! here need more than that: core policy blocks and core desks are core types
//! whose own wire form stays complete, and a seat's label defaults to another
//! field of the same seat. Each decodes leniently into the same complete
//! value, so serialization, and with it the round-trip unknown-field check in
//! `document/mod.rs`, is unchanged.
use super::{Limits, Policies, Seat, ToolAccess};
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;
use tinyhivemind_core::desk::{Desk, ResponderMode};

/// Decode `policies` as an overlay on [`Policies::default`].
///
/// Objects merge key by key, recursively; arrays, scalars and `null` replace
/// the default outright. Keys the default does not have are carried into the
/// merged value, so the core decoder or the unknown-field pass still rejects
/// them.
pub(super) fn policies<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Policies, D::Error> {
    let input = Value::deserialize(deserializer)?;
    let mut merged = serde_json::to_value(Policies::default()).map_err(D::Error::custom)?;
    overlay(&mut merged, input);
    serde_json::from_value(merged).map_err(D::Error::custom)
}

/// Merge `input` into `base`: objects key by key, anything else replaces.
fn overlay(base: &mut Value, input: Value) {
    match (base, input) {
        (Value::Object(base), Value::Object(input)) => {
            for (key, value) in input {
                match base.get_mut(&key) {
                    Some(slot) => overlay(slot, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, input) => *base = input,
    }
}

/// Decode the desk list, letting each desk omit its `description`.
pub(super) fn desks<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Desk>, D::Error> {
    Ok(Vec::<DeskWire>::deserialize(deserializer)?
        .into_iter()
        .map(|desk| Desk {
            id: desk.id,
            name: desk.name,
            description: desk.description,
            members: desk.members,
            responder_mode: desk.responder_mode,
        })
        .collect())
}

/// A core [`Desk`] whose `description` may be omitted.
///
/// Like [`Desk`] it ignores unknown keys; the round-trip pass reports them at
/// their JSON Pointer.
#[derive(Deserialize)]
struct DeskWire {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    members: Vec<String>,
    #[serde(default)]
    responder_mode: ResponderMode,
}

/// A [`Seat`] as written: everything but its identity, role, prompt and
/// template may be omitted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SeatWire {
    id: String,
    #[serde(default)]
    label: Option<String>,
    role: String,
    prompt: String,
    template: String,
    #[serde(default = "empty_object")]
    config: Value,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    tools: ToolAccess,
    #[serde(default)]
    mcp: Vec<String>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    toolpacks: Vec<String>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    permissions: Option<String>,
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    context: Vec<String>,
}

impl From<SeatWire> for Seat {
    fn from(wire: SeatWire) -> Self {
        Self {
            label: wire.label.unwrap_or_else(|| wire.id.clone()),
            id: wire.id,
            role: wire.role,
            prompt: wire.prompt,
            template: wire.template,
            config: wire.config,
            model: wire.model,
            tools: wire.tools,
            mcp: wire.mcp,
            skills: wire.skills,
            profile: wire.profile,
            agent_id: wire.agent_id,
            toolpacks: wire.toolpacks,
            hooks: wire.hooks,
            permissions: wire.permissions,
            limits: wire.limits,
            context: wire.context,
        }
    }
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}
