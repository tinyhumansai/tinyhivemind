//! Portable memory bindings map onto the harness namespace or are refused.
use super::*;
use tinyhivemind_lang::{MemoryIdentity, MemoryLifecycle, SeatMemory};

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
