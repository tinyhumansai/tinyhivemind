//! Unit tests for mention grammar, normalization, and routing decisions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Mention, MentionAuthor, MentionTarget, direct_responder, mentioned_members, resolve};
use crate::{
    desk::{Desk, DeskSet},
    roster::{Person, Roster, RosterMember},
};
use serde_json::json;

fn member(id: &str, name: Option<&str>) -> RosterMember {
    RosterMember {
        id: id.into(),
        name: name.map(str::to_owned),
    }
}

fn desk(id: &str, name: &str, members: &[&str]) -> Desk {
    Desk {
        id: id.into(),
        name: name.into(),
        description: None,
        members: members.iter().map(|id| (*id).to_owned()).collect(),
    }
}

fn target_agent(id: &str) -> MentionTarget {
    MentionTarget::Agent { id: id.into() }
}

fn mention(target: MentionTarget, text: &str, offset: usize) -> Mention {
    Mention {
        target,
        text: text.into(),
        offset,
        quiet: false,
    }
}

#[test]
fn pins_exact_mention_and_author_wire_shapes() {
    let value = serde_json::to_value(mention(target_agent("alice"), "@alice", 2)).unwrap();
    assert_eq!(
        value,
        json!({
            "target": {"kind": "agent", "id": "alice"},
            "text": "@alice",
            "offset": 2
        })
    );
    assert_eq!(
        serde_json::to_value(MentionTarget::Everyone).unwrap(),
        json!({"kind": "everyone"})
    );
    assert_eq!(
        serde_json::to_value(MentionAuthor::Person { id: "p1".into() }).unwrap(),
        json!({"kind": "person", "id": "p1"})
    );
    let quiet = Mention {
        quiet: true,
        ..mention(MentionTarget::Desk { id: "eng".into() }, "@#eng", 0)
    };
    assert_eq!(serde_json::to_value(quiet).unwrap()["quiet"], true);
}

#[test]
fn extracts_at_allowed_boundaries_with_punctuation_case_and_utf8_offsets() {
    let members = [member("alice", Some("Alice"))];
    let desks = [desk("eng", "Engineering", &["alice"])];
    let roster = Roster::new(&members, &[], &[]);
    let desk_set = DeskSet::new(&desks, &[], &[], &[], &[]);
    let body = "é (@ALICE), x@alice [@#Engineering!] @channel; @alice";
    let found = resolve(body, None, &MentionAuthor::Other, &roster, &desk_set);
    assert_eq!(found.len(), 4);
    assert_eq!(found[0], mention(target_agent("alice"), "@ALICE", 4));
    assert_eq!(found[1].target, MentionTarget::Desk { id: "eng".into() });
    assert_eq!(
        &body[found[1].offset..found[1].offset + found[1].text.len()],
        "@#Engineering"
    );
    assert_eq!(found[2].target, MentionTarget::Everyone);
    assert_eq!(found[3].target, target_agent("alice"));
}

#[test]
fn rejects_a_mention_opened_after_a_semicolon() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);

    assert!(
        resolve(
            "prefix;@alice",
            None,
            &MentionAuthor::Other,
            &roster,
            &desks
        )
        .is_empty()
    );
}

#[test]
fn rejects_bad_openers_alias_starts_and_unknown_names() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert!(
        resolve(
            "x@alice @_missing @nobody",
            None,
            &MentionAuthor::Other,
            &roster,
            &desks
        )
        .is_empty()
    );
}

