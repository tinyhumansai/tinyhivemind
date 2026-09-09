//! Unit tests for the room-as-a-tool surface.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tinyhivemind-desk-{name}-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn drains_a_post_and_a_dm_in_the_order_they_were_said() {
    let outbox = scratch("outbox").join("said.jsonl");
    clear_outbox(&outbox);
    append(
        &outbox,
        &serde_json::json!({"kind":"post","message":"B holds at 10^18"}),
    )
    .expect("appends");
    append(
        &outbox,
        &serde_json::json!({"kind":"dm","to":["@checker"],"message":"recheck the depth"}),
    )
    .expect("appends");
    assert_eq!(
        drain_outbox(&outbox),
        vec![
            Utterance::Post {
                message: "B holds at 10^18".into()
            },
            Utterance::Dm {
                to: vec!["checker".into()],
                message: "recheck the depth".into()
            },
        ],
        "the @ is stripped, and order is kept"
    );
    assert_eq!(drain_outbox(&outbox)[0].message(), "B holds at 10^18");
}

#[test]
fn a_cleared_outbox_holds_nothing_from_the_turn_before() {
    let outbox = scratch("cleared").join("said.jsonl");
    append(
        &outbox,
        &serde_json::json!({"kind":"post","message":"last turn"}),
    )
    .expect("appends");
    clear_outbox(&outbox);
    assert!(drain_outbox(&outbox).is_empty());
}

#[test]
fn skips_a_garbled_line_rather_than_losing_the_turn() {
    let outbox = scratch("garbled").join("said.jsonl");
    clear_outbox(&outbox);
    fs::write(
        &outbox,
        "not json\n{\"kind\":\"post\"}\n{\"kind\":\"post\",\"message\":\"   \"}\n\
         {\"kind\":\"shout\",\"message\":\"hey\"}\n\
         {\"kind\":\"dm\",\"to\":[],\"message\":\"nobody\"}\n\
         {\"kind\":\"post\",\"message\":\"the one real line\"}\n",
    )
    .expect("writes");
    assert_eq!(
        drain_outbox(&outbox),
        vec![Utterance::Post {
            message: "the one real line".into()
        }]
    );
}

#[test]
fn drains_nothing_from_an_outbox_that_was_never_written() {
    assert!(drain_outbox(Path::new("/nonexistent/desk/outbox.jsonl")).is_empty());
}

