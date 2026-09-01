//! Unit tests for the bounded, recency-ordered thread index.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{SessionAuthor, SessionFuture, SessionPage, SourceError};
use std::{collections::VecDeque, io, sync::Mutex};

#[derive(Debug)]
struct FakeLog {
    pages: Mutex<VecDeque<std::result::Result<SessionPage, SourceError>>>,
    calls: Mutex<usize>,
}

impl FakeLog {
    fn new(pages: Vec<SessionPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into_iter().map(Ok).collect()),
            calls: Mutex::new(0),
        }
    }

    fn failing() -> Self {
        Self {
            pages: Mutex::new(VecDeque::from([Err(
                Box::new(io::Error::other("offline")) as SourceError
            )])),
            calls: Mutex::new(0),
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.lock().expect("calls lock is not poisoned")
    }
}

impl SessionLog for FakeLog {
    fn read_before(&self, _before: Option<Sequence>, _limit: usize) -> SessionFuture<'_> {
        *self.calls.lock().expect("calls lock is not poisoned") += 1;
        Box::pin(async move {
            self.pages
                .lock()
                .expect("pages lock is not poisoned")
                .pop_front()
                .unwrap_or_else(|| Ok(SessionPage::default()))
        })
    }
}

fn conversation() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

fn message(sequence: u64, chat: Option<&str>, parent: Option<u64>, content: &str) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: chat.map(str::to_owned),
        parent: parent.map(Sequence),
        author: SessionAuthor::Operator,
        content: content.into(),
    }
}

fn page(messages: Vec<LogMessage>, next: Option<u64>) -> SessionPage {
    SessionPage {
        messages,
        next_before: next.map(Sequence),
    }
}

#[test]
fn thread_line_pins_its_wire_shape() {
    let line = ThreadLine {
        root: Sequence(41),
        opening: "draft the launch email".into(),
        replies: 4,
        latest: Sequence(58),
        landed: Some("In review".into()),
    };
    let wire = serde_json::json!({
        "root": 41,
        "opening": "draft the launch email",
        "replies": 4,
        "latest": 58,
        "landed": "In review"
    });
    assert_eq!(serde_json::to_value(&line).expect("serializes"), wire);
    assert_eq!(
        serde_json::from_value::<ThreadLine>(wire).expect("deserializes"),
        line
    );
}

#[test]
fn fold_counts_replies_per_root_and_tracks_the_newest_activity() {
    let rows = vec![
        message(1, Some("engineering"), None, "draft the launch email"),
        message(2, Some("engineering"), None, "check the invoice"),
        message(3, Some("engineering"), Some(1), "on it"),
        message(4, Some("engineering"), Some(1), "here is a draft"),
    ];
    let index = fold_thread_index(&rows, THREAD_INDEX_LIMIT);
    assert_eq!(
        index
            .iter()
            .map(|line| (
                line.root.0,
                line.opening.as_str(),
                line.replies,
                line.latest.0
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, "draft the launch email", 2, 4),
            (2, "check the invoice", 0, 2)
        ]
    );
    assert!(index.iter().all(|line| line.landed.is_none()));
}

