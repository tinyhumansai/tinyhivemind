//! Reading a tool call, and refusing one to the seat that made it.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::support::{members, post};
use crate::speech::{
    CallArguments, READ_DEFAULT, READ_MAX, ToolCall, Utterance, UtteranceRejection,
    check_recipients, interpret, read_limit,
};
use tinyhivemind_core::roster::Roster;

fn said(name: &str, arguments: &CallArguments<'_>) -> Utterance {
    match interpret(name, arguments).expect("a well-formed call") {
        ToolCall::Speak(utterance) => utterance,
        ToolCall::Read { limit } => panic!("expected speech, got a read of {limit}"),
    }
}

#[test]
fn a_post_is_one_utterance_with_its_message_trimmed() {
    assert_eq!(
        said(
            "post",
            &CallArguments {
                message: Some("  B holds at 10^18  "),
                ..Default::default()
            }
        ),
        post("B holds at 10^18"),
    );
}

#[test]
fn a_close_carries_its_message_and_says_the_work_is_finished() {
    let utterance = said(
        "close",
        &CallArguments {
            message: Some("Psi(10^18) = 62418970; checker signed off"),
            ..Default::default()
        },
    );
    assert!(utterance.closing());
    assert_eq!(utterance.message(), "Psi(10^18) = 62418970; checker signed off");
    assert!(!post("anything").closing(), "only a close closes");
}

#[test]
fn a_dm_strips_the_at_and_keeps_the_order_it_was_written() {
    let to = ["@checker".to_string(), " theory ".to_string()];
    assert_eq!(
        said(
            "dm",
            &CallArguments {
                message: Some("recheck the depth"),
                to: &to,
                ..Default::default()
            }
        ),
        Utterance::Dm {
            to: vec!["checker".into(), "theory".into()],
            message: "recheck the depth".into(),
        },
    );
}

#[test]
fn a_dm_naming_the_same_seat_twice_names_it_once() {
    let to = ["checker".to_string(), "@checker".to_string()];
    let Utterance::Dm { to: named, .. } = said(
        "dm",
        &CallArguments {
            message: Some("again"),
            to: &to,
            ..Default::default()
        },
    ) else {
        panic!("a dm");
    };
    assert_eq!(named, vec!["checker".to_string()]);
}

#[test]
fn refuses_a_message_that_is_absent_or_only_whitespace() {
    for message in [None, Some(""), Some("   \n ")] {
        for tool in ["post", "close", "dm"] {
            let to = ["checker".to_string()];
            assert_eq!(
                interpret(
                    tool,
                    &CallArguments {
                        message,
                        to: &to,
                        ..Default::default()
                    }
                ),
                Err(UtteranceRejection::EmptyText { field: "message" }),
                "{tool} with {message:?}",
            );
        }
    }
}

#[test]
fn refuses_a_dm_that_names_nobody() {
    let blanks = [" ".to_string(), "@".to_string()];
    for to in [&[][..], &blanks[..]] {
        assert_eq!(
            interpret(
                "dm",
                &CallArguments {
                    message: Some("hi"),
                    to,
                    ..Default::default()
                }
            ),
            Err(UtteranceRejection::NoRecipients),
            "{to:?}",
        );
    }
}

#[test]
fn refuses_a_tool_the_room_does_not_serve() {
    assert_eq!(
        interpret("shout", &CallArguments::default()),
        Err(UtteranceRejection::UnknownTool {
            name: "shout".into()
        }),
    );
}

#[test]
fn a_rejection_reads_as_a_sentence_the_seat_can_act_on() {
    assert_eq!(
        UtteranceRejection::EmptyText { field: "message" }.to_string(),
        "`message` must be a non-empty string",
    );
    assert_eq!(
        UtteranceRejection::NoRecipients.to_string(),
        "`to` must name at least one seat",
    );
    assert_eq!(
        UtteranceRejection::UnknownTool {
            name: "shout".into()
        }
        .to_string(),
        "unknown tool shout",
    );
    assert_eq!(
        UtteranceRejection::UnknownRecipient {
            id: "johnny".into()
        }
        .to_string(),
        "`to` names @johnny, who is not an active seat on this desk",
    );
    assert_eq!(
        UtteranceRejection::SelfRecipient.to_string(),
        "`to` names you; a message to yourself reaches nobody else",
    );
}

#[test]
fn a_read_is_clamped_rather_than_taken_at_its_word() {
    assert_eq!(read_limit(None), READ_DEFAULT);
    assert_eq!(read_limit(Some(0)), 1);
    assert_eq!(read_limit(Some(7)), 7);
    assert_eq!(read_limit(Some(10_000)), READ_MAX);
    assert_eq!(read_limit(Some(u64::MAX)), READ_MAX);
    assert_eq!(
        interpret(
            "read",
            &CallArguments {
                limit: Some(10_000),
                ..Default::default()
            }
        ),
        Ok(ToolCall::Read { limit: READ_MAX }),
    );
}

#[test]
fn refuses_a_dm_to_a_seat_the_roster_does_not_hold() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let utterance = Utterance::Dm {
        to: vec!["checker".into(), "johnny".into()],
        message: "recheck".into(),
    };
    assert_eq!(
        check_recipients(&utterance, "solver", &roster),
        Err(UtteranceRejection::UnknownRecipient {
            id: "johnny".into()
        }),
        "the first unresolvable id stops the check rather than being dropped",
    );
}

#[test]
fn refuses_a_dm_a_seat_addressed_only_to_itself() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let alone = Utterance::Dm {
        to: vec!["solver".into()],
        message: "thinking out loud".into(),
    };
    assert_eq!(
        check_recipients(&alone, "solver", &roster),
        Err(UtteranceRejection::SelfRecipient),
    );
    let with_a_peer = Utterance::Dm {
        to: vec!["solver".into(), "checker".into()],
        message: "thinking out loud".into(),
    };
    assert_eq!(
        check_recipients(&with_a_peer, "solver", &roster),
        Ok(()),
        "naming itself alongside a peer is a peer message",
    );
    assert_eq!(
        check_recipients(&post("to nobody in particular"), "solver", &roster),
        Ok(()),
        "a post has no recipients to check",
    );
}

#[test]
fn the_wire_form_is_what_a_host_writes_and_reads_back() {
    let cases = [
        (
            post("B holds"),
            serde_json::json!({ "kind": "post", "message": "B holds" }),
        ),
        (
            Utterance::Dm {
                to: vec!["checker".into()],
                message: "recheck".into(),
            },
            serde_json::json!({ "kind": "dm", "to": ["checker"], "message": "recheck" }),
        ),
        (
            Utterance::Close {
                message: "delivered".into(),
            },
            serde_json::json!({ "kind": "close", "message": "delivered" }),
        ),
    ];
    for (utterance, wire) in cases {
        assert_eq!(serde_json::to_value(&utterance).expect("serializes"), wire);
        assert_eq!(
            serde_json::from_value::<Utterance>(wire.clone()).expect("round trips"),
            utterance,
        );
    }
}
