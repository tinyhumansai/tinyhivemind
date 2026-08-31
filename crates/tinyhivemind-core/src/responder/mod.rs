//! Pure, deterministic selection of exactly one responder.

#[cfg(test)]
mod test;

mod types;

pub use types::{
    ResponderDecision, ResponderPlan, ResponderRequest, ResponderRung, SelectionDisposition,
    SelectionPolicy, SelectionRequest, SelectorCandidate,
};

use crate::{
    chat::is_general_chat,
    desk::{Desk, DeskSet, ResponderMode},
    error::{Error, Result},
    mention::direct_responder,
    roster::{Roster, RosterMember},
};

/// Build the pure portion of the responder ladder.
///
/// The result contains exactly one decision or one bounded selector request
/// with a deterministic first-candidate fallback. It never dispatches a turn.
///
/// # Errors
///
/// Returns a structural roster/desk error, a duplicate effective-candidate
/// detail error when selector enrichment is reached, or
/// [`Error::NoActiveResponder`] if a reached fallback has no active agent.
pub fn responder_plan(
    request: &ResponderRequest,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    candidate_details: &[SelectorCandidate],
) -> Result<ResponderPlan> {
    roster.validate()?;
    desks.validate()?;

    if let Some(id) = direct_responder(&request.mentions, roster) {
        return Ok(decided(id, ResponderRung::ExplicitMention));
    }

    if !is_general_chat(request.chat.as_deref()) {
        let chat = request.chat.as_deref().unwrap_or_default();
        match desks.resolve_id(chat) {
            Ok(desk_id) => {
                let desk = desks
                    .iter()
                    .find(|desk| desk.id == desk_id)
                    .ok_or_else(|| Error::UnknownDesk {
                        identity: desk_id.to_owned(),
                    })?;
                return desk_plan(request, roster, desks, desk, candidate_details);
            }
            Err(Error::UnknownDesk { .. }) => {}
            Err(Error::AmbiguousDesk { .. }) => return orchestrator_plan(request, roster),
            Err(error) => return Err(error),
        }

        if let Some(member) = direct_chat_member(chat, roster) {
            return Ok(decided(&member.id, ResponderRung::DirectAgent));
        }
    }

    orchestrator_plan(request, roster)
}

/// Accept a selector response only when it names exactly one candidate id.
///
/// Matching is ASCII-case-insensitive and returns the candidate's canonical id.
/// One trailing period and one matching single-quote, double-quote, or backtick
/// wrapper are tolerated.
#[must_use]
pub fn accept_selection(output: &str, candidates: &[SelectorCandidate]) -> Option<String> {
    let mut value = output.trim();
    if let Some(without_period) = value.strip_suffix('.') {
        value = without_period.trim_end();
    }
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let wrapper = bytes[0];
        if matches!(wrapper, b'\'' | b'"' | b'`') && bytes[value.len() - 1] == wrapper {
            value = value.get(1..value.len() - 1)?.trim();
        }
    }
    if value.is_empty() {
        return None;
    }
    let mut matching = candidates
        .iter()
        .filter(|candidate| candidate.id.eq_ignore_ascii_case(value));
    let first = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    Some(first.id.clone())
}

fn desk_plan(
    request: &ResponderRequest,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    desk: &Desk,
    details: &[SelectorCandidate],
) -> Result<ResponderPlan> {
    let members: Vec<&str> = desks
        .members(&desk.id)?
        .into_iter()
        .filter(|id| roster.active_member(id).is_some())
        .collect();

    let Some(first) = members.first().copied() else {
        return orchestrator_plan(request, roster);
    };
    if desk.responder_mode == ResponderMode::Lead || members.len() == 1 {
        return Ok(decided(first, ResponderRung::DeskDefault));
    }

    let fallback = fallback_decision(first, SelectionDisposition::Unavailable);
    if request.selection_policy == SelectionPolicy::Disabled {
        return Ok(ResponderPlan::Decided {
            decision: fallback_decision(first, SelectionDisposition::Disabled),
        });
    }
    let candidates = candidates_for(&members, details)?;
    Ok(ResponderPlan::Select {
        request: SelectionRequest {
            message: request.message.clone(),
            desk_id: desk.id.clone(),
            candidates,
        },
        fallback,
    })
}

fn candidates_for(
    member_ids: &[&str],
    details: &[SelectorCandidate],
) -> Result<Vec<SelectorCandidate>> {
    member_ids
        .iter()
        .map(|id| {
            let mut matching = details.iter().filter(|candidate| candidate.id == *id);
            let first = matching.next();
            if matching.next().is_some() {
                return Err(Error::DuplicateSelectorCandidate {
                    agent_id: (*id).to_owned(),
                });
            }
            Ok(first.cloned().unwrap_or_else(|| SelectorCandidate {
                id: (*id).to_owned(),
                label: (*id).to_owned(),
                role: "Teammate".into(),
                description: None,
            }))
        })
        .collect()
}

fn direct_chat_member<'a>(chat: &str, roster: &'a Roster<'a>) -> Option<&'a RosterMember> {
    let identity = chat.strip_prefix("dm:").unwrap_or(chat);
    if let Some(member) = roster.active_member(identity) {
        return Some(member);
    }
    let mut matches = roster
        .active_members()
        .filter(|member| member.name.as_deref() == Some(identity));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn orchestrator_plan(request: &ResponderRequest, roster: &Roster<'_>) -> Result<ResponderPlan> {
    roster
        .active_member(&request.orchestrator_id)
        .map(|member| decided(&member.id, ResponderRung::Orchestrator))
        .ok_or_else(|| Error::NoActiveResponder {
            agent_id: request.orchestrator_id.clone(),
        })
}

fn decided(id: &str, rung: ResponderRung) -> ResponderPlan {
    ResponderPlan::Decided {
        decision: ResponderDecision {
            responder_id: id.to_owned(),
            rung,
            disposition: SelectionDisposition::NotApplicable,
        },
    }
}

fn fallback_decision(id: &str, disposition: SelectionDisposition) -> ResponderDecision {
    ResponderDecision {
        responder_id: id.to_owned(),
        rung: ResponderRung::DeskDefault,
        disposition,
    }
}
