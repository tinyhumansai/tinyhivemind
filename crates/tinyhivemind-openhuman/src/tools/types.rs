//! The stable tool vocabulary and its argument declarations.
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    ListHives,
    ListAgents,
    Read,
    SendHive,
    SendAgent,
    Post,
    Ask,
    Broadcast,
    Complete,
    CreateHive,
    CreateAgent,
    JoinHive,
    LeaveHive,
}
impl Kind {
    pub(super) fn all(managed: bool) -> Vec<Self> {
        let mut kinds = vec![
            Self::ListHives,
            Self::ListAgents,
            Self::Read,
            Self::SendHive,
            Self::SendAgent,
            Self::Post,
            Self::Ask,
            Self::Broadcast,
            Self::Complete,
        ];
        if managed {
            kinds.extend([
                Self::CreateHive,
                Self::CreateAgent,
                Self::JoinHive,
                Self::LeaveHive,
            ]);
        }
        kinds
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::ListHives => "hivemind_list_hives",
            Self::ListAgents => "hivemind_list_agents",
            Self::Read => "hivemind_read",
            Self::SendHive => "hivemind_send_hive",
            Self::SendAgent => "hivemind_send_agent",
            Self::Post => "hivemind_post",
            Self::Ask => "hivemind_ask",
            Self::Broadcast => "hivemind_broadcast",
            Self::Complete => "hivemind_complete",
            Self::CreateHive => "hivemind_create_hive",
            Self::CreateAgent => "hivemind_create_agent",
            Self::JoinHive => "hivemind_join_hive",
            Self::LeaveHive => "hivemind_leave_hive",
        }
    }
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::ListHives => "List hives you have joined. Hive and desk have one hive_id.",
            Self::ListAgents => "List registered agent IDs for direct messages across hives.",
            Self::Read => {
                "Read a joined hive (hive_id, optional thread) or your direct transcript with a peer (agent_id), including replies. Supply exactly one destination. after is an exclusive sequence cursor. Reading never schedules a turn."
            }
            Self::SendHive => {
                "Enqueue a message to a joined hive; message_id deduplicates retries. Does not wait for a reply."
            }
            Self::SendAgent => {
                "Enqueue a direct message to a registered agent; message_id deduplicates retries. Does not wait for a reply."
            }
            Self::Post => "Post to your explicitly identified active episode.",
            Self::Ask => {
                "Ask peers in a child conversation of your explicitly identified active episode."
            }
            Self::Broadcast => {
                "Broadcast work within your explicitly identified active episode, subject to conductor bounds."
            }
            Self::Complete => "Complete your explicitly identified active episode assignment.",
            Self::CreateHive => "Create a hive after host authorization, using known agent IDs.",
            Self::CreateAgent => {
                "Ask the host to instantiate a configured agent on the shared runtime. Config must contain only nonsecret settings or references."
            }
            Self::JoinHive => "Join a registered agent to a hive after host authorization.",
            Self::LeaveHive => "Leave a hive after host authorization, retaining recorded history.",
        }
    }
    pub(super) fn fields(self) -> Vec<(&'static str, &'static str, bool)> {
        match self {
            Self::ListHives | Self::ListAgents => vec![],
            Self::Read => vec![
                ("hive_id", "string", false),
                ("agent_id", "string", false),
                ("after", "integer", false),
                ("thread", "integer", false),
            ],
            Self::SendHive => vec![
                ("hive_id", "string", true),
                ("message_id", "string", true),
                ("body", "string", true),
                ("thread", "integer", false),
                ("only_for", "array", false),
            ],
            Self::SendAgent => vec![
                ("agent_id", "string", true),
                ("message_id", "string", true),
                ("body", "string", true),
            ],
            Self::Post | Self::Broadcast | Self::Complete => {
                vec![("episode_id", "string", true), ("body", "string", true)]
            }
            Self::Ask => vec![
                ("episode_id", "string", true),
                ("agents", "array", true),
                ("body", "string", true),
            ],
            Self::CreateHive => vec![
                ("hive_id", "string", true),
                ("name", "string", true),
                ("description", "string", false),
                ("members", "array", false),
            ],
            Self::CreateAgent => vec![
                ("template", "string", true),
                ("config", "object", true),
                ("memory", "object", false),
            ],
            Self::JoinHive | Self::LeaveHive => {
                vec![("hive_id", "string", true), ("agent_id", "string", true)]
            }
        }
    }
    pub(super) fn schema(self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for (name, ty, needed) in self.fields() {
            let mut field = json!({"type":ty});
            if ty == "array" {
                field["items"] = json!({"type":"string"});
            }
            if ty == "integer" {
                field["minimum"] = json!(0);
            }
            properties.insert(name.into(), field);
            if needed {
                required.push(name);
            }
        }
        let mut schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
        if matches!(self, Self::Read) {
            schema["oneOf"] = json!([
                {"required":["hive_id"], "not":{"required":["agent_id"]}},
                {"required":["agent_id"], "not":{"anyOf":[{"required":["hive_id"]},{"required":["thread"]}]}}
            ]);
        }
        schema
    }
    pub(super) fn validate(self, args: &Value) -> anyhow::Result<()> {
        let obj = args
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("arguments must be an object"))?;
        let fields = self.fields();
        for name in obj.keys() {
            anyhow::ensure!(
                fields.iter().any(|(field, _, _)| field == name),
                "unknown argument {name}"
            );
        }
        for (name, ty, required) in fields {
            let Some(value) = obj.get(name) else {
                anyhow::ensure!(!required, "missing argument {name}");
                continue;
            };
            let valid = match ty {
                "string" => value.is_string(),
                "integer" => value.as_u64().is_some(),
                "object" => value.is_object(),
                "array" => value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string)),
                _ => false,
            };
            anyhow::ensure!(valid, "invalid {name}: expected {ty}");
        }
        if matches!(self, Self::Read) {
            anyhow::ensure!(
                obj.contains_key("hive_id") != obj.contains_key("agent_id"),
                "supply exactly one of hive_id or agent_id"
            );
            anyhow::ensure!(
                !obj.contains_key("agent_id") || !obj.contains_key("thread"),
                "thread is only valid for hive reads"
            );
        }
        Ok(())
    }
}
