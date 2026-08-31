//! Unit tests for pure bounded mention dispatch.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{
    mention::{Mention, MentionTarget},
    roster::{Roster, RosterMember},
};
use serde_json::json;

fn members() -> Vec<RosterMember> {
    ["alice", "bob", "carol"]
        .into_iter()
        .map(|id| RosterMember {
            id: id.into(),
            name: None,
        })
        .collect()
}

fn mention(id: &str, offset: usize) -> Mention {
    Mention {
        target: MentionTarget::Agent { id: id.into() },
        text: format!("@{id}"),
        offset,
        quiet: false,
    }
}

fn input(mentions: Vec<Mention>, hop: u32) -> MentionDispatchInput {
    MentionDispatchInput {
        key: DispatchKey {
            trigger_sequence: 41,
        },
        conversation: DispatchConversation {
            desk_id: "eng".into(),
            thread_root: Some(7),
        },
        author_id: "alice".into(),
        content: "handoff".into(),
        mentions,
        hop,
    }
}

fn decide(policy: MentionDispatchPolicy, input: &MentionDispatchInput) -> MentionDispatchDecision {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    mention_dispatch(policy, input, &roster).unwrap()
}

fn assert_wire<T>(value: &T, expected: serde_json::Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Eq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_value(value).unwrap(), expected);
    assert_eq!(serde_json::from_value::<T>(expected).unwrap(), *value);
}

fn assert_requires_every_field<T>(wire: &serde_json::Value)
where
    T: serde::de::DeserializeOwned,
{
    let keys = wire
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(&key);
        assert!(
            serde_json::from_value::<T>(missing).is_err(),
            "{key} must be required"
        );
    }
}

#[test]
fn pins_every_payload_wire_form_round_trip_and_required_field() {
    let policy = MentionDispatchPolicy {
        enabled: true,
        max_hops: 2,
    };
    let key = DispatchKey {
        trigger_sequence: 41,
    };
    let conversation = DispatchConversation {
        desk_id: "eng".into(),
        thread_root: Some(7),
    };
    let value = input(vec![mention("bob", 0)], 0);
    let request = match decide(
        MentionDispatchPolicy {
            enabled: true,
            max_hops: 2,
        },
        &value,
    ) {
        MentionDispatchDecision::One { request } => request,
        other @ MentionDispatchDecision::None { .. } => {
            panic!("expected request, got {other:?}")
        }
    };
    let policy_wire = json!({"enabled": true, "max_hops": 2});
    let key_wire = json!({"trigger_sequence": 41});
    let conversation_wire = json!({"desk_id": "eng", "thread_root": 7});
    let input_wire = json!({
        "key": {"trigger_sequence": 41},
        "conversation": {"desk_id": "eng", "thread_root": 7},
        "author_id": "alice",
        "content": "handoff",
        "mentions": [{
            "target": {"kind": "agent", "id": "bob"},
            "text": "@bob",
            "offset": 0
        }],
        "hop": 0
    });
    let request_wire = json!({
        "key": {"trigger_sequence": 41}, "source_id": "alice", "target_id": "bob",
        "content": "handoff", "conversation": {"desk_id": "eng", "thread_root": 7},
        "child_hop": 1
    });
    assert_wire(&policy, policy_wire.clone());
    assert_wire(&key, key_wire.clone());
    assert_wire(&conversation, conversation_wire.clone());
    assert_wire(&value, input_wire.clone());
    assert_wire(&request, request_wire.clone());
    assert_requires_every_field::<MentionDispatchPolicy>(&policy_wire);
    assert_requires_every_field::<DispatchKey>(&key_wire);
    assert_requires_every_field::<DispatchConversation>(&conversation_wire);
    assert_requires_every_field::<MentionDispatchInput>(&input_wire);
    assert_requires_every_field::<MentionTurnRequest>(&request_wire);

    let no_dispatch_reasons = [
        (NoDispatchReason::Disabled, "disabled"),
        (NoDispatchReason::HopLimitReached, "hop_limit_reached"),
        (NoDispatchReason::SourceInactive, "source_inactive"),
        (
            NoDispatchReason::NoDirectAgentMention,
            "no_direct_agent_mention",
        ),
        (NoDispatchReason::SelfMention, "self_mention"),
        (NoDispatchReason::TargetInactive, "target_inactive"),
        (NoDispatchReason::HopOverflow, "hop_overflow"),
    ];
    for (reason, wire) in no_dispatch_reasons {
        assert_wire(&reason, json!(wire));
        let decision = MentionDispatchDecision::None { reason };
        let decision_wire = json!({"kind": "none", "reason": wire});
        assert_wire(&decision, decision_wire.clone());
        assert_requires_every_field::<MentionDispatchDecision>(&decision_wire);
    }
    let decision = MentionDispatchDecision::One {
        request: request.clone(),
    };
    let decision_wire = json!({"kind": "one", "request": request_wire});
    assert_wire(&decision, decision_wire.clone());
    assert_requires_every_field::<MentionDispatchDecision>(&decision_wire);

    assert_eq!(
        serde_json::from_value::<DispatchConversation>(json!({
            "desk_id": "eng",
            "thread_root": null
        }))
        .unwrap()
        .thread_root,
        None
    );
    assert!(serde_json::from_value::<DispatchConversation>(json!({"desk_id": "eng"})).is_err());
}

