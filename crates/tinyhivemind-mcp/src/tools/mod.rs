//! The tools one seat is served, and how a call of one is carried out.
//!
//! A [`Catalog`] holds the hive tools and the seat's executable tools that
//! its permissions allow, each with the schema the model sees. A tool that
//! changes state (every hive tool but the list and read tools, every program
//! not declared `side_effects = "read"`, and any tool a hook matches) gets
//! two required arguments in its schema, `turn` and `call`, which
//! [`dispatch`] checks and removes before hooks and dispatch.
mod dispatch;
mod gate;
mod ledger;
mod schema;

use crate::seat::{RESERVED, SeatTools};
use crate::{Error, Result};
pub(crate) use dispatch::{SeatState, call};
pub(crate) use ledger::Ledger;
pub(crate) use schema::unsupported;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tinyhivemind_exec::HookRunner;
use tinyhivemind_host::tools::Kind;
use tinyhivemind_lang::{ExecTool, SideEffects};

/// What a served tool runs.
#[derive(Clone, Debug)]
pub(crate) enum Target {
    /// A hive tool, through the host's belt.
    Hive(Kind),
    /// An executable tool of the package.
    Program(ExecTool),
}

/// One tool as a seat is served it.
#[derive(Clone, Debug)]
pub(crate) struct Served {
    pub(crate) name: String,
    description: String,
    schema: Value,
    pub(crate) target: Target,
    /// The call must name its turn and carry a `call`.
    pub(crate) changes_state: bool,
    /// A `pre_tool_use` or `post_tool_use` hook matches it.
    pub(crate) hooked: bool,
}

/// Every tool one seat is served, in a fixed order.
#[derive(Clone, Debug)]
pub(crate) struct Catalog(Vec<Served>);

impl Catalog {
    /// The hive tools (management ones when `managed`), then the seat's
    /// programs, less what its permissions deny.
    pub(crate) fn build(seat: &SeatTools, hooks: &HookRunner, managed: bool) -> Result<Self> {
        let mut served = Vec::new();
        let mut names = BTreeSet::new();
        let hive = Kind::all(managed).into_iter().map(|kind| {
            let reads = matches!(kind, Kind::ListHives | Kind::ListAgents | Kind::Read);
            let (name, schema) = (kind.name().to_owned(), kind.schema());
            (
                name,
                kind.description().to_owned(),
                schema,
                Target::Hive(kind),
                !reads,
            )
        });
        let programs = seat.programs().iter().map(|tool| {
            let writes = tool.side_effects != SideEffects::Read;
            let target = Target::Program(tool.clone());
            (
                tool.id.clone(),
                tool.description.clone(),
                tool.args.clone(),
                target,
                writes,
            )
        });
        for (name, description, schema, target, effectful) in hive.chain(programs) {
            if !names.insert(name.clone()) {
                return Err(Error::DuplicateTool(name));
            }
            if !seat.permits(&name) {
                continue;
            }
            let hooked = hooks.matches_tool(&name);
            let changes_state = effectful || hooked;
            let schema = if changes_state {
                identified(schema)
            } else {
                schema
            };
            served.push(Served {
                name,
                description,
                schema,
                target,
                changes_state,
                hooked,
            });
        }
        Ok(Self(served))
    }

    /// The tool the model calls `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&Served> {
        self.0.iter().find(|tool| tool.name == name)
    }

    /// Every served name.
    pub(crate) fn names(&self) -> Vec<String> {
        self.0.iter().map(|tool| tool.name.clone()).collect()
    }

    /// The `tools/list` result's `tools`.
    pub(crate) fn listing(&self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.schema,
                    })
                })
                .collect(),
        )
    }
}

/// `schema` with the required `turn` and `call` arguments added.
fn identified(mut schema: Value) -> Value {
    if !schema.is_object() {
        schema = json!({"type": "object"});
    }
    let fields = [
        (
            RESERVED[0],
            "Your running turn id, exactly as your turn gave it. A call from any other turn is refused.",
        ),
        (
            RESERVED[1],
            "A name you choose for this one action, unique within the turn. Repeat it only to retry the same call with the same arguments.",
        ),
    ];
    if !schema["properties"].is_object() {
        schema["properties"] = json!({});
    }
    if !schema["required"].is_array() {
        schema["required"] = json!([]);
    }
    for (name, description) in fields {
        schema["properties"][name] = json!({"type": "string", "description": description});
        if let Some(required) = schema["required"].as_array_mut() {
            required.push(json!(name));
        }
    }
    schema
}

#[cfg(test)]
mod test;
