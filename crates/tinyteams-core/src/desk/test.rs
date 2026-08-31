//! Unit tests for desk DTOs, validation, and membership projection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::{Desk, DeskMember, DeskOrder, DeskSet};
use crate::error::Error;

fn desk(id: &str, name: &str, members: &[&str]) -> Desk {
    Desk {
        id: id.into(),
        name: name.into(),
        description: None,
        members: members.iter().map(|member| (*member).into()).collect(),
    }
}

fn set<'a>(
    declared: &'a [Desk],
    added: &'a [Desk],
    member_additions: &'a [DeskMember],
    orders: &'a [DeskOrder],
    retired: &'a [String],
) -> DeskSet<'a> {
    DeskSet::new(declared, added, member_additions, orders, retired)
}

#[test]
fn desk_wire_form_is_exact_and_round_trips() {
    let desk = Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: Some("Builds the product".into()),
        members: vec!["alice".into(), "bob".into()],
    };
    let expected = json!({
        "id": "engineering",
        "name": "Engineering",
        "description": "Builds the product",
        "members": ["alice", "bob"]
    });

    assert_eq!(serde_json::to_value(&desk).unwrap(), expected);
    assert_eq!(serde_json::from_value::<Desk>(expected).unwrap(), desk);
}

#[test]
fn desk_description_accepts_explicit_null() {
    let value = json!({
        "id": "engineering",
        "name": "Engineering",
        "description": null,
        "members": ["alice"]
    });

    assert_eq!(
        serde_json::from_value::<Desk>(value).unwrap(),
        desk("engineering", "Engineering", &["alice"])
    );
}

#[test]
fn desk_member_wire_form_is_exact_and_round_trips() {
    let member = DeskMember {
        desk_id: "engineering".into(),
        agent_id: "bob".into(),
    };
    let expected = json!({
        "desk_id": "engineering",
        "agent_id": "bob"
    });

    assert_eq!(serde_json::to_value(&member).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<DeskMember>(expected).unwrap(),
        member
    );
}

#[test]
fn desk_order_wire_form_is_exact_and_round_trips() {
    let order = DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["bob".into(), "alice".into()],
    };
    let expected = json!({
        "desk_id": "engineering",
        "ordered": ["bob", "alice"]
    });

    assert_eq!(serde_json::to_value(&order).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<DeskOrder>(expected).unwrap(),
        order
    );
}

#[test]
fn desk_wire_fields_are_required() {
    for missing in ["id", "name", "description", "members"] {
        let mut value = json!({
            "id": "engineering",
            "name": "Engineering",
            "description": null,
            "members": ["alice"]
        });
        value.as_object_mut().unwrap().remove(missing);
        assert_missing_field::<Desk>(value, missing);
    }
}

#[test]
fn desk_member_wire_fields_are_required() {
    for missing in ["desk_id", "agent_id"] {
        let mut value = json!({
            "desk_id": "engineering",
            "agent_id": "alice"
        });
        value.as_object_mut().unwrap().remove(missing);
        assert_missing_field::<DeskMember>(value, missing);
    }
}

#[test]
fn desk_order_wire_fields_are_required() {
    for missing in ["desk_id", "ordered"] {
        let mut value = json!({
            "desk_id": "engineering",
            "ordered": ["alice"]
        });
        value.as_object_mut().unwrap().remove(missing);
        assert_missing_field::<DeskOrder>(value, missing);
    }
}

fn assert_missing_field<T>(value: Value, field: &str)
where
    T: for<'de> serde::Deserialize<'de> + std::fmt::Debug,
{
    let error = serde_json::from_value::<T>(value).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("missing field `{field}`")),
        "unexpected error for missing `{field}`: {error}"
    );
}

#[test]
fn rejects_an_empty_desk_id() {
    let declared = [desk("", "Engineering", &[])];
    assert_eq!(
        set(&declared, &[], &[], &[], &[]).validate(),
        Err(Error::EmptyDeskId)
    );
}

#[test]
fn rejects_an_empty_desk_name() {
    let declared = [desk("engineering", "", &[])];
    assert_eq!(
        set(&declared, &[], &[], &[], &[]).validate(),
        Err(Error::EmptyDeskName {
            desk_id: "engineering".into()
        })
    );
}

#[test]
fn rejects_a_duplicate_desk_id_across_declared_and_added_desks() {
    let declared = [desk("engineering", "Engineering", &[])];
    let added = [desk("engineering", "Delivery", &[])];
    assert_eq!(
        set(&declared, &added, &[], &[], &[]).validate(),
        Err(Error::DuplicateDeskId {
            desk_id: "engineering".into()
        })
    );
}

#[test]
fn named_desk_ids_remain_case_sensitive() {
    let declared = [desk("engineering", "Lower", &[])];
    let added = [desk("Engineering", "Upper", &[])];
    assert!(set(&declared, &added, &[], &[], &[]).validate().is_ok());
}

#[test]
fn accepts_the_canonical_general_desk() {
    let declared = [desk("General", "General", &["orchestrator"])];
    assert!(set(&declared, &[], &[], &[], &[]).validate().is_ok());
}

#[test]
fn rejects_general_spellings_on_non_default_desks() {
    for (id, name, identity) in [
        ("general", "Elsewhere", "general"),
        ("MAIN", "Elsewhere", "MAIN"),
        ("elsewhere", "GENERAL", "GENERAL"),
        ("elsewhere", "Main", "Main"),
    ] {
        let declared = [desk(id, name, &[])];
        assert_eq!(
            set(&declared, &[], &[], &[], &[]).validate(),
            Err(Error::ReservedDeskIdentity {
                identity: identity.into()
            })
        );
    }
}

