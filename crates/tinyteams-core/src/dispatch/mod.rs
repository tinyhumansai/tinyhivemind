//! Pure selection of at most one bounded agent-to-agent mention turn.

#[cfg(test)]
mod test;

mod types;

pub use types::{
    DispatchConversation, DispatchKey, MentionDispatchDecision, MentionDispatchInput,
    MentionDispatchPolicy, MentionTurnRequest, NoDispatchReason,
};

use crate::{error::Result, mention::MentionTarget, roster::Roster};

/// Decide whether one committed agent reply may enqueue one mentioned agent.
///
/// Evaluation is deliberately fail-closed. Once the first reading-order,
/// nonquiet direct agent mention is found, a self or inactive target stops the
/// decision; a later mention is never used as fallback.
///
/// # Errors
///
/// Returns a typed core error when the supplied roster snapshot is malformed.
pub fn mention_dispatch(
    policy: MentionDispatchPolicy,
    input: &MentionDispatchInput,
    roster: &Roster<'_>,
) -> Result<MentionDispatchDecision> {
    let none = |reason| MentionDispatchDecision::None { reason };
    if !policy.enabled {
        return Ok(none(NoDispatchReason::Disabled));
    }
    if input.hop >= policy.max_hops {
        return Ok(none(NoDispatchReason::HopLimitReached));
    }
    roster.validate()?;
    if roster.active_member(&input.author_id).is_none() {
        return Ok(none(NoDispatchReason::SourceInactive));
    }

    let target = input
        .mentions
        .iter()
        .filter(|mention| !mention.quiet)
        .filter_map(|mention| match &mention.target {
            MentionTarget::Agent { id } => Some((mention.offset, id.as_str())),
            MentionTarget::Person { .. } | MentionTarget::Desk { .. } | MentionTarget::Everyone => {
                None
            }
        })
        .min_by_key(|(offset, _)| *offset)
        .map(|(_, id)| id);
    let Some(target_id) = target else {
        return Ok(none(NoDispatchReason::NoDirectAgentMention));
    };
    if target_id == input.author_id {
        return Ok(none(NoDispatchReason::SelfMention));
    }
    if roster.active_member(target_id).is_none() {
        return Ok(none(NoDispatchReason::TargetInactive));
    }
    let Some(child_hop) = input.hop.checked_add(1) else {
        return Ok(none(NoDispatchReason::HopOverflow));
    };
    Ok(MentionDispatchDecision::One {
        request: MentionTurnRequest {
            key: input.key,
            source_id: input.author_id.clone(),
            target_id: target_id.to_owned(),
            content: input.content.clone(),
            conversation: input.conversation.clone(),
            child_hop,
        },
    })
}
