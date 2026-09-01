//! Pure selection of at most one bounded agent turn that may cross a channel.
//!
//! [`crate::dispatch::mention_dispatch`] answers *who replies next*, and binds
//! that reply to the conversation the trigger was committed on. This module
//! answers the question a second desk makes possible: *whose channel does the
//! reply run in* — and, once it has run there, *how does the answer come back*.
//!
//! Everything here is still one message, one turn. A desk mention resolves to
//! exactly one agent before it leaves the fold, and there is no decision
//! variant carrying two.
//!
//! # Example
//!
//! ```
//! use tinyhivemind_core::{
//!     desk::{Desk, DeskSet, ResponderMode},
//!     dispatch::{DispatchConversation, DispatchKey},
//!     mention::{Mention, MentionTarget},
//!     referral::{referral, ReferralDecision, ReferralInput, ReferralPolicy, ReferralReach},
//!     roster::{Roster, RosterMember},
//! };
//!
//! let members = [member("ada"), member("linus")];
//! let roster = Roster::new(&members, &[], &[]);
//! let desks = [
//!     desk("payments", &["ada"]),
//!     desk("platform", &["linus"]),
//! ];
//! let desks = DeskSet::new(&desks, &[], &[], &[], &[]);
//!
//! // Ada, on the payments desk, asks a platform engineer who is not here.
//! let input = ReferralInput {
//!     key: DispatchKey { trigger_sequence: 7 },
//!     conversation: DispatchConversation { desk_id: "payments".into(), thread_root: None },
//!     author_id: "ada".into(),
//!     content: "@linus does the gateway retry on 503?".into(),
//!     mentions: vec![Mention {
//!         target: MentionTarget::Agent { id: "linus".into() },
//!         text: "@linus".into(),
//!         offset: 0,
//!         quiet: false,
//!     }],
//!     hop: 0,
//!     origin: None,
//! };
//! let policy = ReferralPolicy {
//!     enabled: true,
//!     max_hops: 2,
//!     reach: ReferralReach::Channels,
//!     returns: true,
//!     ..ReferralPolicy::DEFAULT
//! };
//!
//! let ReferralDecision::One { referral } = referral(policy, &input, &roster, &desks)? else {
//!     panic!("a crossing referral was available");
//! };
//! // The turn runs on Linus's own desk, not by dragging him into payments.
//! assert!(referral.crosses());
//! assert_eq!(referral.to.desk_id, "platform");
//! // And it remembers where the answer has to go back to.
//! assert_eq!(referral.origin.as_ref().map(|o| o.asker_id.as_str()), Some("ada"));
//!
//! fn member(id: &str) -> RosterMember {
//!     RosterMember { id: id.into(), name: Some(id.into()) }
//! }
//! fn desk(id: &str, members: &[&str]) -> Desk {
//!     Desk {
//!         id: id.into(),
//!         name: id.into(),
//!         description: None,
//!         members: members.iter().map(|m| (*m).to_owned()).collect(),
//!         responder_mode: ResponderMode::Lead,
//!     }
//! }
//! # Ok::<(), tinyhivemind_core::error::Error>(())
//! ```

#[cfg(test)]
mod test;

mod types;

pub use types::{
    NoReferralReason, Referral, ReferralDecision, ReferralInput, ReferralKind, ReferralOrigin,
    ReferralPolicy, ReferralReach,
};

use crate::{
    chat::is_general_chat,
    desk::DeskSet,
    dispatch::DispatchConversation,
    error::Result,
    mention::{Mention, MentionTarget},
    roster::Roster,
};

