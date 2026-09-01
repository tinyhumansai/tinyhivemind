//! Unit tests for the one-call atomic referral queue boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::EnqueueRefusal;
use std::{
    collections::HashSet,
    io,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tinyhivemind_core::{
    desk::{Desk, ResponderMode},
    dispatch::{DispatchConversation, DispatchKey},
    mention::{Mention, MentionTarget},
    roster::RosterMember,
};

#[derive(Default)]
struct AtomicQueue {
    calls: AtomicUsize,
    fail_once: AtomicBool,
    refuse: AtomicBool,
    durable: Mutex<DurableQueueState>,
}

#[derive(Default)]
struct DurableQueueState {
    keys: HashSet<(DispatchConversation, DispatchKey)>,
    children: Vec<Referral>,
}

impl ReferralQueue for AtomicQueue {
    fn enqueue_once(&self, referral: Referral) -> ReferralFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if self.fail_once.swap(false, Ordering::SeqCst) {
                return Err(Box::new(io::Error::other("failed before commit")) as BoxError);
            }
            if self.refuse.load(Ordering::SeqCst) {
                return Ok(EnqueueOutcome::Refused {
                    reason: EnqueueRefusal::TargetUnavailable,
                });
            }
            // The key is scoped by the conversation the trigger was committed
            // on, not the one the child runs on: two desks may each hold a
            // reply at the same sequence.
            let key = (referral.from.clone(), referral.key);
            let mut durable = self.durable.lock().unwrap();
            if !durable.keys.insert(key) {
                return Ok(EnqueueOutcome::Already);
            }
            durable.children.push(referral);
            Ok(EnqueueOutcome::Enqueued)
        })
    }
}

fn member(id: &str) -> RosterMember {
    RosterMember {
        id: id.to_owned(),
        name: Some(id.to_owned()),
    }
}

fn desk(id: &str, members: &[&str]) -> Desk {
    Desk {
        id: id.to_owned(),
        name: id.to_owned(),
        description: None,
        members: members.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Lead,
    }
}

const OPEN: ReferralPolicy = ReferralPolicy {
    enabled: true,
    max_hops: 2,
    reach: ReferralReach::Desks,
    returns: true,
};

fn asking(target: &str) -> ReferralInput {
    ReferralInput {
        key: DispatchKey {
            trigger_sequence: 7,
        },
        conversation: DispatchConversation {
            desk_id: "payments".to_owned(),
            thread_root: None,
        },
        author_id: "ada".to_owned(),
        content: format!("@{target} what does the gateway do on 503?"),
        mentions: vec![Mention {
            target: MentionTarget::Agent {
                id: target.to_owned(),
            },
            text: format!("@{target}"),
            offset: 0,
            quiet: false,
        }],
        hop: 0,
        origin: None,
    }
}

async fn attempt(
    queue: &AtomicQueue,
    policy: ReferralPolicy,
    input: &ReferralInput,
) -> ReferralOutcome {
    let members = [member("ada"), member("grace"), member("linus")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [
        desk("payments", &["ada", "grace"]),
        desk("platform", &["linus"]),
    ];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    dispatch_referral(queue, policy, input, &roster, &desks)
        .await
        .expect("snapshots are well formed")
}

#[tokio::test]
async fn a_crossing_referral_reaches_the_queue_exactly_once() {
    let queue = AtomicQueue::default();
    assert_eq!(
        attempt(&queue, OPEN, &asking("linus")).await,
        ReferralOutcome::Referred { crossed: true },
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 1);
    let durable = queue.durable.lock().unwrap();
    assert_eq!(durable.children.len(), 1);
    assert_eq!(durable.children[0].to.desk_id, "platform");
}

#[tokio::test]
async fn a_same_desk_referral_is_reported_as_not_having_crossed() {
    let queue = AtomicQueue::default();
    assert_eq!(
        attempt(&queue, OPEN, &asking("grace")).await,
        ReferralOutcome::Referred { crossed: false },
    );
}

#[tokio::test]
async fn a_pure_refusal_never_calls_the_queue() {
    let queue = AtomicQueue::default();
    assert_eq!(
        attempt(&queue, ReferralPolicy::DEFAULT, &asking("linus")).await,
        ReferralOutcome::NotReferred {
            reason: NoReferralReason::Disabled
        },
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_same_trigger_twice_creates_one_child_turn() {
    let queue = AtomicQueue::default();
    assert_eq!(
        attempt(&queue, OPEN, &asking("linus")).await,
        ReferralOutcome::Referred { crossed: true },
    );
    assert_eq!(
        attempt(&queue, OPEN, &asking("linus")).await,
        ReferralOutcome::Already
    );
    assert_eq!(queue.durable.lock().unwrap().children.len(), 1);
}

#[tokio::test]
async fn an_expected_refusal_is_an_outcome_rather_than_an_error() {
    let queue = AtomicQueue::default();
    queue.refuse.store(true, Ordering::SeqCst);
    assert_eq!(
        attempt(&queue, OPEN, &asking("linus")).await,
        ReferralOutcome::Refused {
            reason: EnqueueRefusal::TargetUnavailable
        },
    );
}

#[tokio::test]
async fn an_unexpected_host_failure_keeps_its_source() {
    let queue = AtomicQueue::default();
    queue.fail_once.store(true, Ordering::SeqCst);
    let members = [member("ada"), member("linus")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["ada"]), desk("platform", &["linus"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let error = dispatch_referral(&queue, OPEN, &asking("linus"), &roster, &desks)
        .await
        .expect_err("the queue failed");
    assert!(matches!(error, crate::Error::Enqueue { .. }));
    assert!(std::error::Error::source(&error).is_some());
    assert!(queue.durable.lock().unwrap().children.is_empty());
}

#[tokio::test]
async fn a_malformed_snapshot_is_a_typed_error_and_calls_nothing() {
    let queue = AtomicQueue::default();
    let members = [member(""), member("ada")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["ada"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    assert!(
        dispatch_referral(&queue, OPEN, &asking("linus"), &roster, &desks)
            .await
            .is_err()
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_outcome_pins_its_wire_form() {
    assert_eq!(
        serde_json::to_value(ReferralOutcome::Referred { crossed: true }).expect("serializes"),
        serde_json::json!({ "status": "referred", "crossed": true }),
    );
    assert_eq!(
        serde_json::to_value(ReferralOutcome::NotReferred {
            reason: NoReferralReason::SelfDesk
        })
        .expect("serializes"),
        serde_json::json!({ "status": "not_referred", "reason": "self_desk" }),
    );
}
