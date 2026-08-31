//! Public API regression tests for the runtime crate.

use tinyteams::{
    Conversation, PAGE_SIZE, SCAN_LIMIT, SESSION_WINDOW, Sequence, SessionAuthor, SessionMessage,
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
fn root_reexports_the_core_algebra() {
    assert!(tinyteams::chat::same_conversation(None, Some("General")));
}
