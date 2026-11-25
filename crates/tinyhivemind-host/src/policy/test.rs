//! Request wire shapes stay explicit and round trip.
use super::{ManagementRequest, SendRequest};

#[test]
fn send_request_wire_shape_is_explicit() -> Result<(), serde_json::Error> {
    let request = SendRequest::Hive {
        hive_id: "h".into(),
        body: "b".into(),
        thread: Some(3),
        only_for: vec![],
    };
    let wire = serde_json::json!({"Hive":{"hive_id":"h","body":"b","thread":3,"only_for":[]}});
    assert_eq!(serde_json::to_value(&request)?, wire);
    assert_eq!(
        serde_json::from_value::<SendRequest>(wire)?,
        request
    );
    Ok(())
}
#[test]
fn management_request_wire_shape_keeps_host_template_and_identity_fields() {
    use serde_json::json;
    let cases = [
        (
            ManagementRequest::CreateAgent {
                template: "research".into(),
                coordinator_id: None,
                memory: None,
                config: json!({"memory_ref":"private-1"}),
            },
            json!({"CreateAgent":{"template":"research","config":{"memory_ref":"private-1"}}}),
        ),
        (
            ManagementRequest::JoinHive {
                hive_id: "h".into(),
                agent_id: "a".into(),
            },
            json!({"JoinHive":{"hive_id":"h","agent_id":"a"}}),
        ),
        (
            ManagementRequest::LeaveHive {
                hive_id: "h".into(),
                agent_id: "a".into(),
            },
            json!({"LeaveHive":{"hive_id":"h","agent_id":"a"}}),
        ),
        (
            ManagementRequest::CreateHive(tinyhivemind_hives::HiveInfo {
                hive_id: "h".into(),
                name: "Hive".into(),
                description: None,
                members: vec![],
            }),
            json!({"CreateHive":{"hive_id":"h","name":"Hive","description":null,"members":[]}}),
        ),
    ];
    for (request, wire) in cases {
        assert_eq!(serde_json::to_value(&request).ok(), Some(wire.clone()));
        assert_eq!(
            serde_json::from_value::<ManagementRequest>(wire).ok(),
            Some(request)
        );
    }
}
#[test]
fn create_agent_binding_wire_is_optional_and_preserves_the_contract()
-> Result<(), serde_json::Error> {
    let legacy = serde_json::json!({"CreateAgent":{"template":"research","config":{}}});
    let decoded: ManagementRequest = serde_json::from_value(legacy.clone())?;
    assert!(matches!(
        &decoded,
        ManagementRequest::CreateAgent { memory: None, .. }
    ));
    assert_eq!(serde_json::to_value(decoded)?, legacy);
    let portable = serde_json::json!({
        "agent_id":"shared", "root":"team:hive", "namespace":"team:hive/agent:shared", "kind":"agent",
        "identity":{"id":"memory","namespace":"team:hive/agent:shared","kind":"agent","lifecycle":"hive","read_only":true,"fork_parent":null},
        "settings":{"seat":"scout","identity":"memory","reads":[],"recall_at":["compaction"],"budget_chars":128,"remember":["note"]}
    });
    let wire =
        serde_json::json!({"CreateAgent":{"template":"research","config":{},"memory":portable}});
    let decoded: ManagementRequest = serde_json::from_value(wire.clone())?;
    assert_eq!(serde_json::to_value(decoded)?, wire);
    Ok(())
}