/// Decide whether one committed agent reply may start one child turn, and
/// which conversation that turn runs on.
///
/// Evaluation is fail-closed and ordered: a disabled policy, an exhausted hop
/// budget, an inactive author, then the first reading-order nonquiet candidate.
/// Once that candidate is found, a self, inactive, deskless or empty-desk
/// target stops the decision; a later mention is never used as a fallback. A
/// return is considered only when there is no forward candidate at all.
///
/// # Errors
///
/// Returns a typed core error when the supplied roster or desk snapshot is
/// malformed.
pub fn referral(
    policy: ReferralPolicy,
    input: &ReferralInput,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ReferralDecision> {
    let none = |reason| Ok(ReferralDecision::None { reason });
    if !policy.enabled {
        return none(NoReferralReason::Disabled);
    }
    if input.hop >= policy.max_hops {
        return none(NoReferralReason::HopLimitReached);
    }
    roster.validate()?;
    desks.validate()?;
    if roster.active_member(&input.author_id).is_none() {
        return none(NoReferralReason::SourceInactive);
    }
    let Some(child_hop) = input.hop.checked_add(1) else {
        return none(NoReferralReason::HopOverflow);
    };

    match candidate(&input.mentions, policy) {
        Some(MentionTarget::Agent { id }) => {
            forward_to_agent(policy, input, roster, desks, &id, child_hop)
        }
        Some(MentionTarget::Desk { id }) => {
            forward_to_desk(policy, input, roster, desks, &id, child_hop)
        }
        // `candidate` only ever yields the two kinds above.
        Some(MentionTarget::Person { .. } | MentionTarget::Everyone) | None => {
            back(policy, input, roster, child_hop)
        }
    }
}

/// The lowest-offset nonquiet mention this policy may act on.
fn candidate(mentions: &[Mention], policy: ReferralPolicy) -> Option<MentionTarget> {
    mentions
        .iter()
        .filter(|mention| !mention.quiet)
        .filter(|mention| match &mention.target {
            MentionTarget::Agent { .. } => true,
            MentionTarget::Desk { .. } => policy.reach.addresses_desks(),
            MentionTarget::Person { .. } | MentionTarget::Everyone => false,
        })
        .min_by_key(|mention| mention.offset)
        .map(|mention| mention.target.clone())
}

/// Resolve a direct agent mention to a conversation.
fn forward_to_agent(
    policy: ReferralPolicy,
    input: &ReferralInput,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    target_id: &str,
    child_hop: u32,
) -> Result<ReferralDecision> {
    let none = |reason| Ok(ReferralDecision::None { reason });
    if target_id == input.author_id {
        return none(NoReferralReason::SelfMention);
    }
    if roster.active_member(target_id).is_none() {
        return none(NoReferralReason::TargetInactive);
    }
    // Without `cross_desk` the child turn stays exactly where
    // `mention_dispatch` puts it, whether or not the target sits on this desk.
    if !policy.reach.crosses() || present(desks, roster, &input.conversation.desk_id, target_id) {
        return Ok(one(
            input,
            target_id,
            input.conversation.clone(),
            None,
            child_hop,
        ));
    }
    let Some(home) = home_desk(desks, target_id) else {
        return none(NoReferralReason::TargetDeskless);
    };
    Ok(one(
        input,
        target_id,
        DispatchConversation {
            desk_id: home.to_owned(),
            thread_root: None,
        },
        origin(policy, input),
        child_hop,
    ))
}

/// Resolve a desk mention to that desk's one responder.
fn forward_to_desk(
    policy: ReferralPolicy,
    input: &ReferralInput,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    desk_identity: &str,
    child_hop: u32,
) -> Result<ReferralDecision> {
    let none = |reason| Ok(ReferralDecision::None { reason });
    let Ok(desk_id) = desks.resolve_id(desk_identity) else {
        return none(NoReferralReason::UnknownDesk);
    };
    if same_desk(&input.conversation.desk_id, desk_id) {
        return none(NoReferralReason::SelfDesk);
    }
    let Ok(members) = desks.members(desk_id) else {
        return none(NoReferralReason::UnknownDesk);
    };
    let target = members
        .into_iter()
        .find(|id| *id != input.author_id && roster.active_member(id).is_some());
    let Some(target_id) = target else {
        return none(NoReferralReason::EmptyDesk);
    };
    Ok(one(
        input,
        target_id,
        DispatchConversation {
            desk_id: desk_id.to_owned(),
            thread_root: None,
        },
        origin(policy, input),
        child_hop,
    ))
}

/// Carry the one answer back to the conversation that asked.
fn back(
    policy: ReferralPolicy,
    input: &ReferralInput,
    roster: &Roster<'_>,
    child_hop: u32,
) -> Result<ReferralDecision> {
    let none = |reason| Ok(ReferralDecision::None { reason });
    if !policy.returns {
        return none(NoReferralReason::NoReferralTarget);
    }
    let Some(origin) = input.origin.as_ref() else {
        return none(NoReferralReason::NoReferralTarget);
    };
    // A reply committed on the conversation that asked has already answered it.
    if origin.conversation == input.conversation {
        return none(NoReferralReason::NoReferralTarget);
    }
    if origin.asker_id == input.author_id {
        return none(NoReferralReason::SelfMention);
    }
    if roster.active_member(&origin.asker_id).is_none() {
        return none(NoReferralReason::TargetInactive);
    }
    Ok(ReferralDecision::One {
        referral: Box::new(Referral {
            key: input.key,
            kind: ReferralKind::Return,
            source_id: input.author_id.clone(),
            target_id: origin.asker_id.clone(),
            content: input.content.clone(),
            from: input.conversation.clone(),
            to: origin.conversation.clone(),
            // A return carries no origin, so a round trip is two hops and
            // cannot ring.
            origin: None,
            child_hop,
        }),
    })
}

/// Build one forward referral.
fn one(
    input: &ReferralInput,
    target_id: &str,
    to: DispatchConversation,
    origin: Option<ReferralOrigin>,
    child_hop: u32,
) -> ReferralDecision {
    let crosses = to != input.conversation;
    ReferralDecision::One {
        referral: Box::new(Referral {
            key: input.key,
            kind: ReferralKind::Forward,
            source_id: input.author_id.clone(),
            target_id: target_id.to_owned(),
            content: input.content.clone(),
            from: input.conversation.clone(),
            to,
            // A referral that stays put needs no back edge: the answer is
            // appended to the conversation the asker is already reading.
            origin: if crosses { origin } else { None },
            child_hop,
        }),
    }
}

/// Where an answer to a crossing forward has to be delivered.
fn origin(policy: ReferralPolicy, input: &ReferralInput) -> Option<ReferralOrigin> {
    policy.returns.then(|| ReferralOrigin {
        conversation: input.conversation.clone(),
        asker_id: input.author_id.clone(),
    })
}

/// Is this agent an effective active member of this conversation's desk?
///
/// Every active member is present in General, which has four spellings and no
/// membership list of its own.
fn present(desks: &DeskSet<'_>, roster: &Roster<'_>, desk_id: &str, agent_id: &str) -> bool {
    if is_general_chat(Some(desk_id)) {
        return roster.active_member(agent_id).is_some();
    }
    desks
        .members(desk_id)
        .is_ok_and(|members| members.contains(&agent_id))
}

/// The first desk in snapshot order holding this agent as an effective member.
fn home_desk<'a>(desks: &DeskSet<'a>, agent_id: &str) -> Option<&'a str> {
    desks.iter().find_map(|desk| {
        let members = desks.members(&desk.id).ok()?;
        members.contains(&agent_id).then_some(desk.id.as_str())
    })
}

/// Do these two desk identities name the same conversation?
fn same_desk(left: &str, right: &str) -> bool {
    left == right || (is_general_chat(Some(left)) && is_general_chat(Some(right)))
}
