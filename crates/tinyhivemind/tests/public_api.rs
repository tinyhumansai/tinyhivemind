//! Public API regression tests for the runtime crate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyhivemind::{
    Conversation, EnqueueOutcome, EnqueueRefusal, MentionDispatchOutcome, PAGE_SIZE,
    PRESENT_SET_LIMIT, SCAN_LIMIT, SESSION_WINDOW, Sequence, SessionAuthor, SessionMessage,
    initialized_state, note_present,
    responder::{ResponderRung, SelectionDisposition},
};

#[test]
fn root_exports_runtime_records_and_constants() {
    let conversation = Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: Some(Sequence(3)),
    };
    let message = SessionMessage {
        sequence: Sequence(4),
        author: SessionAuthor::Operator,
        content: "hello".into(),
    };
    assert_eq!(conversation.thread_root, Some(Sequence(3)));
    assert_eq!(message.sequence, Sequence(4));
    assert_eq!((SESSION_WINDOW, PAGE_SIZE, SCAN_LIMIT), (30, 512, 2048));
}

#[test]
fn root_exports_continuous_sharing_state() {
    let mut state = initialized_state(
        Conversation {
            desk_id: "engineering".into(),
            desk_name: "Engineering".into(),
            thread_root: None,
        },
        Sequence(10),
    );
    assert!(note_present(&mut state, Sequence(11)).is_ok());
    assert!(state.present_above_watermark.contains(&Sequence(11)));
    assert_eq!(PRESENT_SET_LIMIT, 64);
}

#[test]
fn root_reexports_the_core_algebra() {
    assert!(tinyhivemind::chat::same_conversation(None, Some("General")));
    assert_eq!(ResponderRung::DeskDefault, ResponderRung::DeskDefault);
    assert_eq!(
        SelectionDisposition::Unavailable,
        SelectionDisposition::Unavailable
    );
}

#[test]
fn root_exports_dispatch_outcomes_and_conversation_mapping() {
    let conversation = Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: Some(Sequence(12)),
    };
    let scope = tinyhivemind::dispatch::DispatchConversation::from(&conversation);
    assert_eq!(scope.desk_id, "engineering");
    assert_eq!(scope.thread_root, Some(12));
    assert_eq!(
        MentionDispatchOutcome::Enqueued,
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(EnqueueOutcome::Already, EnqueueOutcome::Already);
    assert_eq!(EnqueueRefusal::Unauthorized, EnqueueRefusal::Unauthorized);

    let general = tinyhivemind::dispatch::DispatchConversation::from(&Conversation {
        desk_id: "MAIN".into(),
        desk_name: "General".into(),
        thread_root: Some(Sequence(12)),
    });
    assert_eq!(general.desk_id, tinyhivemind::chat::GENERAL_DESK);
    assert_eq!(general.thread_root, Some(12));
}

#[test]
fn root_exports_search_records_and_constants() {
    use tinyhivemind::{
        EXCERPT_CHARS, MessageHit, SEARCH_LIMIT, SEARCH_SCAN, SearchPattern, SearchQuery,
    };

    let query = SearchQuery::new("/^ship/")
        .in_conversation(Conversation {
            desk_id: "engineering".into(),
            desk_name: "Engineering".into(),
            thread_root: None,
        })
        .by_author("alice");
    assert_eq!(
        query.pattern,
        SearchPattern::Regex {
            source: "^ship".into()
        }
    );
    assert_eq!(query.limit, SEARCH_LIMIT);
    assert_eq!((SEARCH_LIMIT, SEARCH_SCAN, EXCERPT_CHARS), (10, 2048, 96));

    let hit = MessageHit {
        sequence: Sequence(4),
        chat_id: None,
        parent: None,
        author: SessionAuthor::Operator,
        excerpt: "ship it".into(),
        score: 1100,
        kind: tinyhivemind::select::MatchKind::Exact,
    };
    assert_eq!(hit.sequence, Sequence(4));
}

#[test]
fn root_exports_the_pin_fold_and_its_briefing_note() {
    use tinyhivemind::{LogMessage, PIN_LIMIT, PinAction, fold_pins, pin_note, read_directives};

    let rows = [
        LogMessage {
            sequence: Sequence(1),
            chat_id: None,
            parent: None,
            author: SessionAuthor::Operator,
            content: "the rate limiter resets at midnight UTC".into(),
        },
        LogMessage {
            sequence: Sequence(2),
            chat_id: None,
            parent: None,
            author: SessionAuthor::Operator,
            content: "!pin ^1 #limits keep this".into(),
        },
    ];
    let board = fold_pins(&rows, PIN_LIMIT);
    assert_eq!(board[0].sequence, Sequence(1));
    assert_eq!(board[0].label.as_deref(), Some("limits"));
    assert_eq!(
        pin_note(&board).expect("a note").heading,
        "Pinned in this conversation"
    );
    assert_eq!(
        read_directives("!unpin ^1", &SessionAuthor::Operator, Sequence(3))[0].action,
        PinAction::Unpin
    );
}

#[test]
fn root_exports_the_brevity_policy_stated_in_a_briefing() {
    use tinyhivemind::BrevityPolicy;

    assert_eq!(BrevityPolicy::DEFAULT.window, SESSION_WINDOW);
    assert_eq!(BrevityPolicy::DEFAULT.overrun("short"), None);
    assert!(BrevityPolicy::DEFAULT.rule_text().contains("600"));
}