#[test]
fn resolves_an_exact_id_before_a_matching_name() {
    let declared = [
        desk("engineering", "Engineering", &[]),
        desk("delivery", "engineering", &[]),
    ];
    assert_eq!(
        set(&declared, &[], &[], &[], &[]).resolve_id("engineering"),
        Ok("engineering")
    );
}

#[test]
fn resolves_an_exact_name_to_its_id() {
    let declared = [desk("engineering", "Engineering", &[])];
    assert_eq!(
        set(&declared, &[], &[], &[], &[]).resolve_id("Engineering"),
        Ok("engineering")
    );
}

#[test]
fn reports_an_ambiguous_exact_name() {
    let declared = [
        desk("engineering", "Product", &[]),
        desk("delivery", "Product", &[]),
    ];
    assert_eq!(
        set(&declared, &[], &[], &[], &[]).resolve_id("Product"),
        Err(Error::AmbiguousDesk {
            identity: "Product".into()
        })
    );
}

#[test]
fn reports_an_unknown_identity_and_contains_only_resolvable_desks() {
    let declared = [desk("engineering", "Engineering", &[])];
    let desks = set(&declared, &[], &[], &[], &[]);
    assert!(desks.contains("engineering"));
    assert!(desks.contains("Engineering"));
    assert!(!desks.contains("ENGINEERING"));
    assert_eq!(
        desks.resolve_id("unknown"),
        Err(Error::UnknownDesk {
            identity: "unknown".into()
        })
    );
}

#[test]
fn merges_founding_then_added_members_and_deduplicates_first_appearance() {
    let declared = [desk(
        "engineering",
        "Engineering",
        &["alice", "bob", "alice"],
    )];
    let additions = [
        DeskMember {
            desk_id: "engineering".into(),
            agent_id: "bob".into(),
        },
        DeskMember {
            desk_id: "engineering".into(),
            agent_id: "cara".into(),
        },
    ];
    assert_eq!(
        set(&declared, &[], &additions, &[], &[]).members("engineering"),
        Ok(vec!["alice", "bob", "cara"])
    );
}

#[test]
fn removes_retired_members_by_exact_id() {
    let declared = [desk(
        "engineering",
        "Engineering",
        &["alice", "Alice", "bob"],
    )];
    let retired = [String::from("alice")];
    assert_eq!(
        set(&declared, &[], &[], &[], &retired).members("engineering"),
        Ok(vec!["Alice", "bob"])
    );
}

#[test]
fn rejects_a_member_addition_for_an_unknown_desk_id() {
    let additions = [DeskMember {
        desk_id: "unknown".into(),
        agent_id: "alice".into(),
    }];
    assert_eq!(
        set(&[], &[], &additions, &[], &[]).validate(),
        Err(Error::UnknownMemberDesk {
            desk_id: "unknown".into()
        })
    );
}

#[test]
fn lead_is_the_first_final_member_or_none() {
    let declared = [
        desk("engineering", "Engineering", &["alice", "bob"]),
        desk("empty", "Empty", &[]),
    ];
    let retired = [String::from("alice")];
    let desks = set(&declared, &[], &[], &[], &retired);
    assert_eq!(desks.lead("engineering"), Ok(Some("bob")));
    assert_eq!(desks.lead("empty"), Ok(None));
}

#[test]
fn applies_a_complete_member_permutation() {
    let declared = [desk("engineering", "Engineering", &["alice", "bob"])];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["bob".into(), "alice".into()],
    }];
    assert_eq!(
        set(&declared, &[], &[], &orders, &[]).members("engineering"),
        Ok(vec!["bob", "alice"])
    );
}

#[test]
fn rejects_an_order_for_an_unknown_desk_id() {
    let orders = [DeskOrder {
        desk_id: "unknown".into(),
        ordered: vec![],
    }];
    assert_eq!(
        set(&[], &[], &[], &orders, &[]).validate(),
        Err(Error::UnknownOrderDesk {
            desk_id: "unknown".into()
        })
    );
}

#[test]
fn rejects_multiple_orders_for_one_desk() {
    let declared = [desk("engineering", "Engineering", &["alice"])];
    let orders = [
        DeskOrder {
            desk_id: "engineering".into(),
            ordered: vec!["alice".into()],
        },
        DeskOrder {
            desk_id: "engineering".into(),
            ordered: vec!["alice".into()],
        },
    ];
    assert_eq!(
        set(&declared, &[], &[], &orders, &[]).validate(),
        Err(Error::DuplicateDeskOrder {
            desk_id: "engineering".into()
        })
    );
}

#[test]
fn rejects_a_duplicate_member_in_an_order() {
    let declared = [desk("engineering", "Engineering", &["alice", "bob"])];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["alice".into(), "alice".into()],
    }];
    assert_eq!(
        set(&declared, &[], &[], &orders, &[]).validate(),
        Err(Error::DuplicateOrderMember {
            desk_id: "engineering".into(),
            agent_id: "alice".into()
        })
    );
}

#[test]
fn rejects_an_unknown_member_in_an_order() {
    let declared = [desk("engineering", "Engineering", &["alice", "bob"])];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["alice".into(), "cara".into()],
    }];
    assert_eq!(
        set(&declared, &[], &[], &orders, &[]).validate(),
        Err(Error::UnknownOrderMember {
            desk_id: "engineering".into(),
            agent_id: "cara".into()
        })
    );
}

#[test]
fn rejects_an_incomplete_order() {
    let declared = [desk("engineering", "Engineering", &["alice", "bob"])];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["alice".into()],
    }];
    assert_eq!(
        set(&declared, &[], &[], &orders, &[]).validate(),
        Err(Error::IncompleteOrder {
            desk_id: "engineering".into(),
            missing_agent_id: "bob".into()
        })
    );
}
