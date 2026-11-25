//! Portable settings reach host requests without losing memory metadata.
use super::*;
use tinyhivemind_lang::{
    LoweredMemoryBinding, MemoryIdentity, MemoryKind, MemoryLifecycle, SeatMemory, ToolAccess,
};

fn binding() -> LoweredMemoryBinding {
    LoweredMemoryBinding {
        agent_id: "shared".into(),
        root: "team:hive".into(),
        namespace: "team:hive/agent:shared".into(),
        kind: MemoryKind::Agent,
        identity: MemoryIdentity {
            id: "shared-memory".into(),
            namespace: "team:hive/agent:shared".into(),
            kind: MemoryKind::Agent,
            lifecycle: MemoryLifecycle::Hive,
            read_only: false,
            fork_parent: None,
        },
        settings: SeatMemory {
            seat: "scout".into(),
            identity: "shared-memory".into(),
            reads: vec![],
            recall_at: vec![],
            budget_chars: 128,
            remember: vec![],
        },
    }
}
#[test]
fn seat_request_keeps_all_factory_settings_and_memory_contract() -> anyhow::Result<()> {
    let seat = LoweredSeat {
        id: "scout".into(),
        label: "Scout".into(),
        template: "research".into(),
        config: serde_json::json!({"temperature":0}),
        prompt: "Read carefully".into(),
        memory: Some(binding()),
        integration: tinyhivemind_lang::LoweredSeatIntegration {
            model: Some(tinyhivemind_lang::LoweredModel {
                model: tinyhivemind_lang::Model {
                    id: "default".into(),
                    name: "model".into(),
                    ..Default::default()
                },
                provider: None,
            }),
            tools: ToolAccess::default(),
            mcp: vec![tinyhivemind_lang::McpServer {
                id: "repo".into(),
                endpoint: "repo-mcp".into(),
                ..Default::default()
            }],
            skills: vec![tinyhivemind_lang::Skill {
                id: "review".into(),
            }],
            ..Default::default()
        },
    };
    let ManagementRequest::CreateAgent {
        template,
        coordinator_id,
        config,
        memory,
    } = management_request(&seat)
    else {
        anyhow::bail!("a seat request is a creation request");
    };
    assert_eq!(template, seat.template);
    assert_eq!(coordinator_id.as_deref(), Some(seat.id.as_str()));
    assert_eq!(config["id"], seat.id);
    assert_eq!(config["prompt"], seat.prompt);
    assert_eq!(config["model"], "model");
    assert_eq!(config["mcp"], serde_json::json!(["repo"]));
    assert_eq!(config["skills"], serde_json::json!(["review"]));
    assert_eq!(config["config"], seat.config);
    assert_eq!(memory.as_deref(), seat.memory.as_ref());
    Ok(())
}
#[test]
fn coordinator_settings_map_all_retention_bounds() {
    let settings = CoordinatorSettings {
        round_width: 3,
        broadcast_budget: Some(8),
        retention: tinyhivemind_lang::RetentionSettings {
            settled_episodes: Some(2),
            delivered: Some(4),
            interrupted: Some(6),
            pending_per_agent: Some(9),
        },
        ..CoordinatorSettings::default()
    };
    let options = coordinator_options(&settings);
    assert_eq!(options.round_width, 3);
    assert_eq!(options.broadcast_budget, Some(8));
    assert_eq!(options.retention.settled_episodes, Some(2));
    assert_eq!(options.retention.delivered, Some(4));
    assert_eq!(options.retention.interrupted, Some(6));
    assert_eq!(options.retention.pending_per_agent, Some(9));
}
