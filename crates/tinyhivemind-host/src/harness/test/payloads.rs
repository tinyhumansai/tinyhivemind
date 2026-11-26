//! Wire forms, and what `Debug` never shows.
use super::support::plan;
use crate::harness::{
    Approver, DenyAll, Error, Event, HarnessKind, Progress, Started, ToolApproval, ToolDecision,
    ToolEndpoint, TurnUsage,
};
use anyhow::{Context as _, Result};
use serde_json::json;
use std::path::Path;

#[test]
fn each_kind_has_the_name_a_package_uses() -> Result<()> {
    for (kind, name) in [
        (HarnessKind::OpenHuman, "openhuman"),
        (HarnessKind::OpenClaw, "openclaw"),
        (HarnessKind::Codex, "codex"),
    ] {
        assert_eq!(serde_json::to_value(kind)?, json!(name));
        assert_eq!(serde_json::from_value::<HarnessKind>(json!(name))?, kind);
        assert_eq!(kind.to_string(), name);
    }
    assert!(serde_json::from_value::<HarnessKind>(json!("open_human")).is_err());
    Ok(())
}

#[test]
fn an_endpoint_never_shows_its_bearer() {
    let endpoint = ToolEndpoint::new(
        "hivemind",
        "http://127.0.0.1:9/mcp",
        "s3cret",
        ["post".into()],
    );
    let text = format!("{endpoint:?} {endpoint:#?}");
    assert!(!text.contains("s3cret"), "{text}");
    assert!(text.contains("hivemind") && text.contains("redacted"));
    assert_eq!(endpoint.bearer(), "s3cret");
}

#[test]
fn a_seat_plan_shows_its_seat_and_not_its_prompt_or_settings() {
    let mut plan = plan("scout", Path::new("/srv/scout"));
    plan.seat.prompt = "private prompt".into();
    plan.seat.config = json!({"k": "private setting"});
    let text = format!("{plan:?}");
    assert!(text.contains("scout") && text.contains("/srv/scout"));
    assert!(!text.contains("private"), "{text}");
}

#[test]
fn a_plans_turn_timeout_is_the_seats_own_limit() -> Result<()> {
    let mut plan = plan("scout", Path::new("/srv/scout"));
    assert_eq!(plan.turn_timeout(), None);
    plan.seat.integration.limits.timeout_ms = Some(1500);
    assert_eq!(plan.turn_timeout().context("missing")?.as_millis(), 1500);
    Ok(())
}

#[test]
fn usage_of_a_turn_is_the_difference_of_totals() {
    let usage = |input, output| TurnUsage {
        input_tokens: input,
        output_tokens: output,
        ..TurnUsage::default()
    };
    assert_eq!(usage(30, 9).since(usage(10, 4)), Some(usage(20, 5)));
    assert_eq!(usage(30, 9).since(usage(0, 0)), Some(usage(30, 9)));
    // A total that went down is a different thread, not a negative turn.
    assert_eq!(usage(30, 3).since(usage(10, 4)), None);
}

#[test]
fn events_have_a_pinned_wire_form() -> Result<()> {
    let usage = TurnUsage {
        input_tokens: 1,
        cached_input_tokens: 2,
        output_tokens: 3,
        reasoning_tokens: 4,
    };
    let started = Started {
        pid: 7,
        version: "1.2".into(),
        tools: ["a".to_owned()].into(),
        settings: ["m".to_owned()].into(),
    };
    let progress = |progress| Event::Progress {
        turn_id: "t".into(),
        progress,
    };
    for (event, wire) in [
        (
            Event::Started(started),
            json!({"event": "started", "pid": 7, "version": "1.2", "tools": ["a"],
                "settings": ["m"]}),
        ),
        (
            Event::Usage {
                turn_id: "t".into(),
                usage,
            },
            json!({"event": "usage", "turn_id": "t", "usage": {"input_tokens": 1,
                "cached_input_tokens": 2, "output_tokens": 3, "reasoning_tokens": 4}}),
        ),
        (
            progress(Progress::ModelCall { iteration: 2 }),
            json!({"event": "progress", "turn_id": "t",
                "progress": {"kind": "model_call", "iteration": 2}}),
        ),
        (
            progress(Progress::Text { delta: "d".into() }),
            json!({"event": "progress", "turn_id": "t",
                "progress": {"kind": "text", "delta": "d"}}),
        ),
        (
            progress(Progress::ToolCall {
                call_id: "c".into(),
                tool: "a".into(),
            }),
            json!({"event": "progress", "turn_id": "t",
                "progress": {"kind": "tool_call", "call_id": "c", "tool": "a"}}),
        ),
        (
            progress(Progress::ToolResult {
                call_id: "c".into(),
                failed: true,
            }),
            json!({"event": "progress", "turn_id": "t",
                "progress": {"kind": "tool_result", "call_id": "c", "failed": true}}),
        ),
    ] {
        assert_eq!(serde_json::to_value(&event)?, wire);
        assert_eq!(serde_json::from_value::<Event>(wire)?, event);
    }
    Ok(())
}

#[tokio::test]
async fn approvals_are_denied_unless_the_host_decides_otherwise() -> Result<()> {
    let request = ToolApproval {
        seat: "scout".into(),
        turn_id: "t".into(),
        call_id: "c".into(),
        tool: "shell".into(),
        arguments: json!({"command": "ls"}),
    };
    let decision = DenyAll.decide(request).await;
    assert!(matches!(decision, ToolDecision::Deny { .. }));
    assert_eq!(
        serde_json::to_value(decision)?,
        json!({"decision": "deny", "reason": "this host approves no harness tool"})
    );
    assert_eq!(
        serde_json::to_value(ToolDecision::Allow)?,
        json!({"decision": "allow"})
    );
    Ok(())
}

#[test]
fn a_port_error_reaches_the_coordinator_as_its_own_message() {
    let error: tinyhivemind_hives::Error = Error::Harness("broken pipe".into()).into();
    assert!(error.to_string().contains("harness failed: broken pipe"));
    #[cfg(unix)]
    {
        let error: Error = crate::process::Error::Unsupervised.into();
        assert!(error.to_string().contains("supervisor"));
    }
    let resume = Error::Resume {
        session_id: "s1".into(),
        reason: "no such thread".into(),
    };
    assert!(resume.to_string().contains("s1"));
}

#[test]
fn a_refused_seat_keeps_the_clients_own_error() {
    let reason = std::io::Error::other("no provider");
    let error = Error::Refused(Box::new(reason));
    assert_eq!(
        error.to_string(),
        "the seat cannot run on this harness: no provider"
    );
    let Error::Refused(source) = error else {
        unreachable!("built as refused")
    };
    assert!(source.downcast_ref::<std::io::Error>().is_some());
}
