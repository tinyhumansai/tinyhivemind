//! Ephemeral team context assembled separately from durable history.

#[cfg(test)]
mod test;

mod types;

pub use types::{BriefedTeammate, SessionInitialization, TeamBriefing};

use crate::{Conversation, Result, SessionLog, SessionQuery, project_session};
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
             - @everyone, desk, and person mentions provide context only and never fan out agent turns.",
        );
        text
    }
}

/// Project history and return it alongside, never merged with, a team briefing.
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
    Ok(SessionInitialization { briefing, history })
}
