//! Stateless continuous transcript sharing over a caller-owned watermark.

#[cfg(test)]
mod test;

mod types;

pub use types::{ReinitializeReason, SessionDelta, SharingPlan, SharingQuery, SharingState};

use crate::{
    Error, PAGE_SIZE, Result, SCAN_LIMIT, Sequence, SessionLog, SessionMessage,
    session::{matches_conversation, validate_page},
};
use std::collections::BTreeSet;

/// Maximum distinct future rows retained in [`SharingState`].
pub const PRESENT_SET_LIMIT: usize = 64;

/// Create sharing progress after a full initialization has been accepted.
///
/// The host must call this only after the P4 briefing, history, and current
/// trigger are accepted by its agent session.
#[must_use]
pub fn initialized_state(conversation: crate::Conversation, watermark: Sequence) -> SharingState {
    SharingState {
        conversation,
        watermark,
        present_above_watermark: BTreeSet::new(),
    }
}

/// Record a later row already accepted by the host without moving the watermark.
///
/// # Errors
///
/// Returns [`Error::PresentSetOverflow`] if inserting a new future sequence
/// would exceed [`PRESENT_SET_LIMIT`]. The state is unchanged in that case.
pub fn note_present(state: &mut SharingState, sequence: Sequence) -> Result<()> {
    if sequence <= state.watermark || state.present_above_watermark.contains(&sequence) {
        return Ok(());
    }
    if state.present_above_watermark.len() == PRESENT_SET_LIMIT {
        return Err(Error::PresentSetOverflow {
            limit: PRESENT_SET_LIMIT,
            sequence,
        });
    }
    state.present_above_watermark.insert(sequence);
    Ok(())
}

/// Prepare attributed rows added since the caller's last accepted watermark.
///
/// Returned state is only a proposal. The host must serialize or compare-and-
/// swap its commit after it accepts both the delta and current trigger.
///
/// # Errors
///
/// Returns [`Error::WatermarkRegression`] for a regressing bound, [`Error::Read`]
/// for a host read failure, or a P4 page-validation error for malformed pages.
pub async fn prepare_delta(
    log: &(dyn SessionLog + '_),
    query: &SharingQuery<'_>,
) -> Result<SharingPlan> {
    if !query
        .desired_conversation
        .equivalent_to(query.current_conversation)
        || !query
            .desired_conversation
            .equivalent_to(&query.state.conversation)
    {
        return Ok(SharingPlan::Reinitialize {
            reason: ReinitializeReason::ConversationChanged,
        });
    }
    if query.before < query.state.watermark {
        return Err(Error::WatermarkRegression {
            before: query.before,
            watermark: query.state.watermark,
        });
    }
    if query.before == query.state.watermark {
        return Ok(SharingPlan::Delta(SessionDelta {
            messages: Vec::new(),
            next_state: query.state.clone(),
        }));
    }

    let mut cursor = Some(query.before);
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut messages = Vec::new();
    let mut crossed = false;

    while scanned < SCAN_LIMIT && !crossed {
        let limit = PAGE_SIZE.min(SCAN_LIMIT - scanned);
        let page = log
            .read_before(cursor, limit)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, limit, &mut seen)?;

        for raw in &page.messages {
            scanned += 1;
            if raw.sequence <= query.state.watermark {
                crossed = true;
                break;
            }
            if matches_conversation(raw, query.desired_conversation)
                && !raw.content.trim().is_empty()
                && !query.state.present_above_watermark.contains(&raw.sequence)
            {
                messages.push(SessionMessage {
                    sequence: raw.sequence,
                    author: raw.author.clone(),
                    content: raw.content.clone(),
                });
            }
        }

        if crossed {
            break;
        }
        if scanned == SCAN_LIMIT {
            return Ok(SharingPlan::Reinitialize {
                reason: ReinitializeReason::GapTooLarge,
            });
        }
        let Some(next) = page.next_before else {
            return Ok(SharingPlan::Reinitialize {
                reason: ReinitializeReason::WatermarkUnavailable,
            });
        };
        cursor = Some(next);
    }

    messages.reverse();
    let mut next_state = query.state.clone();
    next_state.watermark = query.before;
    next_state
        .present_above_watermark
        .retain(|sequence| *sequence > query.before);
    Ok(SharingPlan::Delta(SessionDelta {
        messages,
        next_state,
    }))
}
