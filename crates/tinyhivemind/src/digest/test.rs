//! Unit tests for bounded, superseding channel compaction.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::{LogMessage, SessionAuthor, SessionFuture, SessionPage, SourceError};
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};
use tinyhivemind_core::aside::Audience;

#[derive(Debug)]
struct FakeLog {
    pages: Mutex<VecDeque<std::result::Result<SessionPage, SourceError>>>,
    calls: Arc<Mutex<usize>>,
}

impl FakeLog {
    fn new(pages: Vec<SessionPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into_iter().map(Ok).collect()),
            calls: Arc::default(),
        }
    }

    fn failing() -> Self {
        Self {
            pages: Mutex::new(VecDeque::from([Err(
                Box::new(io::Error::other("offline")) as SourceError
            )])),
            calls: Arc::default(),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().expect("calls lock")
    }
}

impl SessionLog for FakeLog {
    fn read_before(&self, _: Option<Sequence>, _: usize) -> SessionFuture<'_> {
        *self.calls.lock().expect("calls lock") += 1;
        Box::pin(async move {
            self.pages
                .lock()
                .expect("pages lock")
                .pop_front()
                .unwrap_or_else(|| Ok(SessionPage::default()))
        })
    }
}

/// A digester that answers with fixed text and records what it was asked.
struct FixedDigester {
    answer: String,
    seen: Mutex<Vec<DigestRequest>>,
}

impl FixedDigester {
    fn new(answer: &str) -> Self {
        Self {
            answer: answer.into(),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<DigestRequest> {
        self.seen.lock().expect("seen lock").clone()
    }
}

impl Digester for FixedDigester {
    fn digest<'a>(&'a self, request: &'a DigestRequest) -> DigestFuture<'a> {
        self.seen.lock().expect("seen lock").push(request.clone());
        Box::pin(async move { Ok(self.answer.clone()) })
    }
}

/// A digester that cannot answer at all.
struct BrokenDigester;

impl Digester for BrokenDigester {
    fn digest<'a>(&'a self, _: &'a DigestRequest) -> DigestFuture<'a> {
        Box::pin(async { Err(Box::new(io::Error::other("no model")) as BoxError) })
    }
}

fn engineering() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

fn raw(sequence: u64, content: &str, audience: Audience) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: Some("engineering".into()),
        parent: None,
        author: SessionAuthor::Agent {
            id: format!("agent-{sequence}"),
            label: format!("Agent {sequence}"),
        },
        content: content.into(),
        audience,
    }
}

fn page(messages: Vec<LogMessage>, next: Option<u64>) -> SessionPage {
    SessionPage {
        messages,
        next_before: next.map(Sequence),
    }
}

fn held(through: u64, text: &str) -> ChannelDigest {
    ChannelDigest {
        conversation: engineering(),
        through: Sequence(through),
        covered: through,
        generation: 1,
        text: text.into(),
    }
}

fn message(sequence: u64, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: format!("agent-{sequence}"),
            label: format!("Agent {sequence}"),
        },
        content: content.into(),
        audience: Audience::Desk,
        elided: None,
    }
}

fn request(through: u64, messages: Vec<SessionMessage>) -> DigestRequest {
    DigestRequest {
        conversation: engineering(),
        prior: None,
        messages,
        through: Sequence(through),
        budget_chars: 40,
        pinned: Vec::new(),
    }
}

#[test]
fn digest_values_pin_deterministic_wire_shapes() {
    let digest = held(60, "the room chose the sublinear rank");
    assert_eq!(
        serde_json::to_value(&digest).expect("serializes"),
        serde_json::json!({
            "conversation": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": null
            },
            "through": 60,
            "covered": 60,
            "generation": 1,
            "text": "the room chose the sublinear rank"
        })
    );
    assert_eq!(
        serde_json::from_value::<ChannelDigest>(serde_json::to_value(&digest).expect("serializes"))
            .expect("deserializes"),
        digest
    );
    assert_eq!(
        serde_json::to_value(DigestPlan::Fold {
            after: Some(Sequence(60)),
            through: Sequence(120),
        })
        .expect("serializes"),
        serde_json::json!({"type": "fold", "after": 60, "through": 120})
    );
    assert_eq!(
        serde_json::to_value(DigestRejection::TooLarge {
            limit: 10,
            actual: 11
        })
        .expect("serializes"),
        serde_json::json!({"type": "too_large", "limit": 10, "actual": 11})
    );
    assert_eq!(
        serde_json::to_value(DigestPolicy::DEFAULT).expect("serializes"),
        serde_json::json!({
            "keep_live": 30,
            "fold_after": 20,
            "input_limit": 60,
            "budget_chars": 4000,
            "fold_after_chars": 400_000
        })
    );
    assert_eq!(DigestPolicy::default(), DigestPolicy::DEFAULT);
}

