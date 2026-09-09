//! The roster and desk snapshots every fold in this host borrows.
//!
//! Two processes need them and neither can hold a borrowed view across its own
//! construction: the desk loop, which commits a turn, and the MCP server, which
//! answers a `desk_dm` while the turn is still running. Owning the three
//! vectors in one place and handing out views off them is what lets both build
//! the same room from the same desk file.

use tinyhivemind::{
    desk::{Desk, DeskSet, ResponderMode},
    roster::{Person, Roster, RosterMember},
};

use crate::deskfile::DeskSpec;

/// One desk file, as the snapshots the library folds over.
pub(crate) struct Room {
    members: Vec<RosterMember>,
    people: Vec<Person>,
    declared: Vec<Desk>,
}

impl Room {
    /// Build the snapshots one desk file describes.
    pub(crate) fn new(spec: &DeskSpec) -> Self {
        Self {
            members: spec
                .agents
                .iter()
                .map(|seat| RosterMember {
                    id: seat.id.clone(),
                    name: Some(seat.label.clone()),
                })
                .collect(),
            people: vec![Person {
                id: spec.person_id.clone(),
                label: spec.person_label.clone(),
            }],
            declared: vec![Desk {
                id: spec.id.clone(),
                name: spec.name.clone(),
                description: None,
                members: spec.agents.iter().map(|seat| seat.id.clone()).collect(),
                responder_mode: ResponderMode::Lead,
            }],
        }
    }

    /// Who is here.
    pub(crate) fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &self.people, &[])
    }

    /// What the desks are.
    pub(crate) fn desks(&self) -> DeskSet<'_> {
        DeskSet::new(&self.declared, &[], &[], &[], &[])
    }
}
