//! Unit tests for what the folder is asked.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::compose;
use tinyhivemind::{
    Conversation, DigestRequest, Sequence, SessionAuthor, SessionMessage, aside::Audience,
};

fn request(prior: Option<&str>) -> DigestRequest {
    DigestRequest {
        conversation: Conversation {
            desk_id: "pe1006".into(),
            desk_name: "PE 1006".into(),
            thread_root: None,
        },
        prior: prior.map(str::to_string),
        messages: vec![
            SessionMessage {
                sequence: Sequence(7),
                author: SessionAuthor::Agent {
                    id: "solver".into(),
                    label: "Solver".into(),
                },
                content: "B(g,10^18) = 79414112".into(),
                audience: Audience::Desk,
                elided: None,
            },
            SessionMessage {
                sequence: Sequence(8),
                author: SessionAuthor::System {
                    kind: "workspace".into(),
                    label: "workspace".into(),
                },
                content: "@solver wrote psi_sublinear.py".into(),
                audience: Audience::Desk,
                elided: None,
            },
        ],
        through: Sequence(8),
        budget_chars: 4000,
        pinned: Vec::new(),
    }
}

#[test]
fn asks_for_an_account_rather_than_a_summary_of_activity() {
    let prompt = compose(&request(None));
    assert!(
        prompt.contains("stands for everything the room said"),
        "{prompt}"
    );
    assert!(prompt.contains("At most 4000 characters"), "{prompt}");
    assert!(
        prompt.contains("Never write a number the messages above do not contain"),
        "{prompt}"
    );
    assert!(
        !prompt.contains("## The account so far"),
        "there is no prior account to rewrite"
    );
}

#[test]
fn hands_the_prior_account_over_to_be_rewritten_not_appended_to() {
    let prompt = compose(&request(Some("the room verified B at n<=610")));
    assert!(prompt.contains("## The account so far"), "{prompt}");
    assert!(prompt.contains("the room verified B at n<=610"), "{prompt}");
    assert!(prompt.contains("Do not append to it"), "{prompt}");
}

#[test]
fn renders_every_author_kind_the_room_can_carry() {
    let prompt = compose(&request(None));
    assert!(
        prompt.contains("[7] @solver: B(g,10^18) = 79414112"),
        "{prompt}"
    );
    assert!(
        prompt.contains("[8] system/workspace: @solver wrote psi_sublinear.py"),
        "{prompt}"
    );
}
