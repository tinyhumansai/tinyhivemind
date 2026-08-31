//! Unit tests for the one-call atomic mention-turn queue boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::{
    collections::HashSet,
    io,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tinyteams_core::{
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        MentionTurnRequest,
    },
    mention::{Mention, MentionTarget},
    roster::{Roster, RosterMember},
};

#[derive(Default)]
struct AtomicQueue {
    calls: AtomicUsize,
    keys: Mutex<HashSet<(DispatchConversation, DispatchKey)>>,
}

impl MentionTurnQueue for AtomicQueue {
    fn enqueue_once(&self, request: MentionTurnRequest) -> MentionTurnFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let inserted = self
                .keys
                .lock()
                .unwrap()
                .insert((request.conversation, request.key));
            Ok(if inserted {
                EnqueueOutcome::Enqueued
            } else {
                EnqueueOutcome::Already
            })
        })
    }
}

struct FixedQueue {
    calls: AtomicUsize,
    outcome: std::result::Result<EnqueueOutcome, ()>,
}

impl MentionTurnQueue for FixedQueue {
    fn enqueue_once(&self, _: MentionTurnRequest) -> MentionTurnFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            self.outcome
                .map_err(|()| Box::new(io::Error::other("queue failed")) as BoxError)
        })
    }
}

fn fixture() -> (Vec<RosterMember>, MentionDispatchInput) {
    (
        vec![
            RosterMember {
                id: "alice".into(),
                name: None,
            },
            RosterMember {
                id: "bob".into(),
                name: None,
            },
        ],
        MentionDispatchInput {
            key: DispatchKey {
                trigger_sequence: 9,
            },
            conversation: DispatchConversation {
                desk_id: "eng".into(),
                thread_root: Some(4),
            },
            author_id: "alice".into(),
            content: "please take this @bob".into(),
            mentions: vec![Mention {
                target: MentionTarget::Agent { id: "bob".into() },
                text: "@bob".into(),
                offset: 17,
                quiet: false,
            }],
            hop: 0,
        },
    )
}

fn policy() -> MentionDispatchPolicy {
    MentionDispatchPolicy {
        enabled: true,
        max_hops: 2,
    }
}

#[tokio::test]
async fn no_decision_calls_the_queue_zero_times() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::default();
    let outcome = dispatch_mention(
        &queue,
        MentionDispatchPolicy {
            enabled: false,
            max_hops: 2,
        },
        &input,
        &roster,
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        MentionDispatchOutcome::NotDispatched {
            reason: tinyteams_core::dispatch::NoDispatchReason::Disabled
        }
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_decision_calls_once_and_maps_every_expected_outcome() {
    for (queued, expected) in [
        (EnqueueOutcome::Enqueued, MentionDispatchOutcome::Enqueued),
        (EnqueueOutcome::Already, MentionDispatchOutcome::Already),
        (
            EnqueueOutcome::Unauthorized,
            MentionDispatchOutcome::Refused {
                outcome: EnqueueOutcome::Unauthorized,
            },
        ),
        (
            EnqueueOutcome::TargetUnavailable,
            MentionDispatchOutcome::Refused {
                outcome: EnqueueOutcome::TargetUnavailable,
            },
        ),
        (
            EnqueueOutcome::FeatureDisabled,
            MentionDispatchOutcome::Refused {
                outcome: EnqueueOutcome::FeatureDisabled,
            },
        ),
    ] {
        let (members, input) = fixture();
        let roster = Roster::new(&members, &[], &[]);
        let queue = FixedQueue {
            calls: AtomicUsize::new(0),
            outcome: Ok(queued),
        };
        assert_eq!(
            dispatch_mention(&queue, policy(), &input, &roster)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(queue.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn an_unexpected_queue_failure_is_typed_preserves_source_and_never_retries() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = FixedQueue {
        calls: AtomicUsize::new(0),
        outcome: Err(()),
    };
    let error = dispatch_mention(&queue, policy(), &input, &roster)
        .await
        .unwrap_err();
    assert!(matches!(error, crate::Error::Enqueue { .. }));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(queue.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_duplicate_requests_enqueue_exactly_once() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::default();
    let (left, right) = tokio::join!(
        dispatch_mention(&queue, policy(), &input, &roster),
        dispatch_mention(&queue, policy(), &input, &roster),
    );
    let mut outcomes = [left.unwrap(), right.unwrap()];
    outcomes.sort_by_key(|outcome| matches!(outcome, MentionDispatchOutcome::Already));
    assert_eq!(
        outcomes,
        [
            MentionDispatchOutcome::Enqueued,
            MentionDispatchOutcome::Already
        ]
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn idempotency_key_is_bound_to_conversation_scope() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::default();
    let mut other = input.clone();
    other.conversation.thread_root = Some(5);
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(
        dispatch_mention(&queue, policy(), &other, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Already
    );
}

#[test]
fn pins_runtime_outcome_wire_forms() {
    assert_eq!(
        serde_json::to_value(MentionDispatchOutcome::Refused {
            outcome: EnqueueOutcome::Unauthorized
        })
        .unwrap(),
        serde_json::json!({"status": "refused", "outcome": "unauthorized"})
    );
}
