//! Unit tests for transcript search.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{Sequence, SessionFuture, SessionPage, SourceError};
use std::{collections::VecDeque, io, sync::Mutex};
use tinyhivemind_core::select::MatchKind;

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

fn agent(id: &str) -> SessionAuthor {
    SessionAuthor::Agent {
        id: id.to_owned(),
        label: id.to_owned(),
    }
}

fn message(sequence: u64, chat: Option<&str>, parent: Option<u64>, content: &str) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: chat.map(ToOwned::to_owned),
        parent: parent.map(Sequence),
        author: agent("alice"),
        content: content.to_owned(),
    }
}

fn conversation(thread_root: Option<u64>) -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: thread_root.map(Sequence),
    }
}

/// Newest-first page, as the port requires.
fn page(messages: Vec<LogMessage>) -> SessionPage {
    SessionPage {
        messages,
        next_before: None,
    }
}

#[tokio::test]
async fn finds_matching_rows_best_first() {
    let log = FakeLog::new(vec![page(vec![
        message(
            4,
            Some("engineering"),
            None,
            "shipping is blocked on review",
        ),
        message(3, Some("engineering"), None, "ship"),
        message(2, Some("engineering"), None, "unrelated chatter"),
    ])]);
    let hits = search_messages(&log, &SearchQuery::new("ship"))
        .await
        .expect("searches");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].sequence, Sequence(3));
    assert_eq!(hits[0].kind, MatchKind::Exact);
    assert_eq!(hits[1].sequence, Sequence(4));
}

#[tokio::test]
async fn scopes_a_search_to_one_desk_including_thread_interiors() {
    let log = FakeLog::new(vec![page(vec![
        message(5, Some("support"), None, "ship it"),
        message(4, Some("engineering"), Some(3), "ship it"),
        message(3, Some("engineering"), None, "opening"),
    ])]);
    let query = SearchQuery::new("ship").in_conversation(conversation(None));
    let hits = search_messages(&log, &query).await.expect("searches");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].sequence, Sequence(4));
    assert_eq!(hits[0].parent, Some(Sequence(3)));
}

#[tokio::test]
async fn scopes_a_search_to_one_thread() {
    let log = FakeLog::new(vec![page(vec![
        message(6, Some("engineering"), Some(5), "ship it"),
        message(5, Some("engineering"), None, "another thread"),
        message(4, Some("engineering"), Some(3), "ship it here"),
        message(3, Some("engineering"), None, "ship the root"),
    ])]);
    let query = SearchQuery::new("ship").in_conversation(conversation(Some(3)));
    let hits = search_messages(&log, &query).await.expect("searches");
    let found: Vec<Sequence> = hits.iter().map(|hit| hit.sequence).collect();
    assert_eq!(found, vec![Sequence(4), Sequence(3)]);
}

#[tokio::test]
async fn filters_by_author_id() {
    let mut newest = message(3, None, None, "ship it");
    newest.author = agent("bob");
    let log = FakeLog::new(vec![page(vec![newest, message(2, None, None, "ship it")])]);
    let query = SearchQuery::new("ship").by_author("bob");
    let hits = search_messages(&log, &query).await.expect("searches");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].sequence, Sequence(3));
}

#[tokio::test]
async fn matches_an_operator_and_a_system_row_by_author_id() {
    let mut operator = message(3, None, None, "ship it");
    operator.author = SessionAuthor::Operator;
    let mut system = message(2, None, None, "ship it");
    system.author = SessionAuthor::System {
        kind: "workflow".into(),
        label: "Workflow".into(),
    };
    let log = FakeLog::new(vec![page(vec![operator, system])]);
    let by_operator = search_messages(&log, &SearchQuery::new("ship").by_author("operator"))
        .await
        .expect("searches");
    assert_eq!(by_operator.len(), 1);
    assert_eq!(by_operator[0].sequence, Sequence(3));

    let log = FakeLog::new(vec![page(vec![
        message(4, None, None, "ship it"),
        LogMessage {
            author: SessionAuthor::System {
                kind: "workflow".into(),
                label: "Workflow".into(),
            },
            ..message(2, None, None, "ship it")
        },
    ])]);
    let by_system = search_messages(&log, &SearchQuery::new("ship").by_author("workflow"))
        .await
        .expect("searches");
    assert_eq!(by_system.len(), 1);
    assert_eq!(by_system[0].sequence, Sequence(2));
}