#[test]
fn leaves_a_channel_shorter_than_the_live_tail_alone() {
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(12)), DigestPolicy::DEFAULT),
        DigestPlan::Current
    );
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(30)), DigestPolicy::DEFAULT),
        DigestPlan::Current
    );
}

#[test]
fn waits_for_slack_to_accumulate_behind_the_live_tail() {
    // Twenty rows behind the tail is the threshold, and it is not crossed by
    // reaching it: folding on every row would spend a call to move one message.
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(50)), DigestPolicy::DEFAULT),
        DigestPlan::Current
    );
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(51)), DigestPolicy::DEFAULT),
        DigestPlan::Fold {
            after: None,
            through: Sequence(21)
        }
    );
}

#[test]
fn advances_by_at_most_one_input_limit_per_fold() {
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(400)), DigestPolicy::DEFAULT),
        DigestPlan::Fold {
            after: None,
            through: Sequence(60)
        }
    );
    assert_eq!(
        plan_digest(
            Some(&held(60, "so far")),
            ChannelHead::at(Sequence(400)),
            DigestPolicy::DEFAULT
        ),
        DigestPlan::Fold {
            after: Some(Sequence(60)),
            through: Sequence(120)
        }
    );
}

#[test]
fn stops_folding_once_the_account_reaches_the_live_tail() {
    assert_eq!(
        plan_digest(
            Some(&held(370, "so far")),
            ChannelHead::at(Sequence(400)),
            DigestPolicy::DEFAULT
        ),
        DigestPlan::Current
    );
}

#[tokio::test]
async fn collects_only_desk_visible_rows_of_this_channel() {
    let log = FakeLog::new(vec![page(
        vec![
            raw(5, "fifth", Audience::Desk),
            raw(
                4,
                "private",
                Audience::Aside {
                    members: vec!["theory".into()],
                },
            ),
            raw(3, "   ", Audience::Desk),
            raw(2, "second", Audience::Desk),
            raw(1, "first", Audience::Desk),
        ],
        None,
    )]);
    let collected = collect_digest_input(&log, &engineering(), None, Sequence(5))
        .await
        .expect("collects");
    let sequences: Vec<u64> = collected.iter().map(|row| row.sequence.0).collect();
    assert_eq!(sequences, vec![1, 2, 5], "chronological, desk-visible only");
}

#[tokio::test]
async fn stops_at_the_bound_the_account_already_covers() {
    let log = FakeLog::new(vec![page(
        vec![
            raw(5, "fifth", Audience::Desk),
            raw(4, "fourth", Audience::Desk),
            raw(3, "third", Audience::Desk),
            raw(2, "second", Audience::Desk),
        ],
        Some(2),
    )]);
    let collected = collect_digest_input(&log, &engineering(), Some(Sequence(3)), Sequence(5))
        .await
        .expect("collects");
    let sequences: Vec<u64> = collected.iter().map(|row| row.sequence.0).collect();
    assert_eq!(sequences, vec![4, 5]);
    assert_eq!(log.calls(), 1, "one page was enough");
}

