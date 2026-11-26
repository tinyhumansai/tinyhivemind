//! A lowered seat becomes the tools it is served, or is refused.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use serde_json::json;
use support::{fixtures, tool};
use tinyhivemind_exec::ExecContext;
use tinyhivemind_lang::{
    Access, LoweredSeat, LoweredSeatIntegration, PermissionSet, RuntimeSettings, Sandbox,
};
use tinyhivemind_mcp::{Error, SeatTools};

fn seat(integration: LoweredSeatIntegration) -> LoweredSeat {
    LoweredSeat {
        id: "a".into(),
        label: "A".into(),
        template: "default".into(),
        config: json!({}),
        prompt: String::new(),
        memory: None,
        integration,
    }
}
fn context() -> ExecContext {
    ExecContext {
        package_root: fixtures(),
        ..ExecContext::default()
    }
}

#[test]
fn a_lowered_seat_takes_its_tools_hooks_and_the_strictest_access() {
    let integration = LoweredSeatIntegration {
        programs: vec![tool("stamp", "stamp", "write", json!({"type": "object"}))],
        permissions: vec![PermissionSet {
            id: "p".into(),
            access: Access::ReadOnly,
            ..PermissionSet::default()
        }],
        ..LoweredSeatIntegration::default()
    };
    let runtime = RuntimeSettings {
        autonomy: Access::Autonomous,
        ..RuntimeSettings::default()
    };
    let tools = SeatTools::from_lowered(&seat(integration), &runtime, context()).unwrap();
    assert_eq!(tools.seat(), "a");
    assert_eq!(tools.access(), Access::ReadOnly);
    assert_eq!(tools.program_names(), ["stamp"]);
}

#[test]
fn a_sandbox_other_than_none_is_refused() {
    for sandbox in [Sandbox::ReadOnly, Sandbox::Sandboxed] {
        let runtime = RuntimeSettings {
            sandbox,
            ..RuntimeSettings::default()
        };
        let refused = SeatTools::from_lowered(
            &seat(LoweredSeatIntegration::default()),
            &runtime,
            context(),
        );
        assert!(matches!(refused, Err(Error::UnsupportedSandbox(ref s)) if s == "a"));
        let integration = LoweredSeatIntegration {
            permissions: vec![PermissionSet {
                id: "p".into(),
                sandbox,
                ..PermissionSet::default()
            }],
            ..LoweredSeatIntegration::default()
        };
        let refused =
            SeatTools::from_lowered(&seat(integration), &RuntimeSettings::default(), context());
        assert!(matches!(refused, Err(Error::UnsupportedSandbox(_))));
    }
}

#[test]
fn package_mcp_servers_are_refused_until_they_are_proxied() {
    let integration = LoweredSeatIntegration {
        mcp: vec![
            serde_json::from_value(json!({"id": "files", "endpoint": "files-server"})).unwrap(),
        ],
        ..LoweredSeatIntegration::default()
    };
    let refused =
        SeatTools::from_lowered(&seat(integration), &RuntimeSettings::default(), context());
    assert!(matches!(refused, Err(Error::UnsupportedMcpServers(ref s)) if s == "a"));
}

#[test]
fn a_tool_declaring_turn_or_call_is_refused() {
    for name in ["turn", "call"] {
        let args = json!({"type": "object", "properties": {name: {"type": "string"}}});
        let integration = LoweredSeatIntegration {
            programs: vec![tool("stamp", "stamp", "write", args)],
            ..LoweredSeatIntegration::default()
        };
        let refused =
            SeatTools::from_lowered(&seat(integration), &RuntimeSettings::default(), context());
        assert!(
            matches!(refused, Err(Error::ReservedArgument { ref tool, name: ref n }) if tool == "stamp" && n == name)
        );
    }
}