#[tokio::test]
async fn excerpts_a_long_row_around_the_match() {
    let filler = "a ".repeat(120);
    let content = format!("{filler}needle {filler}");
    let log = FakeLog::new(vec![page(vec![message(2, None, None, &content)])]);
    let hits = search_messages(&log, &SearchQuery::new("needle"))
        .await
        .expect("searches");
    let excerpt = &hits[0].excerpt;
    assert!(excerpt.contains("needle"));
    assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
    assert!(excerpt.chars().count() <= EXCERPT_CHARS + 2);
}

#[tokio::test]
async fn excerpts_a_match_past_a_width_expanding_lowercase_run() {
    // `İ` lowercases to two characters, so 40 of them ahead of the match push
    // the *lowercased* offset `score_pattern` reports to 80 even though
    // "needle" starts at original character 40. A window built from the raw
    // lowered offset lands well past the match; excerpt() has to be given the
    // offset mapped back onto the original text first.
    let filler_before = "İ".repeat(40);
    let filler_after = "a ".repeat(60);
    let content = format!("{filler_before}needle {filler_after}");
    let log = FakeLog::new(vec![page(vec![message(2, None, None, &content)])]);
    let hits = search_messages(&log, &SearchQuery::new("needle"))
        .await
        .expect("searches");
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].excerpt.contains("needle"),
        "excerpt {:?} lost the match behind the expanding filler",
        hits[0].excerpt
    );
}

#[tokio::test]
async fn keeps_a_short_row_whole_and_collapses_its_whitespace() {
    let log = FakeLog::new(vec![page(vec![message(
        2,
        None,
        None,
        "ship\n\n  it   now",
    )])]);
    let hits = search_messages(&log, &SearchQuery::new("ship"))
        .await
        .expect("searches");
    assert_eq!(hits[0].excerpt, "ship it now");
}

#[tokio::test]
async fn returns_nothing_and_reads_nothing_for_a_blank_query_or_zero_limit() {
    let log = FakeLog::new(vec![page(vec![message(2, None, None, "ship")])]);
    assert!(
        search_messages(&log, &SearchQuery::new("   "))
            .await
            .expect("searches")
            .is_empty()
    );
    let mut query = SearchQuery::new("ship");
    query.limit = 0;
    assert!(
        search_messages(&log, &query)
            .await
            .expect("searches")
            .is_empty()
    );
    assert_eq!(log.call_count(), 0);
}

#[tokio::test]
async fn truncates_to_the_query_limit() {
    let log = FakeLog::new(vec![page(vec![
        message(4, None, None, "ship one"),
        message(3, None, None, "ship two"),
        message(2, None, None, "ship three"),
    ])]);
    let mut query = SearchQuery::new("ship");
    query.limit = 2;
    let hits = search_messages(&log, &query).await.expect("searches");
    assert_eq!(hits.len(), 2);
}

#[tokio::test]
async fn reports_a_host_read_failure() {
    let log = FakeLog::failing();
    let error = search_messages(&log, &SearchQuery::new("ship"))
        .await
        .expect_err("read fails");
    assert!(matches!(error, Error::Read { .. }));
}

#[tokio::test]
async fn walks_older_pages_until_the_log_ends() {
    let log = FakeLog::new(vec![
        SessionPage {
            messages: vec![message(9, None, None, "nothing here")],
            next_before: Some(Sequence(9)),
        },
        page(vec![message(4, None, None, "ship it")]),
    ]);
    let hits = search_messages(&log, &SearchQuery::new("ship"))
        .await
        .expect("searches");
    assert_eq!(hits.len(), 1);
    assert_eq!(log.call_count(), 2);
}

