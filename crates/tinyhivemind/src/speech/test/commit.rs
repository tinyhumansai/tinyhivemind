//! What one accepted utterance becomes.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::support::{ASIDES, commit, commit_with, dm, post};
use crate::speech::Utterance;
use tinyhivemind_core::{
    aside::{Audience, NoAsideReason},
    mention::MentionTarget,
};

fn agents(committed: &crate::speech::CommittedUtterance) -> Vec<String> {
    committed
        .mentions
        .iter()
        .filter_map(|mention| match &mention.target {
            MentionTarget::Agent { id } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_post_is_one_desk_row_with_its_mentions_resolved() {
    let committed = commit("solver", &post("@checker please verify, then @theory"));
    assert_eq!(committed.audience, Audience::Desk);
    assert_eq!(agents(&committed), vec!["checker", "theory"]);
    assert!(!committed.closing);
    assert_eq!(committed.refusal, None);
    assert_eq!(committed.content, "@checker please verify, then @theory");
}

#[test]
fn a_dm_takes_its_audience_from_the_field_and_not_from_the_prose() {
    let committed = commit("solver", &dm(&["checker"], "the depth measures 1.23n"));
    assert_eq!(
        committed.audience,
        Audience::Aside {
            members: vec!["checker".into()]
        },
        "the message names nobody, and the audience is still exactly who `to` named",
    );
    assert_eq!(agents(&committed), vec!["checker"]);
    assert_eq!(committed.refusal, None);
}

#[test]
fn a_dm_recipient_the_grammar_would_not_have_matched_still_reaches_them() {
    // The old host spelled `to` back into "@id" and re-read it through the
    // mention grammar. A body that opens a code span swallows the rest of the
    // line, so a recipient re-parsed out of prose could vanish silently.
    let committed = commit("solver", &dm(&["checker"], "`@checker` is a name in prose"));
    assert_eq!(
        committed.audience,
        Audience::Aside {
            members: vec!["checker".into()]
        },
        "the field is the address; masking applies to the body, not to `to`",
    );
    assert_eq!(agents(&committed), vec!["checker"]);
}

#[test]
fn a_dm_whose_text_names_a_peer_hands_that_peer_the_turn() {
    let committed = commit("solver", &dm(&["checker"], "@theory take this next"));
    assert_eq!(
        agents(&committed),
        vec!["theory"],
        "who reads it and who goes next are different questions",
    );
    assert_eq!(
        committed.audience,
        Audience::Aside {
            members: vec!["checker".into()]
        },
    );
}

#[test]
fn a_refused_aside_is_a_desk_row_carrying_the_reason() {
    // Six rows is the budget; a seventh cannot be part of the same aside.
    let committed = commit_with("solver", &dm(&["checker"], "one more"), ASIDES, 6, false);
    assert_eq!(
        committed.audience,
        Audience::Desk,
        "a refusal fails toward the room, never toward silence",
    );
    assert_eq!(committed.refusal, Some(NoAsideReason::BudgetSpent));
    assert_eq!(committed.content, "one more");
}

#[test]
fn an_aside_the_policy_disables_is_refused_with_that_reason() {
    let off = tinyhivemind_core::aside::AsidePolicy {
        enabled: false,
        ..ASIDES
    };
    let committed = commit_with("solver", &dm(&["checker"], "quietly"), off, 0, false);
    assert_eq!(committed.audience, Audience::Desk);
    assert_eq!(committed.refusal, Some(NoAsideReason::Disabled));
}

#[test]
fn a_dm_naming_more_peers_than_the_policy_allows_is_refused() {
    let committed = commit("solver", &dm(&["checker", "theory"], "both of you"));
    assert_eq!(committed.audience, Audience::Desk);
    assert_eq!(committed.refusal, Some(NoAsideReason::AudienceTooLarge));
}

#[test]
fn the_marker_a_seat_writes_reaches_the_same_audience_as_the_tool() {
    let committed = commit("solver", &post("!aside @checker the depth measures 1.23n"));
    assert_eq!(
        committed.audience,
        Audience::Aside {
            members: vec!["checker".into()]
        },
        "a seat briefed on the grammar rather than the tool is not penalised",
    );
}

#[test]
fn a_close_appends_its_row_and_says_the_work_is_finished() {
    let committed = commit(
        "lead",
        &Utterance::Close {
            message: "Psi(10^18) = 62418970, signed off by @checker".into(),
        },
    );
    assert!(committed.closing, "the host is told, and the host decides");
    assert_eq!(committed.audience, Audience::Desk);
    assert_eq!(
        committed.content,
        "Psi(10^18) = 62418970, signed off by @checker",
        "the message is never lost to the closing",
    );
}

#[test]
fn a_post_that_names_nobody_hands_the_turn_to_nobody() {
    let committed = commit("solver", &post("still working"));
    assert!(committed.mentions.is_empty());
    assert_eq!(committed.audience, Audience::Desk);
    assert_eq!(committed.refusal, None);
}
