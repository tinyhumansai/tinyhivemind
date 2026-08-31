//! Public API regression tests for the runtime crate.

use tinyteams::{
    Conversation, EnqueueOutcome, MentionDispatchOutcome, PAGE_SIZE, PRESENT_SET_LIMIT, SCAN_LIMIT,
    SESSION_WINDOW, Sequence, SessionAuthor, SessionMessage, initialized_state, note_present,
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
    assert!(tinyteams::chat::same_conversation(None, Some("General")));
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
    let scope = tinyteams::dispatch::DispatchConversation::from(&conversation);
    assert_eq!(scope.desk_id, "engineering");
    assert_eq!(scope.thread_root, Some(12));
    assert_eq!(
        MentionDispatchOutcome::Enqueued,
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(EnqueueOutcome::Already, EnqueueOutcome::Already);
}
