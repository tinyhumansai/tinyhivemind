//! Building and reading the protocol's messages.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::*;
use serde_json::json;

#[test]
fn initialize_offers_no_file_system_and_no_terminal() {
    let params = initialize();
    assert_eq!(params["protocolVersion"], 1);
    assert_eq!(params["clientCapabilities"]["fs"]["readTextFile"], false);
    assert_eq!(params["clientCapabilities"]["fs"]["writeTextFile"], false);
    assert_eq!(params["clientCapabilities"]["terminal"], false);
    let result = json!({"agentInfo": {"name": "a", "version": "1.2.3"}});
    assert_eq!(agent_version(&result), Some("1.2.3"));
    assert_eq!(agent_version(&json!({})), None);
}

#[test]
fn the_host_server_carries_its_bearer_in_a_header() {
    let tools = ToolEndpoint::new(
        "hive",
        "http://127.0.0.1:1/mcp",
        "secret",
        ["hivemind_post".to_owned()],
    );
    let server = mcp_server(&tools);
    assert_eq!(server["type"], "http");
    assert_eq!(server["name"], "hive");
    assert_eq!(server["url"], "http://127.0.0.1:1/mcp");
    assert_eq!(
        server["headers"][0],
        json!({"name": "Authorization", "value": "Bearer secret"})
    );
    assert_eq!(
        prompt("s", "hi"),
        json!({"sessionId": "s", "prompt": [{"type": "text", "text": "hi"}]})
    );
}

#[test]
fn usage_counts_cached_input_as_input() {
    let result = json!({"usage": {"inputTokens": 90, "cachedReadTokens": 10, "cachedWriteTokens": 5,
        "outputTokens": 7, "thoughtTokens": 2, "totalTokens": 114}});
    assert_eq!(
        usage(&result),
        Some(TurnUsage {
            input_tokens: 105,
            cached_input_tokens: 10,
            output_tokens: 7,
            reasoning_tokens: 2
        })
    );
    assert_eq!(usage(&json!({"stopReason": "end_turn"})), None);
    assert_eq!(usage(&json!({"usage": null})), None);
}

#[test]
fn only_end_turn_is_a_normal_end() {
    assert_eq!(failure("end_turn"), None);
    for reason in ["cancelled", "max_tokens", "max_turn_requests", "refusal"] {
        assert_eq!(
            failure(reason),
            Some(format!("the agent stopped: {reason}"))
        );
    }
}

#[test]
fn updates_are_read_by_kind() {
    let text =
        json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hi"}});
    assert_eq!(update(&text), Update::Text("hi".into()));
    let image = json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "image"}});
    assert_eq!(update(&image), Update::Other);
    let call = json!({"sessionUpdate": "tool_call", "toolCallId": "c1", "title": "mcp.hive.hivemind_post",
        "rawInput": {"server": "hive", "tool": "hivemind_post"}});
    assert_eq!(
        update(&call),
        Update::ToolCall {
            id: "c1".into(),
            title: "mcp.hive.hivemind_post".into(),
            server: Some("hive".into())
        }
    );
    let done =
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "completed"});
    assert_eq!(
        update(&done),
        Update::ToolEnded {
            id: "c1".into(),
            failed: false
        }
    );
    let failed =
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "failed"});
    assert_eq!(
        update(&failed),
        Update::ToolEnded {
            id: "c1".into(),
            failed: true
        }
    );
    let running =
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c1", "status": "in_progress"});
    assert_eq!(update(&running), Update::Other);
    assert_eq!(
        update(&json!({"sessionUpdate": "usage_update"})),
        Update::Other
    );
}

#[test]
fn permission_answers_pick_the_options_by_kind() {
    let options = json!([
        {"optionId": "a1", "kind": "allow_always"},
        {"optionId": "a0", "kind": "allow_once"},
        {"optionId": "r0", "kind": "reject_once"},
    ]);
    assert_eq!(allow(&options).as_deref(), Some("a0"));
    assert_eq!(refuse(&options), selected("r0"));
    let only_always = json!([{"optionId": "a1", "kind": "allow_always"}]);
    assert_eq!(allow(&only_always).as_deref(), Some("a1"));
    assert_eq!(
        refuse(&only_always),
        json!({"outcome": {"outcome": "cancelled"}})
    );
    assert_eq!(allow(&json!(null)), None);
    assert_eq!(
        selected("x"),
        json!({"outcome": {"outcome": "selected", "optionId": "x"}})
    );
}