#[test]
fn fold_orders_by_newest_activity_not_by_root() {
    let rows = vec![
        message(1, Some("engineering"), None, "older root"),
        message(2, Some("engineering"), None, "newer root"),
        message(3, Some("engineering"), Some(1), "revives the older thread"),
    ];
    assert_eq!(
        fold_thread_index(&rows, THREAD_INDEX_LIMIT)
            .iter()
            .map(|line| line.root.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn fold_keeps_only_the_most_recent_threads_up_to_the_limit() {
    let rows: Vec<LogMessage> = (1..=8)
        .map(|sequence| {
            message(
                sequence,
                Some("engineering"),
                None,
                &format!("thread {sequence}"),
            )
        })
        .collect();
    assert_eq!(
        fold_thread_index(&rows, 3)
            .iter()
            .map(|line| line.root.0)
            .collect::<Vec<_>>(),
        vec![8, 7, 6]
    );
    assert!(fold_thread_index(&rows, 0).is_empty());
}

#[test]
fn fold_ignores_blank_roots_blank_replies_and_replies_without_a_root() {
    let rows = vec![
        message(1, Some("engineering"), None, "  \n "),
        message(2, Some("engineering"), Some(1), "reply to a blank root"),
        message(3, Some("engineering"), Some(99), "reply to an unseen root"),
        message(4, Some("engineering"), None, "real root"),
        message(5, Some("engineering"), Some(4), "  "),
        message(6, Some("engineering"), Some(4), "counted"),
    ];
    let index = fold_thread_index(&rows, THREAD_INDEX_LIMIT);
    assert_eq!(
        index
            .iter()
            .map(|line| (line.root.0, line.replies, line.latest.0))
            .collect::<Vec<_>>(),
        vec![(4, 1, 6)]
    );
}

#[test]
fn opening_collapses_whitespace_and_truncates_on_a_character_boundary() {
    let rows = vec![
        message(
            1,
            Some("engineering"),
            None,
            "  draft\n  the   launch\temail  ",
        ),
        message(
            2,
            Some("engineering"),
            None,
            &"é".repeat(THREAD_OPENING_CHARS + 5),
        ),
        message(
            3,
            Some("engineering"),
            None,
            &"x".repeat(THREAD_OPENING_CHARS),
        ),
    ];
    let index = fold_thread_index(&rows, THREAD_INDEX_LIMIT);
    let opening = |root: u64| {
        index
            .iter()
            .find(|line| line.root.0 == root)
            .expect("indexed")
            .opening
            .clone()
    };
    assert_eq!(opening(1), "draft the launch email");
    assert_eq!(opening(2), format!("{}…", "é".repeat(THREAD_OPENING_CHARS)));
    assert_eq!(opening(3), "x".repeat(THREAD_OPENING_CHARS));
}

#[tokio::test]
async fn index_reads_pages_filters_the_desk_and_returns_newest_first() {
    let log = FakeLog::new(vec![
        page(
            vec![
                message(5, Some("engineering"), Some(1), "reply"),
                message(4, Some("other"), None, "elsewhere"),
            ],
            Some(4),
        ),
        page(
            vec![
                message(2, Some("Engineering"), None, "second thread"),
                message(1, Some("engineering"), None, "first thread"),
            ],
            None,
        ),
    ]);
    let index = read_thread_index(&log, &conversation(), THREAD_INDEX_LIMIT)
        .await
        .expect("indexes");
    assert_eq!(
        index
            .iter()
            .map(|line| (line.root.0, line.replies))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 0)]
    );
    assert_eq!(log.call_count(), 2);
}

#[tokio::test]
async fn index_is_empty_and_unread_for_a_thread_or_a_zero_limit() {
    let log = FakeLog::new(vec![page(
        vec![message(1, Some("engineering"), None, "root")],
        None,
    )]);
    let mut inside = conversation();
    inside.thread_root = Some(Sequence(1));
    assert!(
        read_thread_index(&log, &inside, THREAD_INDEX_LIMIT)
            .await
            .expect("indexes")
            .is_empty()
    );
    assert!(
        read_thread_index(&log, &conversation(), 0)
            .await
            .expect("indexes")
            .is_empty()
    );
    assert_eq!(log.call_count(), 0);
}

#[tokio::test]
async fn index_stops_at_its_own_scan_bound_well_below_the_projection_limit() {
    const _: () = assert!(THREAD_INDEX_SCAN < crate::SCAN_LIMIT);
    // One full page exhausts the bound, so the root two rows further back is
    // never read and its thread is absent from the index.
    let rows = (0..THREAD_INDEX_SCAN as u64)
        .map(|offset| {
            message(
                THREAD_INDEX_SCAN as u64 - offset,
                Some("engineering"),
                Some(1),
                "reply",
            )
        })
        .collect();
    let log = FakeLog::new(vec![
        page(rows, Some(1)),
        page(
            vec![message(1, Some("engineering"), None, "unreachable root")],
            None,
        ),
    ]);
    assert!(
        read_thread_index(&log, &conversation(), THREAD_INDEX_LIMIT)
            .await
            .expect("indexes")
            .is_empty()
    );
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn index_reports_read_and_validation_failures() {
    assert!(matches!(
        read_thread_index(&FakeLog::failing(), &conversation(), THREAD_INDEX_LIMIT).await,
        Err(Error::Read { .. })
    ));

    let log = FakeLog::new(vec![page(Vec::new(), Some(4))]);
    assert!(matches!(
        read_thread_index(&log, &conversation(), THREAD_INDEX_LIMIT).await,
        Err(Error::EmptyPageCursor { .. })
    ));
}