#[tokio::test]
async fn refuses_a_fold_whose_range_the_bounded_scan_cannot_reach() {
    // The scan is bounded, so a range wider than it leaves a hole in the
    // middle of the fold, and a fold with a hole is worse than none.
    let mut pages = Vec::new();
    let mut top = 2100_u64;
    while pages.len() < crate::SCAN_LIMIT / crate::PAGE_SIZE {
        let rows: Vec<LogMessage> = (0..crate::PAGE_SIZE as u64)
            .map(|step| raw(top - step, "said something", Audience::Desk))
            .collect();
        let oldest = top - crate::PAGE_SIZE as u64 + 1;
        pages.push(page(rows, Some(oldest)));
        top = oldest - 1;
    }
    let log = FakeLog::new(pages);
    let error = collect_digest_input(&log, &engineering(), Some(Sequence(1)), Sequence(2100))
        .await
        .expect_err("a hole is not foldable");
    assert!(
        matches!(error, Error::DigestGap { after, scanned }
            if after == Sequence(1) && scanned == crate::SCAN_LIMIT),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn folds_what_the_channel_still_holds_when_older_rows_are_gone() {
    // The log ends above the floor. Everything the channel holds above it was
    // collected, and a fold is over what a channel holds.
    let log = FakeLog::new(vec![page(
        vec![
            raw(9, "ninth", Audience::Desk),
            raw(8, "eighth", Audience::Desk),
        ],
        None,
    )]);
    let collected = collect_digest_input(&log, &engineering(), Some(Sequence(2)), Sequence(9))
        .await
        .expect("collects what is there");
    assert_eq!(
        collected
            .iter()
            .map(|row| row.sequence.0)
            .collect::<Vec<_>>(),
        vec![8, 9]
    );
}

#[tokio::test]
async fn reports_a_host_read_failure() {
    let log = FakeLog::failing();
    let error = collect_digest_input(&log, &engineering(), Some(Sequence(1)), Sequence(9))
        .await
        .expect_err("the host failed");
    assert!(matches!(error, Error::Read { .. }), "{error:?}");
}

#[test]
fn accepts_an_account_that_covers_strictly_more() {
    let accepted = accept_digest(
        Some(&held(10, "older")),
        &DigestRequest {
            through: Sequence(20),
            ..request(20, vec![message(11, "a"), message(12, "b")])
        },
        "  the room verified B(x,n)  ",
    )
    .expect("accepted");
    assert_eq!(accepted.text, "the room verified B(x,n)");
    assert_eq!(accepted.through, Sequence(20));
    assert_eq!(accepted.covered, 12, "the older account's coverage carries");
    assert_eq!(accepted.generation, 2);
}

#[test]
fn refuses_an_empty_account() {
    assert_eq!(
        accept_digest(None, &request(20, vec![message(1, "a")]), " \n "),
        Err(DigestRejection::Empty)
    );
}

#[test]
fn refuses_an_account_that_overruns_its_budget() {
    let text = "x".repeat(41);
    assert_eq!(
        accept_digest(None, &request(20, vec![message(1, "a")]), &text),
        Err(DigestRejection::TooLarge {
            limit: 40,
            actual: 41
        })
    );
}

#[test]
fn refuses_an_account_that_covers_no_more_than_the_one_it_replaces() {
    assert_eq!(
        accept_digest(
            Some(&held(20, "older")),
            &request(20, vec![message(1, "a")]),
            "same ground"
        ),
        Err(DigestRejection::Regressed {
            through: Sequence(20),
            held: Sequence(20)
        })
    );
}

#[tokio::test]
async fn folds_a_long_channel_and_hands_the_digester_the_prior_account() {
    let rows: Vec<LogMessage> = (1..=61)
        .rev()
        .map(|sequence| raw(sequence, "said something", Audience::Desk))
        .collect();
    let log = FakeLog::new(vec![page(rows, None)]);
    let digester = FixedDigester::new("the room established the recurrence");
    let outcome = refold(
        &log,
        Some(&digester),
        &engineering(),
        Some(&held(1, "the brief")),
        ChannelHead::at(Sequence(400)),
        &[],
        DigestPolicy::DEFAULT,
    )
    .await
    .expect("folds");
    let DigestOutcome::Folded(digest) = outcome else {
        panic!("expected a fold, got {outcome:?}");
    };
    assert_eq!(digest.through, Sequence(61));
    assert_eq!(digest.generation, 2);
    assert_eq!(digest.text, "the room established the recurrence");
    let seen = digester.seen();
    assert_eq!(seen.len(), 1, "one call per fold");
    assert_eq!(seen[0].prior.as_deref(), Some("the brief"));
    assert_eq!(seen[0].messages.len(), 60);
    assert_eq!(seen[0].budget_chars, 4000);
}

#[tokio::test]
async fn does_not_call_a_digester_for_a_channel_that_is_current() {
    let log = FakeLog::new(vec![]);
    let digester = FixedDigester::new("unused");
    assert_eq!(
        refold(
            &log,
            Some(&digester),
            &engineering(),
            None,
            ChannelHead::at(Sequence(12)),
            &[],
            DigestPolicy::DEFAULT
        )
        .await
        .expect("plans"),
        DigestOutcome::Current
    );
    assert!(digester.seen().is_empty());
    assert_eq!(log.calls(), 0, "a current channel is not even read");
}

#[tokio::test]
async fn treats_a_missing_or_failing_digester_as_a_lost_optimization() {
    let rows: Vec<LogMessage> = (1..=60)
        .rev()
        .map(|sequence| raw(sequence, "said something", Audience::Desk))
        .collect();
    let log = FakeLog::new(vec![page(rows.clone(), None)]);
    assert_eq!(
        refold(
            &log,
            None,
            &engineering(),
            None,
            ChannelHead::at(Sequence(400)),
            &[],
            DigestPolicy::DEFAULT
        )
        .await
        .expect("plans"),
        DigestOutcome::Unavailable
    );
    let log = FakeLog::new(vec![page(rows, None)]);
    assert_eq!(
        refold(
            &log,
            Some(&BrokenDigester),
            &engineering(),
            None,
            ChannelHead::at(Sequence(400)),
            &[],
            DigestPolicy::DEFAULT
        )
        .await
        .expect("plans"),
        DigestOutcome::Unavailable
    );
}

#[tokio::test]
async fn reports_a_rejected_answer_rather_than_committing_it() {
    let rows: Vec<LogMessage> = (1..=60)
        .rev()
        .map(|sequence| raw(sequence, "said something", Audience::Desk))
        .collect();
    let log = FakeLog::new(vec![page(rows, None)]);
    let outcome = refold(
        &log,
        Some(&FixedDigester::new("   ")),
        &engineering(),
        None,
        ChannelHead::at(Sequence(400)),
        &[],
        DigestPolicy::DEFAULT,
    )
    .await
    .expect("plans");
    assert_eq!(
        outcome,
        DigestOutcome::Rejected {
            reason: DigestRejection::Empty
        }
    );
}

#[tokio::test]
async fn advances_an_account_over_a_step_with_nothing_to_say() {
    // Every row in the step was private. The account has nothing to add but
    // must still move, or the same empty step is planned forever.
    let rows: Vec<LogMessage> = (2..=61)
        .rev()
        .map(|sequence| {
            raw(
                sequence,
                "private",
                Audience::Aside {
                    members: vec!["theory".into()],
                },
            )
        })
        .collect();
    let log = FakeLog::new(vec![page(rows, None)]);
    let digester = FixedDigester::new("unused");
    let outcome = refold(
        &log,
        Some(&digester),
        &engineering(),
        Some(&held(1, "the brief")),
        ChannelHead::at(Sequence(400)),
        &[],
        DigestPolicy::DEFAULT,
    )
    .await
    .expect("plans");
    let DigestOutcome::Folded(digest) = outcome else {
        panic!("expected the account to advance, got {outcome:?}");
    };
    assert_eq!(digest.through, Sequence(61));
    assert_eq!(
        digest.text, "the brief",
        "unchanged, because nothing was said"
    );
    assert!(digester.seen().is_empty(), "no call was worth making");
}

#[tokio::test]
async fn leaves_an_empty_first_step_unfolded() {
    let rows: Vec<LogMessage> = (1..=60)
        .rev()
        .map(|sequence| {
            raw(
                sequence,
                "private",
                Audience::Aside {
                    members: vec!["theory".into()],
                },
            )
        })
        .collect();
    let log = FakeLog::new(vec![page(rows, None)]);
    assert_eq!(
        refold(
            &log,
            Some(&FixedDigester::new("unused")),
            &engineering(),
            None,
            ChannelHead::at(Sequence(400)),
            &[],
            DigestPolicy::DEFAULT
        )
        .await
        .expect("plans"),
        DigestOutcome::Current
    );
}

#[tokio::test]
async fn refuses_to_fold_one_channel_into_another_channel_s_account() {
    let log = FakeLog::new(vec![]);
    let other = Conversation {
        desk_id: "research".into(),
        desk_name: "Research".into(),
        thread_root: None,
    };
    let error = refold(
        &log,
        Some(&FixedDigester::new("unused")),
        &other,
        Some(&held(60, "engineering's account")),
        ChannelHead::at(Sequence(400)),
        &[],
        DigestPolicy::DEFAULT,
    )
    .await
    .expect_err("channels do not share an account");
    assert!(
        matches!(
            error,
            Error::DigestConversationChanged { ref held, ref requested }
                if held == "engineering" && requested == "research"
        ),
        "{error:?}"
    );
}

#[test]
fn composes_a_history_from_the_account_and_the_rows_it_does_not_cover() {
    let messages = vec![
        message(9, "old"),
        message(10, "boundary"),
        message(11, "new"),
    ];
    let composed = apply_digest(Some(&held(10, "what came before")), &messages);
    assert_eq!(composed.digest.as_deref(), Some("what came before"));
    assert_eq!(composed.covered_through, Some(Sequence(10)));
    assert_eq!(
        composed
            .messages
            .iter()
            .map(|row| row.sequence.0)
            .collect::<Vec<_>>(),
        vec![11],
        "a covered row is not also shown in full"
    );
}

#[test]
fn passes_a_history_through_untouched_when_there_is_no_account() {
    let messages = vec![message(9, "old"), message(11, "new")];
    let composed = apply_digest(None, &messages);
    assert_eq!(composed.digest, None);
    assert_eq!(composed.covered_through, None);
    assert_eq!(composed.messages, messages);
}

#[test]
fn folds_a_short_channel_that_has_grown_expensive() {
    // Twenty-four rows is four past the live tail and inside `fold_after`, so
    // the row trigger declines. On a desk writing derivations that is already
    // more scrollback than a seat should be handed.
    let policy = DigestPolicy {
        fold_after_chars: 50_000,
        ..DigestPolicy::DEFAULT
    };
    let short = ChannelHead {
        sequence: Sequence(34),
        unfolded_chars: 50_000,
    };
    assert_eq!(
        plan_digest(None, short, policy),
        DigestPlan::Current,
        "at the threshold is not past it",
    );
    let grown = ChannelHead {
        sequence: Sequence(34),
        unfolded_chars: 50_001,
    };
    assert_eq!(
        plan_digest(None, grown, policy),
        DigestPlan::Fold {
            after: None,
            through: Sequence(4),
        },
        "either threshold is enough, and the step is bounded as always",
    );
}

#[test]
fn a_channel_of_short_rows_still_folds_on_the_row_count_alone() {
    let policy = DigestPolicy {
        fold_after_chars: 0,
        ..DigestPolicy::DEFAULT
    };
    assert_eq!(
        plan_digest(
            None,
            ChannelHead {
                sequence: Sequence(400),
                unfolded_chars: usize::MAX,
            },
            policy,
        ),
        DigestPlan::Fold {
            after: None,
            through: Sequence(60),
        },
        "zero disables the size trigger however large the channel is",
    );
}

#[test]
fn a_live_tail_that_is_large_on_its_own_is_never_folded() {
    // The tail is delivered verbatim, always. A room whose `keep_live` rows
    // alone exceed the budget has a `keep_live` set too large, and that is the
    // host's error to make rather than one compaction can fix.
    assert_eq!(
        plan_digest(
            None,
            ChannelHead {
                sequence: Sequence(30),
                unfolded_chars: 10_000_000,
            },
            DigestPolicy::from_token_budget(1),
        ),
        DigestPlan::Current,
    );
}

#[test]
fn an_account_that_already_covers_the_channel_is_not_refolded_by_size() {
    let held = held(370, "so far");
    assert_eq!(
        plan_digest(
            Some(&held),
            ChannelHead {
                sequence: Sequence(400),
                unfolded_chars: usize::MAX,
            },
            DigestPolicy::DEFAULT,
        ),
        DigestPlan::Current,
        "there is nothing above the tail left to fold, whatever it weighs",
    );
}

#[test]
fn a_token_budget_becomes_a_character_threshold_and_nothing_else() {
    let policy = DigestPolicy::from_token_budget(50_000);
    assert_eq!(
        policy.fold_after_chars,
        50_000 * DigestPolicy::CHARS_PER_TOKEN,
    );
    assert_eq!(policy.keep_live, DigestPolicy::DEFAULT.keep_live);
    assert_eq!(policy.fold_after, DigestPolicy::DEFAULT.fold_after);
    assert_eq!(policy.input_limit, DigestPolicy::DEFAULT.input_limit);
    assert_eq!(policy.budget_chars, DigestPolicy::DEFAULT.budget_chars);
    assert_eq!(
        DigestPolicy::from_token_budget(usize::MAX).fold_after_chars,
        usize::MAX,
        "a budget nobody could spend saturates rather than wrapping to nothing",
    );
    assert_eq!(
        DigestPolicy::from_token_budget(0).fold_after_chars,
        0,
        "no budget is the disabled state, not a fold on every turn",
    );
}

#[test]
fn a_head_with_no_count_plans_exactly_as_it_did_before() {
    for sequence in [12_u64, 30, 50, 51, 400] {
        assert_eq!(
            plan_digest(
                None,
                ChannelHead::at(Sequence(sequence)),
                DigestPolicy::DEFAULT
            ),
            plan_digest(
                None,
                ChannelHead {
                    sequence: Sequence(sequence),
                    unfolded_chars: 0,
                },
                DigestPolicy::DEFAULT,
            ),
        );
    }
}

/// One pinned row, as the host's board reports it.
fn pin(sequence: u64) -> Pin {
    Pin {
        sequence: Sequence(sequence),
        pinned_at: Sequence(sequence + 1),
        pinned_by: SessionAuthor::Agent {
            id: "lead".into(),
            label: "Lead".into(),
        },
        label: None,
        note: None,
        excerpt: None,
    }
}

/// Sixty desk rows, newest first, the way a log pages them back.
///
/// Sixty is `input_limit`, so a first fold with no prior account covers all of
/// them in one step and the page ends exactly where the walk stops.
fn a_long_channel() -> Vec<LogMessage> {
    (1..=60)
        .rev()
        .map(|sequence| raw(sequence, "said something", Audience::Desk))
        .collect()
}

#[tokio::test]
async fn the_fold_is_told_which_of_its_rows_the_room_pinned() {
    let log = FakeLog::new(vec![page(a_long_channel(), None)]);
    let digester = FixedDigester::new("an account");
    let board = [
        pin(3),
        pin(9),
        // Past what this step reaches: still a row the reader is handed in
        // full, so the fold is not answerable for it.
        pin(70),
        // Named twice by two markers, and named once here.
        pin(3),
    ];
    refold(
        &log,
        Some(&digester),
        &engineering(),
        None,
        ChannelHead::at(Sequence(400)),
        &board,
        DigestPolicy::DEFAULT,
    )
    .await
    .expect("folds");
    let seen = digester.seen();
    assert_eq!(
        seen[0].pinned,
        vec![Sequence(3), Sequence(9)],
        "ascending, deduplicated, and nothing above `through`",
    );
}

#[tokio::test]
async fn a_fold_with_no_pins_is_told_so_rather_than_guessing() {
    let log = FakeLog::new(vec![page(a_long_channel(), None)]);
    let digester = FixedDigester::new("an account");
    refold(
        &log,
        Some(&digester),
        &engineering(),
        None,
        ChannelHead::at(Sequence(400)),
        &[],
        DigestPolicy::DEFAULT,
    )
    .await
    .expect("folds");
    assert!(digester.seen()[0].pinned.is_empty());
}
