//! Deterministic fuzz-style regression tests for public mention handling.
//!
//! These cases deliberately use arbitrary Unicode and punctuation-heavy
//! bodies. They run under the normal test suite, so parser invariants are
//! exercised in CI without requiring a separate fuzzer installation.

#![allow(clippy::expect_used)]

use tinyhivemind_core::{
    desk::{Desk, DeskSet, ResponderMode},
    mention::{MENTION_CAP, MentionAuthor, resolve},
    roster::{Roster, RosterMember},
};

fn snapshots() -> (Roster<'static>, DeskSet<'static>) {
    let members = Box::leak(Box::new([
        RosterMember {
            id: "alice".into(),
            name: Some("Alice".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob".into()),
        },
    ]));
    let desks = Box::leak(Box::new([Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: vec!["alice".into(), "bob".into()],
        responder_mode: ResponderMode::Lead,
    }]));
    (
        Roster::new(members, &[], &[]),
        DeskSet::new(desks, &[], &[], &[], &[]),
    )
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 7;
    *state ^= *state >> 9;
    *state
}

fn body(state: &mut u64) -> String {
    const FRAGMENTS: [&str; 14] = [
        "@Alice",
        "@Bob",
        "@#Engineering",
        "@everyone",
        "@unknown",
        "@@Alice",
        "`@Alice`",
        "```\n@Bob\n```",
        "\n",
        " ",
        "😀",
        "é",
        "_",
        "-",
    ];
    let count = usize::try_from(next(state) % 120).expect("bounded count");
    (0..count)
        .map(|_| {
            let index =
                usize::try_from(next(state) % FRAGMENTS.len() as u64).expect("fragment index fits");
            FRAGMENTS[index]
        })
        .collect()
}

#[test]
fn arbitrary_authored_bodies_are_deterministic_bounded_and_byte_aligned() {
    let (roster, desks) = snapshots();
    let mut state = 0x7c3d_12af_9b55_0042;

    for _case in 0..512 {
        let body = body(&mut state);
        let first = resolve(&body, None, &MentionAuthor::Other, &roster, &desks);
        let second = resolve(&body, None, &MentionAuthor::Other, &roster, &desks);

        assert_eq!(
            first, second,
            "the parser must be deterministic for {body:?}"
        );
        assert!(first.len() <= MENTION_CAP);
        for mention in first {
            assert!(body.is_char_boundary(mention.offset));
            assert_eq!(body[mention.offset..].chars().next(), Some('@'));
            assert!(body[mention.offset..].starts_with(&mention.text));
        }
    }
}
