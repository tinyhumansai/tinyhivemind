//! Unit tests for roster structure and payload representation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Person, Roster, RosterMember};
use crate::error::Error;
use serde_json::json;

#[test]
fn pins_roster_payload_wire_shapes() {
    assert_eq!(
        serde_json::to_value(RosterMember {
            id: "alice".into(),
            name: None,
        })
        .unwrap(),
        json!({"id": "alice", "name": null})
    );
    assert_eq!(
        serde_json::to_value(Person {
            id: "p1".into(),
            label: "Ada".into(),
        })
        .unwrap(),
        json!({"id": "p1", "label": "Ada"})
    );
}

#[test]
fn roster_member_name_is_required_but_accepts_null() {
    assert!(serde_json::from_value::<RosterMember>(json!({"id": "alice"})).is_err());
    assert_eq!(
        serde_json::from_value::<RosterMember>(json!({"id": "alice", "name": null})).unwrap(),
        RosterMember {
            id: "alice".into(),
            name: None,
        }
    );
}

#[test]
fn validates_ids_per_namespace_but_allows_alias_collisions() {
    let members = [
        RosterMember {
            id: "same".into(),
            name: Some("Shared".into()),
        },
        RosterMember {
            id: "other".into(),
            name: Some("Shared".into()),
        },
    ];
    let people = [Person {
        id: "same".into(),
        label: "Shared".into(),
    }];
    assert_eq!(Roster::new(&members, &people, &[]).validate(), Ok(()));
}

#[test]
fn rejects_blank_and_duplicate_ids() {
    let blank = [RosterMember {
        id: "  ".into(),
        name: None,
    }];
    assert_eq!(
        Roster::new(&blank, &[], &[]).validate(),
        Err(Error::EmptyRosterMemberId)
    );

    let people = [
        Person {
            id: "p".into(),
            label: "One".into(),
        },
        Person {
            id: "p".into(),
            label: "Two".into(),
        },
    ];
    assert_eq!(
        Roster::new(&[], &people, &[]).validate(),
        Err(Error::DuplicatePersonId {
            person_id: "p".into()
        })
    );

    let members = [member_for_test("a"), member_for_test("a")];
    assert_eq!(
        Roster::new(&members, &[], &[]).validate(),
        Err(Error::DuplicateRosterMemberId {
            member_id: "a".into()
        })
    );

    let blank_person = [Person {
        id: "\t".into(),
        label: String::new(),
    }];
    assert_eq!(
        Roster::new(&[], &blank_person, &[]).validate(),
        Err(Error::EmptyPersonId)
    );
}

fn member_for_test(id: &str) -> RosterMember {
    RosterMember {
        id: id.into(),
        name: None,
    }
}

#[test]
fn active_lookup_excludes_only_exact_retired_ids() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: None,
        },
        RosterMember {
            id: "Alice".into(),
            name: None,
        },
    ];
    let retired = [String::from("alice")];
    let roster = Roster::new(&members, &[], &retired);
    assert_eq!(
        roster
            .active_members()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Alice"]
    );
}