#[test]
fn merges_the_server_into_an_existing_agent_configuration() {
    let config = config_block(
        Path::new("/bin/desk"),
        Path::new("/ws/.desk/outbox.jsonl"),
        Path::new("/ws/transcript.jsonl"),
        Some(r#"{"model":"ladder/reasoning"}"#),
    );
    let value: serde_json::Value = serde_json::from_str(&config).expect("valid json");
    assert_eq!(value["model"], "ladder/reasoning", "what was there is kept");
    assert_eq!(value["mcp"]["desk"]["type"], "local");
    assert_eq!(value["mcp"]["desk"]["enabled"], true);
    assert_eq!(
        value["mcp"]["desk"]["command"],
        serde_json::json!([
            "/bin/desk",
            "--mcp-server",
            "--outbox",
            "/ws/.desk/outbox.jsonl",
            "--transcript",
            "/ws/transcript.jsonl"
        ])
    );
}

#[test]
fn builds_a_configuration_from_nothing_or_from_nonsense() {
    for existing in [None, Some("not json at all"), Some("[1,2,3]")] {
        let config = config_block(
            Path::new("/bin/desk"),
            Path::new("/out"),
            Path::new("/t"),
            existing,
        );
        let value: serde_json::Value = serde_json::from_str(&config).expect("valid json");
        assert_eq!(value["mcp"]["desk"]["type"], "local", "for {existing:?}");
    }
}

#[test]
fn lists_exactly_the_four_tools_a_seat_may_call() {
    let listed = tools();
    let names: Vec<&str> = listed
        .as_array()
        .expect("an array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names, vec!["post", "dm", "close", "read"]);
    for tool in listed.as_array().expect("an array") {
        assert!(
            tool["inputSchema"]["type"] == "object",
            "every tool needs a schema: {tool}"
        );
    }
}

#[test]
fn a_post_call_records_one_utterance_and_says_so() {
    let outbox = scratch("call-post").join("said.jsonl");
    clear_outbox(&outbox);
    let request = serde_json::json!({
        "params": { "name": "post", "arguments": { "message": "  ready  " } }
    });
    assert_eq!(
        call(&request, &outbox, Path::new("/nope")),
        Ok("posted to the desk".into())
    );
    assert_eq!(
        drain_outbox(&outbox),
        vec![Utterance::Post {
            message: "ready".into()
        }]
    );
}

#[test]
fn refuses_a_call_that_names_nobody_or_says_nothing() {
    let outbox = scratch("call-bad").join("said.jsonl");
    clear_outbox(&outbox);
    let empty = serde_json::json!({
        "params": { "name": "post", "arguments": { "message": "  " } }
    });
    assert_eq!(
        call(&empty, &outbox, Path::new("/nope")),
        Err("`message` must be a non-empty string".into())
    );
    let nobody = serde_json::json!({
        "params": { "name": "dm", "arguments": { "to": [], "message": "hi" } }
    });
    assert_eq!(
        call(&nobody, &outbox, Path::new("/nope")),
        Err("`to` must name at least one seat".into())
    );
    let unknown = serde_json::json!({ "params": { "name": "shout", "arguments": {} } });
    assert_eq!(
        call(&unknown, &outbox, Path::new("/nope")),
        Err("unknown tool shout".into())
    );
    assert!(
        drain_outbox(&outbox).is_empty(),
        "a refused call is not a message"
    );
}

#[test]
fn a_dm_call_names_its_recipients_back_to_the_seat() {
    let outbox = scratch("call-dm").join("said.jsonl");
    clear_outbox(&outbox);
    let request = serde_json::json!({
        "params": {
            "name": "dm",
            "arguments": { "to": ["@checker", "theory"], "message": "recheck" }
        }
    });
    assert_eq!(
        call(&request, &outbox, Path::new("/nope")),
        Ok("sent to @checker, @theory".into())
    );
}

#[test]
fn reading_the_desk_shows_only_what_every_member_may_read() {
    let transcript = scratch("read").join("transcript.jsonl");
    fs::write(
        &transcript,
        "{\"sequence\":1,\"author\":{\"type\":\"person\",\"id\":\"steven\",\"label\":\"Steven\"},\
          \"content\":\"the brief\",\"audience\":{\"kind\":\"desk\"}}\n\
         {\"sequence\":2,\"author\":{\"type\":\"agent\",\"id\":\"theory\",\"label\":\"Theory\"},\
          \"content\":\"a private word\",\"audience\":{\"kind\":\"aside\",\"members\":[\"solver\"]}}\n\
         {\"sequence\":3,\"author\":{\"type\":\"agent\",\"id\":\"solver\",\"label\":\"Solver\"},\
          \"content\":\"B holds\",\"audience\":{\"kind\":\"desk\"}}\n",
    )
    .expect("writes");
    let request = serde_json::json!({
        "params": { "name": "read", "arguments": { "limit": 50 } }
    });
    let shown = call(&request, Path::new("/nope"), &transcript).expect("reads");
    assert!(shown.contains("[1] steven: the brief"), "{shown}");
    assert!(shown.contains("[3] solver: B holds"), "{shown}");
    assert!(!shown.contains("a private word"), "{shown}");
    assert!(
        shown.find("[1]") < shown.find("[3]"),
        "chronological: {shown}"
    );
}

#[test]
fn reading_a_desk_that_has_not_spoken_says_so() {
    let request = serde_json::json!({ "params": { "name": "read", "arguments": {} } });
    assert_eq!(
        call(
            &request,
            Path::new("/nope"),
            Path::new("/nonexistent.jsonl")
        ),
        Ok("(the desk has no messages yet)".into())
    );
}

#[test]
fn a_close_carries_its_message_like_any_other_utterance() {
    let outbox = scratch("close").join("said.jsonl");
    clear_outbox(&outbox);
    append(
        &outbox,
        &serde_json::json!({"kind":"close","message":"Psi(10^18) = 62418970; checker signed off"}),
    )
    .expect("appends");
    let said = drain_outbox(&outbox);
    assert_eq!(
        said,
        vec![Utterance::Close {
            message: "Psi(10^18) = 62418970; checker signed off".into()
        }],
        "a close is drained like a post"
    );
    assert_eq!(
        said[0].message(),
        "Psi(10^18) = 62418970; checker signed off",
        "and the room still gets the text"
    );
}

#[test]
fn the_close_tool_writes_a_close_row() {
    let outbox = scratch("close-call").join("said.jsonl");
    clear_outbox(&outbox);
    let transcript = scratch("close-call").join("absent.jsonl");
    let request = serde_json::json!({
        "params": { "name": "close", "arguments": { "message": "delivered" } }
    });
    call(&request, &outbox, &transcript).expect("close is served");
    assert_eq!(
        drain_outbox(&outbox),
        vec![Utterance::Close {
            message: "delivered".into()
        }]
    );
}

#[test]
fn a_close_with_no_message_is_refused_while_the_seat_can_still_fix_it() {
    let outbox = scratch("close-empty").join("said.jsonl");
    clear_outbox(&outbox);
    let transcript = scratch("close-empty").join("absent.jsonl");
    let request = serde_json::json!({
        "params": { "name": "close", "arguments": { "message": "   " } }
    });
    assert!(
        call(&request, &outbox, &transcript).is_err(),
        "an empty close is refused, not silently closed"
    );
    assert!(drain_outbox(&outbox).is_empty());
}
