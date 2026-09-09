//! Shared fixtures: one desk of four seats, and the helpers every submodule
//! uses to drive `interpret` and `commit_utterance`.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::speech::{CommitRequest, CommittedUtterance, Utterance, commit_utterance};
use tinyhivemind_core::{
    aside::AsidePolicy,
    desk::{Desk, DeskSet, ResponderMode},
    dispatch::DispatchConversation,
    roster::{Roster, RosterMember},
};

/// The desk the PE 1006 runs used, in miniature.
pub(super) fn members() -> Vec<RosterMember> {
    ["lead", "solver", "theory", "checker"]
        .into_iter()
        .map(|id| RosterMember {
            id: id.into(),
            name: Some(id.to_uppercase()),
        })
        .collect()
}

pub(super) fn desks() -> Vec<Desk> {
    vec![Desk {
        id: "pe1006".into(),
        name: "PE 1006".into(),
        description: None,
        members: vec![
            "lead".into(),
            "solver".into(),
            "theory".into(),
            "checker".into(),
        ],
        responder_mode: ResponderMode::Lead,
    }]
}

/// The policy the `desk` example runs: one peer, six rows, settlement owed.
pub(super) const ASIDES: AsidePolicy = AsidePolicy {
    enabled: true,
    max_members: 1,
    max_messages: 6,
    must_surface: true,
    require_thread: false,
};

/// Commit one utterance on a fresh desk with nothing spent and nothing owed.
pub(super) fn commit(speaker: &str, utterance: &Utterance) -> CommittedUtterance {
    commit_with(speaker, utterance, ASIDES, 0, false)
}

/// Commit one utterance, saying what the room has already spent.
pub(super) fn commit_with(
    speaker: &str,
    utterance: &Utterance,
    policy: AsidePolicy,
    spent: usize,
    unsettled: bool,
) -> CommittedUtterance {
    let members = members();
    let desks_value = desks();
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&desks_value, &[], &[], &[], &[]);
    let conversation = DispatchConversation {
        desk_id: "pe1006".into(),
        thread_root: None,
    };
    commit_utterance(&CommitRequest {
        utterance,
        speaker_id: speaker,
        conversation: &conversation,
        aside: policy,
        spent,
        unsettled,
        roster: &roster,
        desks: &desks,
    })
    .expect("the fixture roster and desks are well formed")
}

/// A `dm` to one peer.
pub(super) fn dm(to: &[&str], message: &str) -> Utterance {
    Utterance::Dm {
        to: to.iter().map(|id| (*id).to_string()).collect(),
        message: message.into(),
    }
}

/// A `post`.
pub(super) fn post(message: &str) -> Utterance {
    Utterance::Post {
        message: message.into(),
    }
}
