//! Unit tests for roster and desk name searches.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{agents, agents_matching, desks, desks_matching, people, people_matching};
use crate::{
    desk::{Desk, DeskSet, ResponderMode},
    roster::{Person, Roster, RosterMember},
    select::{MatchField, MatchKind, Pattern},
};

fn members() -> Vec<RosterMember> {
    vec![
        RosterMember {
            id: "alice".into(),
            name: Some("Alice Nakamura".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob Ferrante".into()),
        },
        RosterMember {
            id: "carol".into(),
            name: None,
        },
    ]
}

fn desk_records() -> Vec<Desk> {
    vec![
        Desk {
            id: "engineering".into(),
            name: "Engineering".into(),
            description: Some("Ship the product".into()),
            members: vec!["alice".into()],
            responder_mode: ResponderMode::Lead,
        },
        Desk {
            id: "support".into(),
            name: "Support".into(),
            description: None,
            members: vec!["bob".into()],
            responder_mode: ResponderMode::Lead,
        },
    ]
}

#[test]
fn finds_an_agent_by_display_name() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let hits = agents("naka", &roster, 8);
    assert_eq!(hits[0].id, "alice");
    assert_eq!(hits[0].field, MatchField::Label);
    assert_eq!(hits[0].kind, MatchKind::WordPrefix);
}

#[test]
fn finds_an_agent_with_no_display_name_by_its_id() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let hits = agents("carol", &roster, 8);
    assert_eq!(hits[0].id, "carol");
    assert_eq!(hits[0].label, "carol");
}

#[test]
fn never_offers_a_retired_agent() {
    let members = members();
    let retired = vec!["alice".to_owned()];
    let roster = Roster::new(&members, &[], &retired);
    assert!(agents("naka", &roster, 8).is_empty());
}

#[test]
fn finds_a_person_by_label() {
    let people_records = vec![Person {
        id: "u-1".into(),
        label: "Dana Okoro".into(),
    }];
    let roster = Roster::new(&[], &people_records, &[]);
    let hits = people("okoro", &roster, 8);
    assert_eq!(hits[0].id, "u-1");
}

#[test]
fn finds_a_desk_by_name_and_by_description() {
    let records = desk_records();
    let set = DeskSet::new(&records, &[], &[], &[], &[]);
    let by_name = desks("engin", &set, 8);
    assert_eq!(by_name[0].id, "engineering");
    assert_eq!(by_name[0].field, MatchField::Label);

    let by_description = desks("product", &set, 8);
    assert_eq!(by_description[0].id, "engineering");
    assert_eq!(by_description[0].field, MatchField::Detail);
}

#[test]
fn offers_a_desk_declared_twice_only_once() {
    let declared = desk_records();
    let added = vec![Desk {
        id: "engineering".into(),
        name: "Engineering (ops)".into(),
        description: None,
        members: vec!["bob".into()],
        responder_mode: ResponderMode::Lead,
    }];
    let set = DeskSet::new(&declared, &added, &[], &[], &[]);
    let hits = desks("engineering", &set, 8);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].label, "Engineering");
}

#[test]
fn returns_nothing_for_an_unmatched_query() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let records = desk_records();
    let set = DeskSet::new(&records, &[], &[], &[], &[]);
    assert!(agents("zzzz", &roster, 8).is_empty());
    assert!(people("zzzz", &roster, 8).is_empty());
    assert!(desks("zzzz", &set, 8).is_empty());
}

#[test]
fn accepts_a_pattern_on_every_picker() {
    let members = members();
    let people_records = vec![Person {
        id: "u-1".into(),
        label: "Dana Okoro".into(),
    }];
    let roster = Roster::new(&members, &people_records, &[]);
    let records = desk_records();
    let set = DeskSet::new(&records, &[], &[], &[], &[]);
    assert_eq!(
        agents_matching(&Pattern::Text("bob"), &roster, 8)[0].id,
        "bob"
    );
    assert_eq!(
        people_matching(&Pattern::Text("dana"), &roster, 8)[0].id,
        "u-1"
    );
    assert_eq!(
        desks_matching(&Pattern::Text("support"), &set, 8)[0].id,
        "support"
    );
}

#[cfg(feature = "regex")]
#[test]
fn finds_agents_by_a_compiled_expression() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let expression = regex::Regex::new("(?i)^(alice|bob)$").expect("compiles");
    let hits = agents_matching(&Pattern::Regex(&expression), &roster, 8);
    assert_eq!(hits.len(), 2);
}
