//! Typed failures from validating or querying borrowed collaboration data.

#[cfg(test)]
mod test;

/// A typed failure from validating or querying collaboration data.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// A desk has no id.
    #[error("desk id must not be empty")]
    EmptyDeskId,
    /// A desk has no display name.
    #[error("desk `{desk_id}` name must not be empty")]
    EmptyDeskName {
        /// The id of the desk with no name.
        desk_id: String,
    },
    /// More than one desk has the same exact id.
    #[error("duplicate desk id `{desk_id}`")]
    DuplicateDeskId {
        /// The duplicated desk id.
        desk_id: String,
    },
    /// A non-default desk collides with a reserved General spelling.
    #[error("reserved desk identity `{identity}`")]
    ReservedDeskIdentity {
        /// The colliding id or name.
        identity: String,
    },
    /// More than one desk has the requested exact name.
    #[error("ambiguous desk `{identity}`")]
    AmbiguousDesk {
        /// The ambiguous name.
        identity: String,
    },
    /// No desk has the requested exact id or name.
    #[error("unknown desk `{identity}`")]
    UnknownDesk {
        /// The unresolved id or name.
        identity: String,
    },
    /// A membership addition targets an unknown exact desk id.
    #[error("member addition targets unknown desk `{desk_id}`")]
    UnknownMemberDesk {
        /// The unknown target id.
        desk_id: String,
    },
    /// A member order targets an unknown exact desk id.
    #[error("member order targets unknown desk `{desk_id}`")]
    UnknownOrderDesk {
        /// The unknown target id.
        desk_id: String,
    },
    /// More than one member order targets a desk.
    #[error("duplicate member order for desk `{desk_id}`")]
    DuplicateDeskOrder {
        /// The multiply ordered desk id.
        desk_id: String,
    },
    /// An order repeats a member id.
    #[error("duplicate member `{agent_id}` in order for desk `{desk_id}`")]
    DuplicateOrderMember {
        /// The ordered desk id.
        desk_id: String,
        /// The repeated agent id.
        agent_id: String,
    },
    /// An order names an agent outside the desk's final member set.
    #[error("unknown member `{agent_id}` in order for desk `{desk_id}`")]
    UnknownOrderMember {
        /// The ordered desk id.
        desk_id: String,
        /// The unknown agent id.
        agent_id: String,
    },
    /// An order omits a member from the desk's final member set.
    #[error("order for desk `{desk_id}` is missing member `{missing_agent_id}`")]
    IncompleteOrder {
        /// The ordered desk id.
        desk_id: String,
        /// The first omitted agent id in final-member order.
        missing_agent_id: String,
    },
}

/// The crate-wide result type for fallible collaboration algebra.
pub type Result<T> = std::result::Result<T, Error>;
