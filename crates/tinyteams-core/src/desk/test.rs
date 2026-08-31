//! Unit tests for desk DTOs, validation, and membership projection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Serialize, Serializer as _};

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
fn desk_wire_fields_are_stable_snake_case() {
    assert_eq!(
        field_names(&desk("engineering", "Engineering", &["alice"])),
        ["id", "name", "description", "members"]
    );
}

#[test]
fn desk_member_wire_fields_are_stable_snake_case() {
    assert_eq!(
        field_names(&DeskMember {
            desk_id: "engineering".into(),
            agent_id: "bob".into(),
        }),
        ["desk_id", "agent_id"]
    );
}

#[test]
fn desk_order_wire_fields_are_stable_snake_case() {
    assert_eq!(
        field_names(&DeskOrder {
            desk_id: "engineering".into(),
            ordered: vec!["bob".into(), "alice".into()],
        }),
        ["desk_id", "ordered"]
    );
}

#[test]
fn desk_wire_types_are_deserializable() {
    fn assert_deserialize<T: for<'de> serde::Deserialize<'de>>() {}

    assert_deserialize::<Desk>();
    assert_deserialize::<DeskMember>();
    assert_deserialize::<DeskOrder>();
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

#[derive(Debug)]
struct WireError;

impl std::fmt::Display for WireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("wire test error")
    }
}

impl std::error::Error for WireError {}

impl serde::ser::Error for WireError {
    fn custom<T: std::fmt::Display>(_message: T) -> Self {
        Self
    }
}

fn field_names(value: &impl Serialize) -> Vec<&'static str> {
    value.serialize(FieldNameSerializer).unwrap()
}

/// The field recorder intentionally supports structs only. Exercising every
/// rejection keeps this test utility honest and prevents a serde shape change
/// from being mistaken for a successful wire-field assertion.
#[test]
fn wire_field_recorder_rejects_every_non_struct_shape() {
    assert!(FieldNameSerializer.serialize_bool(true).is_err());
    assert!(FieldNameSerializer.serialize_i8(1).is_err());
    assert!(FieldNameSerializer.serialize_i16(1).is_err());
    assert!(FieldNameSerializer.serialize_i32(1).is_err());
    assert!(FieldNameSerializer.serialize_i64(1).is_err());
    assert!(FieldNameSerializer.serialize_u8(1).is_err());
    assert!(FieldNameSerializer.serialize_u16(1).is_err());
    assert!(FieldNameSerializer.serialize_u32(1).is_err());
    assert!(FieldNameSerializer.serialize_u64(1).is_err());
    assert!(FieldNameSerializer.serialize_f32(1.0).is_err());
    assert!(FieldNameSerializer.serialize_f64(1.0).is_err());
    assert!(FieldNameSerializer.serialize_char('x').is_err());
    assert!(FieldNameSerializer.serialize_str("x").is_err());
    assert!(FieldNameSerializer.serialize_bytes(b"x").is_err());
    assert!(FieldNameSerializer.serialize_none().is_err());
    assert!(FieldNameSerializer.serialize_some(&"x").is_err());
    assert!(FieldNameSerializer.serialize_unit().is_err());
    assert!(FieldNameSerializer.serialize_unit_struct("Unit").is_err());
    assert!(
        FieldNameSerializer
            .serialize_unit_variant("Enum", 0, "Unit")
            .is_err()
    );
    assert!(
        FieldNameSerializer
            .serialize_newtype_struct("Newtype", &"x")
            .is_err()
    );
    assert!(
        FieldNameSerializer
            .serialize_newtype_variant("Enum", 0, "Newtype", &"x")
            .is_err()
    );
    assert!(FieldNameSerializer.serialize_seq(Some(1)).is_err());
    assert!(FieldNameSerializer.serialize_tuple(1).is_err());
    assert!(
        FieldNameSerializer
            .serialize_tuple_struct("Tuple", 1)
            .is_err()
    );
    assert!(
        FieldNameSerializer
            .serialize_tuple_variant("Enum", 0, "Tuple", 1)
            .is_err()
    );
    assert!(FieldNameSerializer.serialize_map(Some(1)).is_err());
    assert!(
        FieldNameSerializer
            .serialize_struct_variant("Enum", 0, "Struct", 1)
            .is_err()
    );
}

struct FieldNameSerializer;

struct StructFields(Vec<&'static str>);

impl serde::ser::SerializeStruct for StructFields {
    type Ok = Vec<&'static str>;
    type Error = WireError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        _value: &T,
    ) -> Result<(), Self::Error> {
        self.0.push(key);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(self.0)
    }
}

macro_rules! unsupported {
    ($($name:ident($($arg:ident: $type:ty),*);)*) => {$ (
        fn $name(self, $($arg: $type),*) -> Result<Self::Ok, Self::Error> {
            $(let _ = $arg;)*
            Err(WireError)
        }
    )*};
}

impl serde::Serializer for FieldNameSerializer {
    type Ok = Vec<&'static str>;
    type Error = WireError;
    type SerializeSeq = serde::ser::Impossible<Self::Ok, Self::Error>;
    type SerializeTuple = serde::ser::Impossible<Self::Ok, Self::Error>;
    type SerializeTupleStruct = serde::ser::Impossible<Self::Ok, Self::Error>;
    type SerializeTupleVariant = serde::ser::Impossible<Self::Ok, Self::Error>;
    type SerializeMap = serde::ser::Impossible<Self::Ok, Self::Error>;
    type SerializeStruct = StructFields;
    type SerializeStructVariant = serde::ser::Impossible<Self::Ok, Self::Error>;

    unsupported! {
        serialize_bool(value: bool);
        serialize_i8(value: i8);
        serialize_i16(value: i16);
        serialize_i32(value: i32);
        serialize_i64(value: i64);
        serialize_u8(value: u8);
        serialize_u16(value: u16);
        serialize_u32(value: u32);
        serialize_u64(value: u64);
        serialize_f32(value: f32);
        serialize_f64(value: f64);
        serialize_char(value: char);
        serialize_str(value: &str);
        serialize_bytes(value: &[u8]);
        serialize_unit_struct(name: &'static str);
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(WireError)
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(WireError)
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(WireError)
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(WireError)
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(WireError)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(WireError)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(StructFields(Vec::with_capacity(len)))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(WireError)
    }
}
