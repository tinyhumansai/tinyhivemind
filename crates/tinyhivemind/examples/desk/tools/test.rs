//! Unit tests for the two renderings of one tool surface.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use std::{fs, path::PathBuf};
use tinyhivemind::speech::Utterance;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tinyhivemind-tools-{name}-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn serving(dir: &std::path::Path) -> Serving {
    Serving {
        outbox: dir.join("said.jsonl"),
        transcript: dir.join("transcript.jsonl"),
        desk: None,
        turn: None,
    }
}

#[test]
fn every_spec_becomes_a_tool_with_the_librarys_own_words() {
    let dir = scratch("render");
    let built = desk_tools(&serving(&dir));
    assert_eq!(built.len(), tool_specs().len());
    for (tool, spec) in built.iter().zip(tool_specs()) {
        assert_eq!(
            tool.name(),
            format!("{PREFIX}{}", spec.name),
            "a seat is briefed on the prefixed name and must find it",
        );
        assert_eq!(
            tool.description(),
            spec.description,
            "the description is contract text, rendered verbatim",
        );
        assert_eq!(
            tool.parameters_schema(),
            parameters_schema(spec),
            "one schema, whichever renderer asked for it",
        );
    }
}

#[test]
fn a_schema_states_the_shape_and_the_requirement_of_every_argument() {
    let dm = tool_specs()
        .iter()
        .find(|spec| spec.name == "dm")
        .expect("dm is served");
    assert_eq!(
        parameters_schema(dm),
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Seat ids, without the @.",
                },
                "message": { "type": "string" },
            },
            "required": ["to", "message"],
        }),
    );
}

#[tokio::test]
async fn a_tool_call_reaches_the_room_the_same_way_an_mcp_call_does() {
    let dir = scratch("execute");
    let serving = serving(&dir);
    mcp::clear_outbox(&serving.outbox);
    let post = DeskTool::new(
        tool_specs()
            .iter()
            .find(|spec| spec.name == "post")
            .expect("post is served"),
        &serving,
    );
    let result = post
        .execute(serde_json::json!({ "message": "  B holds at 10^18  " }))
        .await
        .expect("the room was reachable");
    assert!(!result.is_error, "{result:?}");
    assert_eq!(
        mcp::drain_outbox(&serving.outbox),
        vec![Utterance::Post {
            message: "B holds at 10^18".into()
        }],
    );
}

#[tokio::test]
async fn a_refused_call_is_a_tool_error_the_seat_reads_rather_than_a_failure() {
    let dir = scratch("refused");
    let serving = serving(&dir);
    mcp::clear_outbox(&serving.outbox);
    let dm = DeskTool::new(
        tool_specs()
            .iter()
            .find(|spec| spec.name == "dm")
            .expect("dm is served"),
        &serving,
    );
    let result = dm
        .execute(serde_json::json!({ "to": [], "message": "hi" }))
        .await
        .expect("the tool ran; it is the call that was wrong");
    assert!(result.is_error, "{result:?}");
    assert!(
        result.content.contains("must name at least one seat"),
        "the seat is handed the library's sentence: {result:?}",
    );
    assert!(
        mcp::drain_outbox(&serving.outbox).is_empty(),
        "a refused call is not a message",
    );
}
