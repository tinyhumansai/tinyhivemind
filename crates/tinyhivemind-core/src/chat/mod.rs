//! Conversation identity: which stored chat id names which conversation.
//!
//! A message is journaled under a chat id, and every surface that reads the
//! journal back — history rendering, thread resumption, an agent's context
//! seed — has to answer the same question about it: *is this the conversation
//! I am asking about?* Answering it in more than one place is how a transcript
//! ends up split across the ids that happened to write it, so it is answered
//! here and nowhere else.
//!
//! The whole subtlety is the default desk, which has four spellings and one
//! identity. See [`is_general_chat`].

#[cfg(test)]
mod test;

/// The console's default/orchestrator thread id.
///
/// The console addresses every send on that thread with `chat: "main"`, so
/// replies answering it are journaled under `"main"` rather than
/// [`GENERAL_DESK`]. Both spellings name the same conversation.
pub const MAIN_THREAD_ID: &str = "main";

/// The default desk's own id and display name.
///
/// A host may also use this string as the operator-facing word for the default
/// channel. If it does, the two must not be allowed to drift — pin them
/// together with a compile-time assertion on the host side rather than
/// duplicating the literal.
pub const GENERAL_DESK: &str = "General";

/// Does this stored chat id mean the General desk?
///
/// **Four spellings, one desk.** A console addresses its default thread as
/// `"main"`; an unaddressed message stores `None`; older events carry `""`;
/// and the desk's own id and name are `"General"`. All four are one
/// conversation, which is what stops a transcript from splitting across
/// whichever id happened to write each message.
///
/// This equivalence is deliberately **not** local to history rendering. A host
/// resolving a remembered thread root compares that root's chat id against the
/// channel being answered into, and comparing the raw strings there makes a
/// root stored as `None` fail to match the `"General"` it is rendered under —
/// so a threaded continuation silently resumes in the channel instead of its
/// thread. Two places deciding "same conversation?" by different rules is the
/// drift; one function is the fix. See [`same_conversation`].
///
/// # Examples
///
/// ```
/// # use tinyhivemind_core::chat::is_general_chat;
/// assert!(is_general_chat(None));
/// assert!(is_general_chat(Some("")));
/// assert!(is_general_chat(Some("main")));
/// assert!(is_general_chat(Some("General")));
/// assert!(!is_general_chat(Some("engineering")));
/// ```
#[must_use]
pub fn is_general_chat(chat: Option<&str>) -> bool {
    match chat {
        None => true,
        Some(chat) => {
            chat.is_empty()
                || chat.eq_ignore_ascii_case(MAIN_THREAD_ID)
                || chat.eq_ignore_ascii_case(GENERAL_DESK)
        }
    }
}

/// Do two stored chat ids name the same conversation?
///
/// Every spelling of the General desk is one conversation — see
/// [`is_general_chat`] — and everything else compares verbatim, because a desk
/// id is an opaque identifier and two desks differing only in case are two
/// desks.
///
/// Deliberately **not** a general-purpose case-insensitive compare: the folding
/// is a fact about one desk's history, not a licence to loosen the others.
///
/// # Examples
///
/// ```
/// # use tinyhivemind_core::chat::same_conversation;
/// // Every spelling of General is one conversation.
/// assert!(same_conversation(None, Some("General")));
/// assert!(same_conversation(Some("main"), Some("")));
///
/// // Everything else is verbatim — including case.
/// assert!(same_conversation(Some("engineering"), Some("engineering")));
/// assert!(!same_conversation(Some("engineering"), Some("Engineering")));
///
/// // General is not everyone else's conversation.
/// assert!(!same_conversation(None, Some("engineering")));
/// ```
#[must_use]
pub fn same_conversation(a: Option<&str>, b: Option<&str>) -> bool {
    if is_general_chat(a) || is_general_chat(b) {
        return is_general_chat(a) && is_general_chat(b);
    }
    a == b
}