#[test]
fn ambiguous_aliases_fail_closed_but_desk_syntax_bypasses_other_namespaces() {
    let members = [member("agent", Some("Shared"))];
    let people = [Person {
        id: "p".into(),
        label: "Shared".into(),
    }];
    let desk_records = [desk("desk", "Shared", &["agent"])];
    let roster = Roster::new(&members, &people, &[]);
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let found = resolve(
        "@Shared @#Shared",
        None,
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(
        found,
        vec![mention(
            MentionTarget::Desk { id: "desk".into() },
            "@#Shared",
            8
        )]
    );
}

#[test]
fn assigns_stable_ascii_person_slug_collision_suffixes() {
    let people = [
        Person {
            id: "p1".into(),
            label: "Ada Lovelace".into(),
        },
        Person {
            id: "p2".into(),
            label: "Ada-Lovelace".into(),
        },
    ];
    let roster = Roster::new(&[], &people, &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let found = resolve(
        "@ada_lovelace @ada_lovelace_2",
        None,
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(found[0].target, MentionTarget::Person { id: "p1".into() });
    assert_eq!(found[1].target, MentionTarget::Person { id: "p2".into() });
}

#[test]
fn allocates_person_slugs_without_colliding_with_an_existing_suffixed_base() {
    let people = [
        Person {
            id: "p1".into(),
            label: "Ada".into(),
        },
        Person {
            id: "p2".into(),
            label: "Ada".into(),
        },
        Person {
            id: "p3".into(),
            label: "Ada 2".into(),
        },
    ];
    let roster = Roster::new(&[], &people, &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let aliases = super::aliases(&roster, &desks);

    for (text, id) in [("ada", "p1"), ("ada_2", "p2"), ("ada_2_2", "p3")] {
        assert!(aliases.iter().any(|alias| {
            alias.text == text && alias.target == MentionTarget::Person { id: id.into() }
        }));
    }

    let found = resolve(
        "@ada @ada_2 @ada_2_2",
        None,
        &MentionAuthor::Other,
        &roster,
        &desks,
    );

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].target, MentionTarget::Person { id: "p2".into() });
    assert_eq!(found[1].target, MentionTarget::Person { id: "p3".into() });
}

#[test]
fn matches_unicode_aliases_when_case_mapping_changes_utf8_length() {
    let members = [member("agent", Some("İ"))];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let body = "é @i̇";

    let found = resolve(body, None, &MentionAuthor::Other, &roster, &desks);

    assert_eq!(found, vec![mention(target_agent("agent"), "@i̇", 3)]);
    assert_eq!(&body[found[0].offset..], "@i̇");
}

#[test]
fn ignores_closed_inline_and_fenced_code_but_not_an_unclosed_inline_tick() {
    let members = [member("alice", None), member("bob", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let body = "`@alice` @bob\n```rust\n@alice\n```\n@alice";
    let found = resolve(body, None, &MentionAuthor::Other, &roster, &desks);
    assert_eq!(
        found
            .iter()
            .map(|item| item.target.clone())
            .collect::<Vec<_>>(),
        vec![target_agent("bob"), target_agent("alice")]
    );

    let unclosed = resolve(
        "` text @alice",
        None,
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(unclosed.len(), 1);

    let double_ticks = resolve(
        "`` @alice `` @bob",
        None,
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(double_ticks, vec![mention(target_agent("bob"), "@bob", 13)]);
}

#[test]
fn recognizes_tilde_fences_and_masks_an_unclosed_fence_to_eof() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert!(
        resolve(
            "~~~\n@alice\n",
            None,
            &MentionAuthor::Other,
            &roster,
            &desks
        )
        .is_empty()
    );

    let body = "```\n```still code\n@alice\n```\n@alice";
    let found = resolve(body, None, &MentionAuthor::Other, &roster, &desks);
    assert_eq!(found, vec![mention(target_agent("alice"), "@alice", 29)]);
}

#[test]
fn supplied_empty_is_authoritative_and_malformed_or_code_spans_are_dropped() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert!(
        resolve(
            "@alice",
            Some(vec![]),
            &MentionAuthor::Other,
            &roster,
            &desks
        )
        .is_empty()
    );

    let malformed = vec![
        mention(target_agent("alice"), "@alice", 99),
        mention(target_agent("alice"), "@alice", 1),
        mention(target_agent("alice"), "@alice", 1),
    ];
    assert!(
        resolve(
            "`@alice`",
            Some(malformed),
            &MentionAuthor::Other,
            &roster,
            &desks
        )
        .is_empty()
    );
}

#[test]
fn supplied_mentions_must_match_one_exact_mention_token() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let body = "@alice blah";

    let found = resolve(
        body,
        Some(vec![mention(target_agent("alice"), body, 0)]),
        &MentionAuthor::Other,
        &roster,
        &desks,
    );

    assert!(found.is_empty());
}

#[test]
fn supplied_stale_or_wrong_alias_targets_survive_as_quiet_context() {
    let members = [member("alice", None), member("bob", None)];
    let retired = [String::from("old")];
    let roster = Roster::new(&members, &[], &retired);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let supplied = vec![
        mention(target_agent("old"), "@old", 0),
        mention(target_agent("ghost"), "@alice", 5),
        mention(target_agent("bob"), "@bob", 12),
    ];
    let found = resolve(
        "@old @alice @bob",
        Some(supplied),
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(found.len(), 3);
    assert!(found[0].quiet);
    assert!(found[1].quiet);
    assert!(!found[2].quiet);
}

#[test]
fn normalizes_order_offsets_self_repeats_and_ping_cap() {
    let members: Vec<_> = (0..52)
        .map(|index| member(&format!("a{index}"), None))
        .collect();
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let body = (0..52)
        .map(|index| format!("@a{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let found = resolve(
        &body,
        None,
        &MentionAuthor::Agent { id: "a0".into() },
        &roster,
        &desks,
    );
    assert_eq!(found.len(), 51);
    assert_eq!(found.iter().filter(|item| !item.quiet).count(), 50);
    assert!(found.last().unwrap().quiet);

    let repeated = resolve("@a1 @a1", None, &MentionAuthor::Other, &roster, &desks);
    assert!(!repeated[0].quiet);
    assert!(repeated[1].quiet);

    let supplied = vec![
        mention(target_agent("a2"), "@a2", 4),
        mention(target_agent("a1"), "@a1", 0),
        mention(target_agent("a2"), "@a1", 0),
    ];
    let ordered = resolve(
        "@a1 @a2",
        Some(supplied),
        &MentionAuthor::Other,
        &roster,
        &desks,
    );
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[0].target, target_agent("a1"));
}

#[test]
fn direct_responder_uses_only_first_nonquiet_active_agent() {
    let members = [member("alice", None), member("bob", None)];
    let retired = [String::from("alice")];
    let roster = Roster::new(&members, &[], &retired);
    let mentions = [
        mention(MentionTarget::Everyone, "@everyone", 0),
        mention(target_agent("alice"), "@alice", 10),
        Mention {
            quiet: true,
            ..mention(target_agent("bob"), "@bob", 17)
        },
        mention(target_agent("bob"), "@bob", 22),
    ];
    assert_eq!(direct_responder(&mentions, &roster), Some("bob"));
}

#[test]
fn direct_responder_fails_closed_for_an_invalid_roster() {
    let blank_members = [member("", None)];
    let blank_roster = Roster::new(&blank_members, &[], &[]);
    let duplicate_members = [member("alice", None), member("alice", Some("Other"))];
    let duplicate_roster = Roster::new(&duplicate_members, &[], &[]);
    let mentions = [mention(target_agent("alice"), "@alice", 0)];

    assert_eq!(direct_responder(&mentions, &blank_roster), None);
    assert_eq!(direct_responder(&mentions, &duplicate_roster), None);
}

#[test]
fn expands_context_without_fanout_deduplicates_and_excludes_responder() {
    let members = [
        member("alice", None),
        member("bob", None),
        member("cara", None),
        member("dave", None),
    ];
    let retired = [String::from("dave")];
    let desk_records = [desk("eng", "Engineering", &["alice", "bob", "dave"])];
    let roster = Roster::new(&members, &[], &retired);
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &retired);
    let mentions = [
        mention(target_agent("cara"), "@cara", 0),
        mention(MentionTarget::Desk { id: "eng".into() }, "@#eng", 6),
        mention(MentionTarget::Everyone, "@everyone", 12),
        mention(MentionTarget::Person { id: "p".into() }, "@person", 22),
    ];
    assert_eq!(
        mentioned_members(&mentions, Some("eng"), Some("alice"), &roster, &desks),
        vec!["cara", "bob"]
    );
    assert_eq!(
        mentioned_members(
            &[mention(MentionTarget::Everyone, "@everyone", 0)],
            None,
            Some("alice"),
            &roster,
            &desks
        ),
        vec!["bob", "cara"]
    );
    assert!(
        mentioned_members(
            &[mention(MentionTarget::Everyone, "@everyone", 0)],
            Some("missing"),
            None,
            &roster,
            &desks
        )
        .is_empty()
    );
}
