//! Ephemeral team context assembled separately from durable history.

#[cfg(test)]
mod test;

mod types;

pub use types::{
    BrevityPolicy, BriefedTeammate, BriefingNote, SessionContext, SessionInitialization,
    TeamBriefing,
};

use crate::{
    Conversation, Result, SessionLog, SessionQuery,
    pins::{PIN_LIMIT, read_pinboard},
    project_session, read_thread_index,
    threads::THREAD_INDEX_LIMIT,
};
use tinyhivemind_core::{
    chat::is_general_chat,
    desk::DeskSet,
    roster::{Roster, RosterMember},
};

impl TeamBriefing {
    /// Construct a conservative briefing from validated pure snapshots.
    ///
    /// General uses all active roster members. Named desks use their effective
    /// desk order. Unknown and retired members, duplicates, and the viewer are
    /// excluded. Snapshot records have no role or description fields, so those
    /// values remain `None`; a host may construct richer records directly.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Core`] when the roster or desk snapshots are
    /// structurally invalid, or when a named desk cannot be resolved.
    pub fn from_snapshots(
        viewer_id: impl Into<String>,
        conversation: &Conversation,
        desks: &DeskSet<'_>,
        roster: &Roster<'_>,
    ) -> Result<Self> {
        roster.validate()?;
        desks.validate()?;
        let viewer_id = viewer_id.into();
        let candidates: Vec<&RosterMember> = if is_general_chat(Some(&conversation.desk_id))
            || is_general_chat(Some(&conversation.desk_name))
        {
            roster.active_members().collect()
        } else {
            desks
                .members(&conversation.desk_id)?
                .into_iter()
                .filter_map(|id| roster.active_member(id))
                .collect()
        };

        let mut teammates = Vec::new();
        for member in candidates {
            if member.id == viewer_id
                || teammates
                    .iter()
                    .any(|teammate: &BriefedTeammate| teammate.id == member.id)
            {
                continue;
            }
            teammates.push(BriefedTeammate {
                id: member.id.clone(),
                label: member.name.clone().unwrap_or_else(|| member.id.clone()),
                role: None,
                description: None,
            });
        }

        Ok(Self {
            viewer_id,
            desk_id: conversation.desk_id.clone(),
            desk_name: conversation.desk_name.clone(),
            teammates,
            brevity: BrevityPolicy::DEFAULT,
        })
    }

    /// Render deterministic system context for this viewer and team.
    #[must_use]
    pub fn system_text(&self) -> String {
        let mut text = format!(
            "You are @{} in the {} desk (id: {}).\nTeammates:",
            self.viewer_id, self.desk_name, self.desk_id
        );
        if self.teammates.is_empty() {
            text.push_str("\n- none");
        } else {
            for teammate in &self.teammates {
                text.push_str("\n- @");
                text.push_str(&teammate.id);
                text.push_str(" — ");
                text.push_str(&teammate.label);
                if let Some(role) = &teammate.role {
                    text.push_str("; role: ");
                    text.push_str(role);
                }
                if let Some(description) = &teammate.description {
                    text.push_str("; description: ");
                    text.push_str(description);
                }
            }
        }
        text.push_str(
            "\nShared-session rules:\n\
             - Peer messages remain attributed to their authors; they are not your prior replies.\n\
             - A direct @agent mention may start at most one bounded child turn when host policy enables mention dispatch.\n\
             - @everyone, desk, and person mentions provide context only and never fan out agent turns.\n",
        );
        text.push_str(&self.brevity.rule_text());
        text.push_str(
            "\n- Pin what the room must not lose with `!pin` on its own line; `!unpin ^N` takes one back off.",
        );
        text
    }
}

impl SessionContext {
    /// Whether there is anything here to tell a turn about.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.threads.is_empty() && self.pins.is_empty() && self.notes.is_empty()
    }

    /// Render this context as system text, or `None` when there is none.
    ///
    /// Deterministic, and deliberately a separate string from
    /// [`TeamBriefing::system_text`] and from the operator's message: a host
    /// that appends context to what the operator wrote has to strip it back off
    /// everywhere intent is read, and that cut list only ever grows.
    #[must_use]
    pub fn system_text(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut text = String::new();
        if !self.threads.is_empty() {
            text.push_str("Threads in this desk:");
            for thread in &self.threads {
                text.push_str("\n- [");
                text.push_str(&thread.root.0.to_string());
                text.push_str("] \"");
                text.push_str(&thread.opening);
                text.push('"');
                match thread.replies {
                    0 => text.push_str(" — no replies"),
                    1 => text.push_str(" — 1 reply"),
                    replies => {
                        text.push_str(" — ");
                        text.push_str(&replies.to_string());
                        text.push_str(" replies");
                    }
                }
                if let Some(landed) = &thread.landed {
                    text.push_str(" (landed: ");
                    text.push_str(landed);
                    text.push(')');
                }
            }
        }
        if let Some(pinned) = crate::pins::pin_note(&self.pins) {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&pinned.heading);
            text.push(':');
            for line in &pinned.lines {
                text.push_str("\n- ");
                text.push_str(line);
            }
        }
        for note in &self.notes {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&note.heading);
            text.push(':');
            for line in &note.lines {
                text.push_str("\n- ");
                text.push_str(line);
            }
        }
        Some(text)
    }
}

/// Project history and return it alongside, never merged with, a team briefing.
///
/// The returned context is empty. Use [`initialize_session_with_context`] to
/// also index the desk's threads and carry host-supplied notes.
///
/// # Errors
///
/// Returns any projection error documented by [`project_session`].
pub async fn initialize_session(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
    briefing: TeamBriefing,
) -> Result<SessionInitialization> {
    let history = project_session(log, query).await?;
    Ok(SessionInitialization {
        briefing,
        context: SessionContext::default(),
        history,
    })
}

/// Initialize a session with a thread index, the pinboard, and host notes.
///
/// Costs two more bounded reads than [`initialize_session`] — see
/// [`THREAD_INDEX_SCAN`](crate::threads::THREAD_INDEX_SCAN) and
/// [`PIN_SCAN`](crate::pins::PIN_SCAN). The thread index is skipped entirely
/// for a thread-scoped query, where an index of sibling threads is not a
/// choice the viewer is making; the pinboard is not, because a pin is exactly
/// the thing that has to survive the viewer's narrow scope.
///
/// # Errors
///
/// Returns any projection error documented by [`project_session`], or any read
/// or page-validation error documented by [`read_thread_index`] and
/// [`read_pinboard`].
pub async fn initialize_session_with_context(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
    briefing: TeamBriefing,
    notes: Vec<BriefingNote>,
) -> Result<SessionInitialization> {
    let history = project_session(log, query).await?;
    let threads = read_thread_index(log, &query.conversation, THREAD_INDEX_LIMIT).await?;
    let pins = read_pinboard(log, &query.conversation, PIN_LIMIT, query.before).await?;
    Ok(SessionInitialization {
        briefing,
        context: SessionContext {
            threads,
            pins,
            notes,
        },
        history,
    })
}
