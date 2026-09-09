//! The room as a tool: what a seat may say, and what one utterance becomes.
//!
//! A seat's free text is its *thinking*. What it says to the room is an action
//! it takes, and this module is the algebra of that action: the tools it may
//! call, what makes a call valid, and the single fold from an accepted call to
//! the row the host appends.
//!
//! # Why this is not a delimiter
//!
//! A model-authored delimiter is an interface with a fallible producer. A seat
//! closed a verified result with `<<<POST>>> … <<<POST>>>` instead of
//! `<<<POST … POST>>>`; the room received the three characters `>>>`, read
//! that as silence, and nudged the seat that had just spoken. A delimiter has
//! no schema, no validation, and no way to tell its producer it got it wrong.
//!
//! A tool call has all three. [`interpret`] refuses a malformed call with a
//! typed [`UtteranceRejection`] whose `Display` is written to be read by the
//! seat, *inside its own turn*, while it can still call again. The fence
//! survives in [`fence`] for the two callers that cannot reach a tool.
//!
//! # Why this is not a host
//!
//! Nothing here opens a socket, writes a file, or parses JSON. A host maps its
//! own wire onto [`CallArguments`] and renders [`tool_specs`] into its own
//! tool language; what it gets back is a decision, not an effect.
//!
//! **A tool call is a request to speak. The host appends, and the host
//! decides.** In particular a [`Utterance::Dm`] is resolved through the aside
//! policy and may be refused, in which case the row stays desk-visible —
//! failing toward the room rather than toward silence — and the refusal is
//! carried back in [`CommittedUtterance::refusal`] so the host can say so.
//!
//! # Example
//!
//! ```
//! use tinyhivemind::speech::{CallArguments, ToolCall, Utterance, interpret};
//!
//! let call = interpret(
//!     "post",
//!     &CallArguments { message: Some("  B holds at 10^18  "), ..Default::default() },
//! )
//! .expect("a post with a message is an utterance");
//! assert_eq!(
//!     call,
//!     ToolCall::Speak(Utterance::Post { message: "B holds at 10^18".into() }),
//! );
//!
//! // And a malformed one is refused to its author, with a sentence it can read.
//! let refused = interpret("post", &CallArguments::default()).expect_err("no message");
//! assert_eq!(refused.to_string(), "`message` must be a non-empty string");
//! ```

pub mod fence;

mod tools;
mod types;

#[cfg(test)]
mod test;

pub use tools::{READ_DEFAULT, READ_MAX, ToolParameter, ToolSpec, tool_specs};
pub use types::{
    CallArguments, CommittedUtterance, ParameterKind, ToolCall, Utterance, UtteranceRejection,
};

use tinyhivemind_core::{
    aside::{AsideDecision, AsideInput, AsidePolicy, Audience, aside},
    desk::DeskSet,
    dispatch::DispatchConversation,
    error::Result,
    mention::{Mention, MentionAuthor, MentionTarget, resolve},
    roster::Roster,
};

/// The line-leading marker a seat may write instead of calling `dm`.
///
/// Kept because the grammar predates the tool and a seat briefed on either
/// spelling should reach the same audience. The decision is the library's
/// whichever way it was asked for.
const ASIDE_MARKER: &str = "!aside";

/// Read one tool call, or refuse it to the seat that made it.
///
/// `name` is the bare tool name. A host that namespaces its tools — an MCP
/// server called `desk` serving `post` as `desk_post` — strips its own prefix
/// before calling this.
///
/// # Errors
///
/// Returns the [`UtteranceRejection`] to hand back to the seat: an unknown
/// tool, an empty message, or a `dm` that named nobody. Recipients are checked
/// for shape here and against the roster in [`commit_utterance`], which is the
/// first point that holds one.
pub fn interpret(
    name: &str,
    arguments: &CallArguments<'_>,
) -> std::result::Result<ToolCall, UtteranceRejection> {
    match name {
        "post" => Ok(ToolCall::Speak(Utterance::Post {
            message: text(arguments, "message")?,
        })),
        "close" => Ok(ToolCall::Speak(Utterance::Close {
            message: text(arguments, "message")?,
        })),
        "dm" => {
            let message = text(arguments, "message")?;
            let to = recipients(arguments.to);
            if to.is_empty() {
                return Err(UtteranceRejection::NoRecipients);
            }
            Ok(ToolCall::Speak(Utterance::Dm { to, message }))
        }
        "read" => Ok(ToolCall::Read {
            limit: read_limit(arguments.limit),
        }),
        other => Err(UtteranceRejection::UnknownTool {
            name: other.to_string(),
        }),
    }
}

