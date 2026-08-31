//! Agent and person identities visible in one shared conversation.

#[cfg(test)]
mod test;

mod types;

pub use types::{Person, RosterMember};

use crate::error::{Error, Result};

/// A borrowed view of current agent and person identities.
#[derive(Debug)]
pub struct Roster<'a> {
    members: &'a [RosterMember],
    people: &'a [Person],
    retired_member_ids: &'a [String],
}

impl<'a> Roster<'a> {
    /// Borrow the snapshots that define the current roster.
    #[must_use]
    pub const fn new(
        members: &'a [RosterMember],
        people: &'a [Person],
        retired_member_ids: &'a [String],
    ) -> Self {
        Self {
            members,
            people,
            retired_member_ids,
        }
    }

    /// Validate structural identity invariants.
    ///
    /// Agent and person ids occupy independent namespaces. Display aliases are
    /// deliberately allowed to collide and fail closed during mention lookup.
    ///
    /// # Errors
    ///
    /// Returns a typed error for the first blank or duplicate id.
    pub fn validate(&self) -> Result<()> {
        let mut member_ids = Vec::new();
        for member in self.members {
            if member.id.trim().is_empty() {
                return Err(Error::EmptyRosterMemberId);
            }
            if member_ids.contains(&member.id.as_str()) {
                return Err(Error::DuplicateRosterMemberId {
                    member_id: member.id.clone(),
                });
            }
            member_ids.push(member.id.as_str());
        }

        let mut person_ids = Vec::new();
        for person in self.people {
            if person.id.trim().is_empty() {
                return Err(Error::EmptyPersonId);
            }
            if person_ids.contains(&person.id.as_str()) {
                return Err(Error::DuplicatePersonId {
                    person_id: person.id.clone(),
                });
            }
            person_ids.push(person.id.as_str());
        }
        Ok(())
    }

    /// Iterate active agents in roster order.
    pub fn active_members(&self) -> impl Iterator<Item = &'a RosterMember> + '_ {
        self.members
            .iter()
            .filter(|member| !self.is_retired(&member.id))
    }

    /// Find an active agent by its exact id.
    #[must_use]
    pub fn active_member(&self, id: &str) -> Option<&'a RosterMember> {
        self.active_members().find(|member| member.id == id)
    }

    /// Find a person by its exact id.
    #[must_use]
    pub fn person(&self, id: &str) -> Option<&'a Person> {
        self.people.iter().find(|person| person.id == id)
    }

    /// Iterate people in roster order.
    pub fn people(&self) -> impl Iterator<Item = &'a Person> + '_ {
        self.people.iter()
    }

    /// Report whether an agent id is retired.
    #[must_use]
    pub fn is_retired(&self, id: &str) -> bool {
        self.retired_member_ids.iter().any(|retired| retired == id)
    }
}
