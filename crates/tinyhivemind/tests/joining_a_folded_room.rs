//! What a seat taking its first turn on a long room is handed.
//!
//! The account exists for exactly this reader: one that has never spoken, is
//! given a bounded window, and would otherwise be handed thirty rows and no
//! idea what the four hundred before them established. This drives only the
//! public surface, because the guarantee is a contract and not an example
//! detail — the `desk` example satisfies it by construction today, which is
//! not the same as satisfying it on purpose.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use tinyhivemind::{
    ChannelDigest, ChannelHead, Conversation, DigestPlan, DigestPolicy, Sequence, SessionAuthor,
    SessionMessage, apply_digest, aside::Audience, plan_digest,
};

fn conversation() -> Conversation {
    Conversation {
        desk_id: "pe1006".into(),
        desk_name: "PE 1006".into(),
        thread_root: None,
    }
}

fn account(through: u64) -> ChannelDigest {
    ChannelDigest {
        conversation: conversation(),
        through: Sequence(through),
        covered: through,
        generation: 3,
        text: "@theory established B(x,n) at ^12; @checker signed off on 62418970 at ^287".into(),
    }
}

fn window(from: u64, to: u64) -> Vec<SessionMessage> {
    (from..=to)
        .map(|sequence| SessionMessage {
            sequence: Sequence(sequence),
            author: SessionAuthor::Agent {
                id: "lead".into(),
                label: "Lead".into(),
            },
            content: format!("row {sequence}"),
            audience: Audience::Desk,
            elided: None,
        })
        .collect()
}

#[test]
fn a_seat_that_has_never_spoken_is_handed_the_account_and_not_the_scrollback() {
    let held = account(370);
    let history = apply_digest(Some(&held), &window(341, 400));

    assert_eq!(
        history.digest.as_deref(),
        Some(held.text.as_str()),
        "the reader who was not there is given what the room established",
    );
    assert_eq!(history.covered_through, Some(Sequence(370)));
    assert_eq!(
        history.messages.first().map(|message| message.sequence),
        Some(Sequence(371)),
        "and none of the rows the account already stands for",
    );
    assert_eq!(history.messages.len(), 30);
}

#[test]
fn a_room_large_enough_to_need_an_account_has_planned_one() {
    // The guarantee the size trigger buys: "a large room has an account" is
    // true by construction rather than by whether enough rows happened.
    let policy = DigestPolicy::from_token_budget(50_000);
    let large_but_short = ChannelHead {
        sequence: Sequence(34),
        unfolded_chars: 50_000 * DigestPolicy::CHARS_PER_TOKEN + 1,
    };
    assert!(
        matches!(
            plan_digest(None, large_but_short, policy),
            DigestPlan::Fold { .. }
        ),
        "24 rows of derivations is a fold the row count would never have planned",
    );
}

#[test]
fn a_seat_joining_a_room_with_no_account_is_handed_the_rows_untouched() {
    let history = apply_digest(None, &window(1, 12));
    assert_eq!(history.digest, None);
    assert_eq!(history.covered_through, None);
    assert_eq!(history.messages.len(), 12, "compaction is an optimization");
}