/// How many messages a `read` asking for `limit` should be given.
///
/// Absent is [`READ_DEFAULT`]; anything else is clamped into
/// `1..=`[`READ_MAX`]. A seat that asks for the whole transcript gets a
/// bounded answer rather than its own context window back.
#[must_use]
pub fn read_limit(limit: Option<u64>) -> usize {
    let asked = limit.unwrap_or(READ_DEFAULT as u64);
    usize::try_from(asked)
        .unwrap_or(READ_MAX)
        .clamp(1, READ_MAX)
}

fn text(
    arguments: &CallArguments<'_>,
    field: &'static str,
) -> std::result::Result<String, UtteranceRejection> {
    let value = arguments.message.unwrap_or_default().trim();
    if value.is_empty() {
        return Err(UtteranceRejection::EmptyText { field });
    }
    Ok(value.to_string())
}

/// Seat ids as the room holds them: no `@`, no blanks, no repeats, in order.
fn recipients(to: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in to {
        let id = entry.trim().trim_start_matches('@').trim();
        if !id.is_empty() && !out.iter().any(|held| held == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// Everything the room needs to turn one utterance into one row.
///
/// Every field is borrowed from what the caller already holds for the turn.
/// The two counts are folded by the host from its own journal, exactly as
/// [`aside`] has always required, because neither is derivable from arguments
/// this crate is given.
#[derive(Debug)]
pub struct CommitRequest<'a> {
    /// What the seat said.
    pub utterance: &'a Utterance,
    /// Whose turn it is.
    pub speaker_id: &'a str,
    /// The conversation the row is committed on.
    pub conversation: &'a DispatchConversation,
    /// The desk's aside policy.
    pub aside: AsidePolicy,
    /// How many rows the currently open aside has already spent.
    pub spent: usize,
    /// Whether a different prior aside among these participants is unsettled.
    pub unsettled: bool,
    /// Who is on the roster.
    pub roster: &'a Roster<'a>,
    /// What the desks are.
    pub desks: &'a DeskSet<'a>,
}

/// Turn one accepted utterance into the row the host appends.
///
/// This is the whole of what an utterance means: the text, who may read it,
/// the mentions dispatch will route on, and whether the desk was reported
/// finished. It is a fold — no read, no write, no wait — so a host may call it
/// from inside its tool handler and hand the refusal back to the seat before
/// the turn ends.
///
/// Two rules are worth stating because they are not obvious from the types:
///
/// - **A `dm`'s recipients are targets, not prose.** They are built from the
///   `to` field directly. A host that spelled them back into `@a @b` and
///   re-read them through the grammar would lose any id the grammar does not
///   accept, silently.
/// - **A refused aside is a desk row.** The refusal is returned beside the
///   audience rather than in place of it, so the message reaches the room
///   either way and the seat can be told which way it went.
///
/// # Errors
///
/// Returns a typed core error when the supplied roster or desk snapshot is
/// malformed, and [`Error::UnknownRecipient`](UtteranceRejection) is not
/// among them: a `dm` naming somebody who cannot receive one is refused as a
/// rejection through [`check_recipients`] before this is called.
pub fn commit_utterance(request: &CommitRequest<'_>) -> Result<CommittedUtterance> {
    let content = request.utterance.message().to_string();
    let author = MentionAuthor::Agent {
        id: request.speaker_id.to_string(),
    };
    let written = resolve(&content, None, &author, request.roster, request.desks);

    let addressed = match request.utterance {
        Utterance::Dm { to, .. } => targets(to),
        _ => Vec::new(),
    };
    // A `dm` addresses its recipients whether or not its text also names them.
    // Where the text names nobody, they are also who the next turn goes to.
    let mentions = if addressed.is_empty() || names_a_peer(&written, request.speaker_id) {
        written
    } else {
        addressed.clone()
    };

    let private = !addressed.is_empty() || content.trim_start().starts_with(ASIDE_MARKER);
    if !private {
        return Ok(CommittedUtterance {
            content,
            audience: Audience::Desk,
            mentions,
            closing: request.utterance.closing(),
            refusal: None,
        });
    }

    let decision = aside(
        request.aside,
        &AsideInput {
            conversation: request.conversation.clone(),
            author_id: request.speaker_id.to_string(),
            mentions: if addressed.is_empty() {
                mentions.clone()
            } else {
                addressed
            },
            spent: request.spent,
            unsettled: request.unsettled,
        },
        request.roster,
        request.desks,
    )?;
    let (audience, refusal) = match decision {
        AsideDecision::One { audience } => (audience, None),
        AsideDecision::None { reason } => (Audience::Desk, Some(reason)),
    };
    // A private row's content is only ever safe with its audience. A `dm`'s
    // body may name a peer the `to` field did not — the check-in above hands
    // that peer the next turn only when it is one of the admitted readers,
    // never the content itself carried to somebody who was never admitted to
    // it. A refusal falls back to `Audience::Desk` and needs no filtering:
    // the row is fully public by the time this runs.
    let mentions = match &audience {
        Audience::Aside { members } => mentions
            .into_iter()
            .filter(|mention| {
                matches!(&mention.target, MentionTarget::Agent { id } if members.contains(id))
            })
            .collect(),
        Audience::Desk => mentions,
    };
    Ok(CommittedUtterance {
        content,
        audience,
        mentions,
        closing: request.utterance.closing(),
        refusal,
    })
}

/// Refuse a `dm` that named somebody who cannot receive one.
///
/// Split from [`commit_utterance`] so a host can run it the moment the call
/// arrives — the recipient check is the one rejection a seat can act on and
/// the one it is most likely to trip, by naming a seat that has left the desk
/// or by inventing one.
///
/// # Errors
///
/// Returns [`UtteranceRejection::UnknownRecipient`] for the first id that is
/// not an active roster agent, and [`UtteranceRejection::SelfRecipient`] when
/// a seat addresses only itself. The first unresolvable id stops the check
/// rather than being dropped in favor of a later one: an audience assembled by
/// quietly discarding names is not the audience its author wrote.
pub fn check_recipients(
    utterance: &Utterance,
    speaker_id: &str,
    roster: &Roster<'_>,
) -> std::result::Result<(), UtteranceRejection> {
    let Utterance::Dm { to, .. } = utterance else {
        return Ok(());
    };
    for id in to {
        if roster.active_member(id).is_none() {
            return Err(UtteranceRejection::UnknownRecipient { id: id.clone() });
        }
    }
    if to.iter().all(|id| id == speaker_id) {
        return Err(UtteranceRejection::SelfRecipient);
    }
    Ok(())
}

/// The active peers one utterance addresses, without its author and without
/// repeats, in the order they were written.
///
/// A `dm` addresses the seats its `to` field names. Anything else addresses
/// whoever its text mentions. A host needs this to answer the bookkeeping
/// questions [`aside`] asks of it — whether a prior aside among these same
/// participants is still unsettled — and asking it here means the host does
/// not re-derive an audience the fold has already decided.
#[must_use]
pub fn addressed_peers(
    utterance: &Utterance,
    speaker_id: &str,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Vec<String> {
    let mentions = match utterance {
        Utterance::Dm { to, .. } => targets(to),
        other => {
            let author = MentionAuthor::Agent {
                id: speaker_id.to_string(),
            };
            resolve(other.message(), None, &author, roster, desks)
        }
    };
    let mut ordered: Vec<&Mention> = mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);
    let mut peers: Vec<String> = Vec::new();
    for mention in ordered {
        if mention.quiet {
            continue;
        }
        if let MentionTarget::Agent { id } = &mention.target
            && id != speaker_id
            && !peers.contains(id)
        {
            peers.push(id.clone());
        }
    }
    peers
}

/// Whether a message's own text hands the turn to somebody other than its author.
fn names_a_peer(mentions: &[Mention], speaker_id: &str) -> bool {
    mentions.iter().any(|mention| {
        !mention.quiet && matches!(&mention.target, MentionTarget::Agent { id } if id != speaker_id)
    })
}

/// The `to` field as mentions, in the order the seat wrote them.
///
/// The offsets are positional rather than authored: no such text exists in the
/// message body, and `aside` reads them only to keep reading order. Marking
/// them quiet would remove them from the audience they exist to name.
fn targets(to: &[String]) -> Vec<Mention> {
    to.iter()
        .enumerate()
        .map(|(index, id)| Mention {
            target: MentionTarget::Agent { id: id.clone() },
            text: format!("@{id}"),
            offset: index,
            quiet: false,
        })
        .collect()
}