#[tokio::test]
async fn a_tool_a_permission_denies_or_shadowing_a_hive_tool_is_not_served() {
    use std::sync::Arc;
    use tinyhivemind_hives::{Coordinator, CoordinatorOptions, MemoryStorage};
    use tinyhivemind_mcp::{HiveToolServer, ServerOptions, Services};
    let coordinator = Coordinator::new(
        "r".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap();
    let server = HiveToolServer::start(Services::new(coordinator), ServerOptions::default())
        .await
        .unwrap();
    let denied = SeatTools::new("a")
        .with_programs(vec![tool(
            "stamp",
            "stamp",
            "write",
            json!({"type": "object"}),
        )])
        .with_permissions(vec![PermissionSet {
            id: "p".into(),
            deny_tools: vec!["stamp".into(), "hivemind_broadcast".into()],
            ..PermissionSet::default()
        }]);
    let endpoint = server.serve_seat(denied).unwrap();
    assert!(!endpoint.tools.contains("stamp"));
    assert!(!endpoint.tools.contains("hivemind_broadcast"));
    assert!(endpoint.tools.contains("hivemind_post"));
    let allowed_only = SeatTools::new("b").with_permissions(vec![PermissionSet {
        id: "p".into(),
        allow_tools: Some(vec!["hivemind_read".into()]),
        ..PermissionSet::default()
    }]);
    let endpoint = server.serve_seat(allowed_only).unwrap();
    assert_eq!(endpoint.tools.iter().collect::<Vec<_>>(), ["hivemind_read"]);
    assert!(
        matches!(server.serve_seat(SeatTools::new("a")), Err(Error::DuplicateSeat(ref s)) if s == "a")
    );
    let shadow = SeatTools::new("c").with_programs(vec![tool(
        "hivemind_post",
        "stamp",
        "write",
        json!({"type": "object"}),
    )]);
    assert!(
        matches!(server.serve_seat(shadow), Err(Error::DuplicateTool(ref t)) if t == "hivemind_post")
    );
    assert!(server.withdraw_seat("a"));
    assert!(!server.withdraw_seat("a"));
    server.serve_seat(SeatTools::new("a")).unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_lowered_seats_tool_allow_list_is_all_it_is_served() {
    use std::sync::Arc;
    use support::Client;
    use tinyhivemind_hives::{Coordinator, CoordinatorOptions, MemoryStorage};
    use tinyhivemind_lang::ToolAccess;
    use tinyhivemind_mcp::{HiveToolServer, ServerOptions, Services};
    let integration = LoweredSeatIntegration {
        tools: ToolAccess {
            allow: vec!["hivemind_post".into(), "hivemind_read".into()],
            ..ToolAccess::default()
        },
        programs: vec![tool("stamp", "stamp", "write", json!({"type": "object"}))],
        ..LoweredSeatIntegration::default()
    };
    let seat_tools =
        SeatTools::from_lowered(&seat(integration), &RuntimeSettings::default(), context())
            .unwrap();
    let coordinator = Coordinator::new(
        "r".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap();
    let server = HiveToolServer::start(Services::new(coordinator), ServerOptions::default())
        .await
        .unwrap();
    let endpoint = server.serve_seat(seat_tools).unwrap();
    assert_eq!(
        endpoint.tools.iter().collect::<Vec<_>>(),
        ["hivemind_post", "hivemind_read", "stamp"]
    );
    let client = Client::of(&server, &endpoint);
    let listed: Vec<String> = client
        .tools()
        .await
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(listed, ["hivemind_read", "hivemind_post", "stamp"]);
    let refused = client
        .rpc(
            "tools/call",
            json!({"name": "hivemind_list_hives", "arguments": {}}),
        )
        .await;
    assert_eq!(refused["error"]["code"], -32602);
}

#[test]
fn a_schema_keyword_the_server_cannot_check_is_refused() {
    let args =
        json!({"type": "object", "properties": {"note": {"type": "string", "pattern": "^a"}}});
    let integration = LoweredSeatIntegration {
        programs: vec![tool("stamp", "stamp", "write", args)],
        ..LoweredSeatIntegration::default()
    };
    let refused =
        SeatTools::from_lowered(&seat(integration), &RuntimeSettings::default(), context());
    assert!(
        matches!(refused, Err(Error::UnsupportedSchema { ref tool, ref keyword }) if tool == "stamp" && keyword == "pattern"),
        "{refused:?}"
    );
}