#[test]
fn disabled_and_zero_hops_stop_before_source_or_mentions() {
    let missing = input(vec![], 0);
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: false,
                max_hops: 9
            },
            &missing
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::Disabled
        }
    );
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 0
            },
            &missing
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::HopLimitReached
        }
    );
}

#[test]
fn disabled_and_exhausted_policy_precede_roster_validation() {
    let malformed = [
        RosterMember {
            id: "duplicate".into(),
            name: None,
        },
        RosterMember {
            id: "duplicate".into(),
            name: None,
        },
    ];
    let roster = Roster::new(&malformed, &[], &[]);
    let value = input(vec![mention("bob", 0)], 0);
    assert_eq!(
        mention_dispatch(
            MentionDispatchPolicy {
                enabled: false,
                max_hops: 2,
            },
            &value,
            &roster,
        )
        .unwrap(),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::Disabled,
        }
    );
    assert_eq!(
        mention_dispatch(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 0,
            },
            &value,
            &roster,
        )
        .unwrap(),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::HopLimitReached,
        }
    );
}

#[test]
fn honors_one_two_and_large_limits_without_a_library_cap() {
    for (max_hops, hop, expected_child) in [
        (1, 0, 1),
        (2, 1, 2),
        (1_000_000, 999_999, 1_000_000),
        (u32::MAX, u32::MAX - 1, u32::MAX),
    ] {
        let decision = decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops,
            },
            &input(vec![mention("bob", 0)], hop),
        );
        assert!(
            matches!(decision, MentionDispatchDecision::One { request } if request.child_hop == expected_child)
        );
    }
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: u32::MAX
            },
            &input(vec![mention("bob", 0)], u32::MAX)
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::HopLimitReached
        }
    );
}

#[test]
fn requires_an_active_source_after_policy_checks() {
    let mut value = input(vec![mention("bob", 0)], 0);
    value.author_id = "gone".into();
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2
            },
            &value
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::SourceInactive
        }
    );
}

#[test]
fn skips_quiet_and_non_agent_mentions_then_uses_reading_order() {
    let mut quiet = mention("carol", 0);
    quiet.quiet = true;
    let desk = Mention {
        target: MentionTarget::Desk { id: "eng".into() },
        text: "@#eng".into(),
        offset: 2,
        quiet: false,
    };
    let everyone = Mention {
        target: MentionTarget::Everyone,
        text: "@everyone".into(),
        offset: 3,
        quiet: false,
    };
    let decision = decide(
        MentionDispatchPolicy {
            enabled: true,
            max_hops: 2,
        },
        &input(
            vec![
                mention("carol", 20),
                everyone,
                mention("bob", 10),
                desk,
                quiet,
            ],
            0,
        ),
    );
    assert!(
        matches!(decision, MentionDispatchDecision::One { request } if request.target_id == "bob")
    );
}

#[test]
fn self_and_inactive_first_direct_mentions_fail_closed_without_fallback() {
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2
            },
            &input(vec![mention("bob", 5), mention("alice", 0)], 0)
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::SelfMention
        }
    );
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2
            },
            &input(vec![mention("bob", 5), mention("retired", 0)], 0)
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::TargetInactive
        }
    );
}

#[test]
fn non_agent_only_content_never_fans_out() {
    let values = vec![
        Mention {
            target: MentionTarget::Person { id: "p".into() },
            text: "@p".into(),
            offset: 0,
            quiet: false,
        },
        Mention {
            target: MentionTarget::Desk { id: "eng".into() },
            text: "@#eng".into(),
            offset: 3,
            quiet: false,
        },
        Mention {
            target: MentionTarget::Everyone,
            text: "@everyone".into(),
            offset: 9,
            quiet: false,
        },
    ];
    assert_eq!(
        decide(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2
            },
            &input(values, 0)
        ),
        MentionDispatchDecision::None {
            reason: NoDispatchReason::NoDirectAgentMention
        }
    );
}