#[tokio::test]
async fn searches_threads_by_their_opening_words() {
    let log = FakeLog::new(vec![page(vec![
        message(5, Some("engineering"), Some(4), "a reply about shipping"),
        message(4, Some("engineering"), None, "invoice for the venue"),
        message(3, Some("engineering"), None, "shipping the launch email"),
    ])]);
    let hits = search_threads(
        &log,
        &conversation(None),
        &SearchPattern::parse("shipping"),
        8,
    )
    .await
    .expect("searches");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line.root, Sequence(3));
    assert_eq!(hits[0].kind, MatchKind::Prefix);
}

#[tokio::test]
async fn searches_no_threads_from_inside_one_or_at_zero_limit() {
    let log = FakeLog::new(vec![page(vec![message(3, None, None, "shipping")])]);
    assert!(
        search_threads(
            &log,
            &conversation(Some(3)),
            &SearchPattern::parse("ship"),
            8
        )
        .await
        .expect("searches")
        .is_empty()
    );
    assert!(
        search_threads(&log, &conversation(None), &SearchPattern::parse("ship"), 0)
            .await
            .expect("searches")
            .is_empty()
    );
    assert!(
        search_threads(&log, &conversation(None), &SearchPattern::parse(" "), 8)
            .await
            .expect("searches")
            .is_empty()
    );
    assert_eq!(log.call_count(), 0);
}

#[test]
fn parses_a_delimited_query_as_an_expression_and_anything_else_as_text() {
    assert_eq!(
        SearchPattern::parse("/^ship/"),
        SearchPattern::Regex {
            source: "^ship".into()
        }
    );
    assert_eq!(
        SearchPattern::parse("  ship  "),
        SearchPattern::Text {
            query: "ship".into()
        }
    );
}

#[test]
fn pins_the_wire_form_of_a_search_query() {
    let query = SearchQuery::new("/^ship/")
        .in_conversation(conversation(Some(3)))
        .by_author("alice");
    assert_eq!(
        serde_json::to_value(&query).expect("serializes"),
        serde_json::json!({
            "pattern": { "type": "regex", "source": "^ship" },
            "scope": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": 3
            },
            "author_id": "alice",
            "before": null,
            "limit": SEARCH_LIMIT
        })
    );
    assert_eq!(
        serde_json::from_value::<SearchQuery>(serde_json::to_value(&query).expect("serializes"))
            .expect("deserializes"),
        query
    );
}

#[test]
fn pins_the_wire_form_of_a_message_hit() {
    let hit = MessageHit {
        sequence: Sequence(7),
        chat_id: Some("engineering".into()),
        parent: Some(Sequence(3)),
        author: agent("alice"),
        excerpt: "ship it".into(),
        score: 1100,
        kind: MatchKind::Exact,
    };
    assert_eq!(
        serde_json::to_value(&hit).expect("serializes"),
        serde_json::json!({
            "sequence": 7,
            "chat_id": "engineering",
            "parent": 3,
            "author": { "type": "agent", "id": "alice", "label": "alice" },
            "excerpt": "ship it",
            "score": 1100,
            "kind": "exact"
        })
    );
}

#[cfg(not(feature = "regex"))]
#[tokio::test]
async fn refuses_an_expression_without_the_feature() {
    let log = FakeLog::new(vec![page(vec![message(2, None, None, "ship")])]);
    let error = search_messages(&log, &SearchQuery::new("/^ship/"))
        .await
        .expect_err("unsupported");
    assert!(matches!(error, Error::RegexUnsupported { .. }));
}

#[cfg(feature = "regex")]
mod expressions {
    use super::{FakeLog, SearchQuery, message, page, search_messages};
    use crate::{Error, Sequence};

    #[tokio::test]
    async fn searches_by_a_compiled_expression() {
        let log = FakeLog::new(vec![page(vec![
            message(4, None, None, "shipped the release"),
            message(3, None, None, "we will ship later"),
        ])]);
        let hits = search_messages(&log, &SearchQuery::new("/^ship(ped)?/"))
            .await
            .expect("searches");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].sequence, Sequence(4));
    }

    #[tokio::test]
    async fn reports_an_expression_that_does_not_compile() {
        let log = FakeLog::new(vec![page(vec![message(2, None, None, "ship")])]);
        let error = search_messages(&log, &SearchQuery::new("/ship(/"))
            .await
            .expect_err("invalid");
        assert!(matches!(error, Error::InvalidPattern { .. }));
    }
}
