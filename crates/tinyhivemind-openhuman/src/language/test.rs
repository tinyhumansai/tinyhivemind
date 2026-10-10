//! Portable settings reach adapter requests without losing memory metadata.
use super::*;
use tinyhivemind_lang::{MemoryIdentity, MemoryLifecycle, SeatMemory, ToolAccess};

pub(crate) fn binding() -> LoweredMemoryBinding {
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
fn maps_an_agent_namespace_and_refuses_unrepresentable_bindings() -> Result<()> {
    let mut portable = binding();
    let harness = memory_binding(&portable)?;
    assert_eq!(harness.agent_id(), "shared");
    assert_eq!(harness.root_namespace(), Some("team:hive"));
    portable.kind = MemoryKind::Pool;
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::MemoryBindingUnsupported)
    ));
    portable.kind = MemoryKind::Agent;
    portable.namespace = "team:hive/pool:shared".into();
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::MemoryBindingUnsupported)
    ));
    portable.identity.namespace = "root/agent:shared".into();
    portable.root = "root".into();
    portable.namespace = "root/agent:shared".into();
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::InvalidMemoryRoot { .. })
    ));
    portable.identity.namespace = "team:hive/agent:bad id".into();
    portable.root = "team:hive".into();
    portable.agent_id = "bad id".into();
    portable.namespace = "team:hive/agent:bad id".into();
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::InvalidMemoryAgentId { .. })
    ));
    Ok(())
}
#[test]
fn seat_request_keeps_all_factory_settings_and_memory_contract() -> Result<()> {
    let seat = LoweredSeat {
        id: "scout".into(),
        label: "Scout".into(),
        template: "research".into(),
        config: serde_json::json!({"temperature":0}),
        prompt: "Read carefully".into(),
        model: Some("model".into()),
        tools: ToolAccess::default(),
        mcp: vec!["repo".into()],
        skills: vec!["review".into()],
        memory: Some(binding()),
    };
    let ManagementRequest::CreateAgent {
        template,
        config,
        memory,
    } = management_request(&seat)
    else {
        return Err(Error::MemoryBindingUnsupported);
    };
    assert_eq!(template, seat.template);
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

#[test]
fn refuses_inconsistent_portable_identity_metadata() {
    let mut portable = binding();
    portable.identity.kind = MemoryKind::Pool;
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::MemoryBindingUnsupported)
    ));
    portable = binding();
    portable.identity.namespace = "team:other/agent:shared".into();
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::MemoryBindingUnsupported)
    ));
    portable = binding();
    portable.settings.identity = "other-memory".into();
    assert!(matches!(
        memory_binding(&portable),
        Err(Error::MemoryBindingUnsupported)
    ));
}
