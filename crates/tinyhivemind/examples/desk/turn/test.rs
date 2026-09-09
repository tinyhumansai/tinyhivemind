//! Unit tests for what a turn is taken to have said.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tinyhivemind-turn-{name}-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir.join("outbox.jsonl")
}

/// A turn that ran, produced text, and never marked any of it as a message.
fn narrated(text: &str) -> agent::TurnOutput {
    agent::TurnOutput {
        message: text.into(),
        posted: false,
        ..agent::TurnOutput::default()
    }
}

#[test]
fn a_tool_call_is_what_the_seat_said() {
    let outbox = scratch("tool");
    mcp::clear_outbox(&outbox);
    fs::write(
        &outbox,
        "{\"kind\":\"post\",\"message\":\"B(x,n) holds at 10^18\"}\n",
    )
    .expect("writes");
    let mut output = narrated("thinking out loud");
    let said = settle(&outbox, &mut output).expect("the seat spoke");
    assert_eq!(
        said.utterance,
        Utterance::Post {
            message: "B(x,n) holds at 10^18".into()
        },
    );
    assert_eq!(
        output.message, "B(x,n) holds at 10^18",
        "the room's message replaces the narration, not the other way round",
    );
    assert!(output.posted);
}

#[test]
fn the_last_of_several_calls_stands() {
    let outbox = scratch("twice");
    mcp::clear_outbox(&outbox);
    fs::write(
        &outbox,
        "{\"kind\":\"post\",\"message\":\"partial\"}\n\
         {\"kind\":\"close\",\"message\":\"settled\"}\n",
    )
    .expect("writes");
    let mut output = narrated("");
    let said = settle(&outbox, &mut output).expect("the seat spoke");
    assert_eq!(
        said.utterance,
        Utterance::Close {
            message: "settled".into()
        },
        "one message per turn, and the seat meant the second",
    );
}

#[test]
fn a_fence_still_counts_as_speech() {
    let outbox = scratch("fence");
    mcp::clear_outbox(&outbox);
    let mut output = agent::TurnOutput {
        message: "@checker the recursion is sublinear".into(),
        posted: true,
        ..agent::TurnOutput::default()
    };
    let said = settle(&outbox, &mut output).expect("a fenced message is a message");
    assert_eq!(
        said.utterance,
        Utterance::Post {
            message: "@checker the recursion is sublinear".into()
        },
        "the fence is the documented fallback for a CLI that cannot reach the tools",
    );
}

#[test]
fn narration_is_not_speech_however_much_of_it_there_is() {
    // Run 29's defect, both turns of it. The provider failed mid-flight after
    // twenty minutes of real work; the accumulated commentary was appended to
    // the transcript as though the seat had addressed the room, and because it
    // named nobody the desk could not hand the turn on either.
    let outbox = scratch("narration");
    mcp::clear_outbox(&outbox);
    let mut output = narrated(
        "Let me verify the small cases and understand the structure better.",
    );
    assert!(
        settle(&outbox, &mut output).is_none(),
        "a turn that called no tool and wrote no fence has not spoken",
    );
    assert!(!output.posted);
}

#[test]
fn an_empty_turn_says_nothing_rather_than_saying_the_empty_string() {
    let outbox = scratch("empty");
    mcp::clear_outbox(&outbox);
    for text in ["", "   \n  "] {
        let mut output = agent::TurnOutput {
            message: text.into(),
            // Even with the marker: a fence around nothing is still nothing.
            posted: true,
            ..agent::TurnOutput::default()
        };
        assert!(settle(&outbox, &mut output).is_none(), "{text:?}");
    }
}
