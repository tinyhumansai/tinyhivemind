//! Unit tests for the MCP transport under the room's tools.
//!
//! What a tool *means* is tested in `tinyhivemind::speech`. These cover the
//! wiring: that the served descriptors are the library's specs and not a second
//! statement of them, that an accepted call reaches the outbox and a refused one
//! does not, and that a drain survives what another process wrote.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tinyhivemind-desk-{name}-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Make a `tools/call` request the way an agent CLI does.
fn request(name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "params": { "name": name, "arguments": arguments } })
}

#[test]
fn drains_a_post_and_a_dm_in_the_order_they_were_said() {
    let outbox = scratch("outbox").join("said.jsonl");
    clear_outbox(&outbox);
    call(
        &request("post", serde_json::json!({ "message": "B holds at 10^18" })),
        &outbox,
        Path::new("/nope"),
    )
    .expect("a post is served");
    call(
        &request(
            "dm",
            serde_json::json!({ "to": ["@checker"], "message": "recheck the depth" }),
        ),
        &outbox,
        Path::new("/nope"),
    )
    .expect("a dm is served");
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
        "the @ is stripped by the library, and the order is the outbox's",
    );
}

#[test]
fn a_cleared_outbox_holds_nothing_from_the_turn_before() {
    let outbox = scratch("cleared").join("said.jsonl");
    call(
        &request("post", serde_json::json!({ "message": "last turn" })),
        &outbox,
        Path::new("/nope"),
    )
    .expect("a post is served");
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
         {\"kind\":\"post\",\"message\":\"the one real line\"}\n",
    )
    .expect("writes");
    assert_eq!(
        drain_outbox(&outbox),
        vec![Utterance::Post {
            message: "the one real line".into()
        }],
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
fn serves_the_librarys_tool_surface_and_never_a_second_statement_of_it() {
    let listed = tools();
    let served = listed.as_array().expect("an array");
    assert_eq!(
        served.len(),
        tool_specs().len(),
        "every spec is served, and nothing else is",
    );
    for (descriptor, spec) in served.iter().zip(tool_specs()) {
        assert_eq!(descriptor["name"], spec.name);
        assert_eq!(
            descriptor["description"], spec.description,
            "the description a seat reads is the library's, verbatim",
        );
        assert_eq!(descriptor["inputSchema"]["type"], "object");
        for parameter in spec.parameters {
            let property = &descriptor["inputSchema"]["properties"][parameter.name];
            assert!(
                !property.is_null(),
                "{}.{} is declared and not served",
                spec.name,
                parameter.name,
            );
            let expected = match parameter.kind {
                ParameterKind::Text => "string",
                ParameterKind::TextList => "array",
                ParameterKind::Count { .. } => "integer",
            };
            assert_eq!(property["type"], expected, "{}", parameter.name);
        }
    }
}

#[test]
fn a_required_argument_is_served_as_required() {
    let listed = tools();
    let dm = listed
        .as_array()
        .expect("an array")
        .iter()
        .find(|tool| tool["name"] == "dm")
        .expect("dm is served");
    assert_eq!(
        dm["inputSchema"]["required"],
        serde_json::json!(["to", "message"]),
    );
    let read = listed
        .as_array()
        .expect("an array")
        .iter()
        .find(|tool| tool["name"] == "read")
        .expect("read is served");
    assert_eq!(read["inputSchema"]["required"], serde_json::json!([]));
}

#[test]
fn an_accepted_call_reaches_the_outbox_and_says_so() {
    let outbox = scratch("call-post").join("said.jsonl");
    clear_outbox(&outbox);
    assert_eq!(
        call(
            &request("post", serde_json::json!({ "message": "  ready  " })),
            &outbox,
            Path::new("/nope")
        ),
        Ok("posted to the desk".into()),
    );
    assert_eq!(
        call(
            &request(
                "dm",
                serde_json::json!({ "to": ["@checker", "theory"], "message": "recheck" })
            ),
            &outbox,
            Path::new("/nope")
        ),
        Ok("sent to @checker, @theory".into()),
    );
    assert_eq!(
        call(
            &request("close", serde_json::json!({ "message": "delivered" })),
            &outbox,
            Path::new("/nope")
        ),
        Ok("posted to the desk; the desk will close after this turn".into()),
    );
    assert_eq!(drain_outbox(&outbox).len(), 3);
}

#[test]
fn a_refused_call_reaches_the_seat_and_not_the_room() {
    let outbox = scratch("call-bad").join("said.jsonl");
    clear_outbox(&outbox);
    let cases = [
        (
            request("post", serde_json::json!({ "message": "  " })),
            "`message` must be a non-empty string",
        ),
        (
            request("dm", serde_json::json!({ "to": [], "message": "hi" })),
            "`to` must name at least one seat",
        ),
        (request("shout", serde_json::json!({})), "unknown tool shout"),
        (
            request("close", serde_json::json!({ "message": "   " })),
            "`message` must be a non-empty string",
        ),
    ];
    for (bad, reason) in cases {
        assert_eq!(
            call(&bad, &outbox, Path::new("/nope")),
            Err(reason.to_string()),
            "the seat is handed the library's sentence, while it can still act on it",
        );
    }
    assert!(
        drain_outbox(&outbox).is_empty(),
        "a refused call is not a message",
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
    let shown = call(
        &request("read", serde_json::json!({ "limit": 50 })),
        Path::new("/nope"),
        &transcript,
    )
    .expect("reads");
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
    assert_eq!(
        call(
            &request("read", serde_json::json!({})),
            Path::new("/nope"),
            Path::new("/nonexistent.jsonl")
        ),
        Ok("(the desk has no messages yet)".into()),
    );
}
