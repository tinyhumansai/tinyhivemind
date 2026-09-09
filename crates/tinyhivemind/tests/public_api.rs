//! Public API regression tests for the runtime crate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyhivemind::aside::Audience;
use tinyhivemind::aside::Viewer;
use tinyhivemind::{
    ChannelHead,
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
        audience: Audience::Aside {
            members: vec!["linus".into()],
        },
        elided: Some(tinyhivemind::Elision {
            through: Sequence(4),
            messages: 1,
            settled_at: None,
        }),
    };
    assert_eq!(conversation.thread_root, Some(Sequence(3)));
    assert_eq!(message.sequence, Sequence(4));
    assert_eq!(
        message.audience,
        Audience::Aside {
            members: vec!["linus".into()]
        }
    );
    assert_eq!(
        message.elided.as_ref().map(|elision| elision.messages),
        Some(1)
    );
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

    let query = SearchQuery::new("/^ship/", Viewer::Operator)
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
    assert_eq!(query.viewer, Viewer::Operator);
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

#[tokio::test]
async fn the_search_viewer_argument_narrows_what_is_matched() {
    use std::{pin::Pin, sync::Mutex};
    use tinyhivemind::{LogMessage, SearchQuery, search_messages};

    struct FixedLog(Mutex<Vec<LogMessage>>);

    impl tinyhivemind::SessionLog for FixedLog {
        fn read_before(
            &self,
            _before: Option<Sequence>,
            _limit: usize,
        ) -> tinyhivemind::SessionFuture<'_> {
            let messages = std::mem::take(&mut *self.0.lock().expect("log lock is not poisoned"));
            Box::pin(async move {
                Ok(tinyhivemind::SessionPage {
                    messages,
                    next_before: None,
                })
            }) as Pin<Box<_>>
        }
    }

    let row = LogMessage {
        sequence: Sequence(1),
        chat_id: None,
        parent: None,
        author: SessionAuthor::Agent {
            id: "ada".into(),
            label: "Ada".into(),
        },
        content: "we should ship the migration tonight".into(),
        audience: Audience::Aside {
            members: vec!["linus".into()],
        },
    };
    let log = FixedLog(Mutex::new(vec![row.clone()]));

    let outsider_hits = search_messages(&log, &SearchQuery::new("ship", Viewer::Operator))
        .await
        .expect("outsider search succeeds");
    // The row was already consumed by the outsider search above, so reseed it
    // for the member search — the point under test is what each viewer's own
    // call returns, not a shared cursor.
    *log.0.lock().expect("log lock is not poisoned") = vec![row];
    let member_hits = search_messages(
        &log,
        &SearchQuery::new("ship", Viewer::Agent { id: "linus".into() }),
    )
    .await
    .expect("member search succeeds");

    assert!(
        !outsider_hits.is_empty(),
        "the operator viewer reads every audience"
    );
    assert!(
        !member_hits.is_empty(),
        "an addressed member reads its own aside"
    );

    *log.0.lock().expect("log lock is not poisoned") = vec![LogMessage {
        sequence: Sequence(1),
        chat_id: None,
        parent: None,
        author: SessionAuthor::Agent {
            id: "ada".into(),
            label: "Ada".into(),
        },
        content: "we should ship the migration tonight".into(),
        audience: Audience::Aside {
            members: vec!["linus".into()],
        },
    }];
    let excluded_hits = search_messages(
        &log,
        &SearchQuery::new("ship", Viewer::Agent { id: "grace".into() }),
    )
    .await
    .expect("outsider agent search succeeds");
    assert!(
        excluded_hits.is_empty(),
        "an agent outside the aside's audience must not match its content"
    );
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
            audience: Audience::Desk,
        },
        LogMessage {
            sequence: Sequence(2),
            chat_id: None,
            parent: None,
            author: SessionAuthor::Agent {
                id: "ada".into(),
                label: "Ada".into(),
            },
            content: "!pin ^1 #limits keep this".into(),
            audience: Audience::Aside {
                members: vec!["linus".into()],
            },
        },
    ];
    let board = fold_pins(&rows, &Viewer::Operator, PIN_LIMIT);
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

    // The `!pin` marker is inside an aside addressed to `linus`, not to the
    // desk: an outsider's board never sees a directive it could not read, and
    // an addressed member's does — proving the `Viewer` argument is not
    // ignored.
    let outsider_board = fold_pins(&rows, &Viewer::Agent { id: "grace".into() }, PIN_LIMIT);
    assert!(
        outsider_board.is_empty(),
        "an outsider's aside directive never touches the pin board"
    );
    let member_board = fold_pins(&rows, &Viewer::Agent { id: "linus".into() }, PIN_LIMIT);
    assert_eq!(member_board[0].sequence, Sequence(1));
    assert_eq!(member_board[0].label.as_deref(), Some("limits"));
}

#[test]
fn root_exports_the_brevity_policy_stated_in_a_briefing() {
    use tinyhivemind::BrevityPolicy;

    assert_eq!(BrevityPolicy::DEFAULT.window, SESSION_WINDOW);
    assert_eq!(BrevityPolicy::DEFAULT.overrun("short"), None);
    assert!(BrevityPolicy::DEFAULT.rule_text().contains("600"));
}

#[tokio::test]
async fn the_referral_queue_port_is_available_to_consumers() {
    use std::sync::Mutex;
    use tinyhivemind::{
        BoxError, Referral, ReferralFuture, ReferralInput, ReferralOutcome, ReferralPolicy,
        ReferralQueue, ReferralReach,
        desk::{Desk, DeskSet, ResponderMode},
        dispatch::{DispatchConversation, DispatchKey},
        mention::{Mention, MentionTarget},
        roster::{Roster, RosterMember},
    };

    /// The shape of a host: one atomic transaction, keyed by the conversation
    /// the trigger was committed on plus its sequence.
    #[derive(Default)]
    struct Queue {
        enqueued: Mutex<Vec<Referral>>,
    }

    impl ReferralQueue for Queue {
        fn enqueue_once(&self, referral: Referral) -> ReferralFuture<'_> {
            Box::pin(async move {
                let mut enqueued = self.enqueued.lock().unwrap();
                if enqueued
                    .iter()
                    .any(|held| held.from == referral.from && held.key == referral.key)
                {
                    return Ok::<_, BoxError>(EnqueueOutcome::Already);
                }
                enqueued.push(referral);
                Ok(EnqueueOutcome::Enqueued)
            })
        }
    }

    let members = [
        RosterMember {
            id: "ada".into(),
            name: None,
        },
        RosterMember {
            id: "linus".into(),
            name: None,
        },
    ];
    let roster = Roster::new(&members, &[], &[]);
    let records = [
        Desk {
            id: "payments".into(),
            name: "Payments".into(),
            description: None,
            members: vec!["ada".into()],
            responder_mode: ResponderMode::Lead,
        },
        Desk {
            id: "platform".into(),
            name: "Platform".into(),
            description: None,
            members: vec!["linus".into()],
            responder_mode: ResponderMode::Lead,
        },
    ];
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let input = ReferralInput {
        key: DispatchKey {
            trigger_sequence: 7,
        },
        conversation: DispatchConversation {
            desk_id: "payments".into(),
            thread_root: None,
        },
        author_id: "ada".into(),
        content: "@linus can you look?".into(),
        mentions: vec![Mention {
            target: MentionTarget::Agent { id: "linus".into() },
            text: "@linus".into(),
            offset: 0,
            quiet: false,
        }],
        hop: 0,
        origin: None,
    };

    let queue = Queue::default();
    let policy = ReferralPolicy {
        enabled: true,
        max_hops: 2,
        reach: ReferralReach::Channels,
        returns: true,
    };
    let outcome = tinyhivemind::dispatch_referral(&queue, policy, &input, &roster, &desks)
        .await
        .expect("well formed");
    assert_eq!(outcome, ReferralOutcome::Referred { crossed: true });
    assert_eq!(queue.enqueued.lock().unwrap()[0].to.desk_id, "platform");

    // The same trigger twice creates one child turn, not two.
    let again = tinyhivemind::dispatch_referral(&queue, policy, &input, &roster, &desks)
        .await
        .expect("well formed");
    assert_eq!(again, ReferralOutcome::Already);
    assert_eq!(queue.enqueued.lock().unwrap().len(), 1);

    // And the conservative default never reaches the queue at all.
    let quiet =
        tinyhivemind::dispatch_referral(&queue, ReferralPolicy::DEFAULT, &input, &roster, &desks)
            .await
            .expect("well formed");
    assert!(matches!(quiet, ReferralOutcome::NotReferred { .. }));
    assert_eq!(queue.enqueued.lock().unwrap().len(), 1);
}

#[test]
fn root_exports_channel_compaction() {
    use tinyhivemind::{
        ChannelDigest, DigestPlan, DigestPolicy, DigestRejection, DigestRequest, accept_digest,
        apply_digest, plan_digest,
    };

    let conversation = Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    };
    let policy = DigestPolicy::DEFAULT;

    // A short channel is left alone; a long one folds in bounded steps.
    assert_eq!(plan_digest(None, ChannelHead::at(Sequence(20)), policy), DigestPlan::Current);
    assert_eq!(
        plan_digest(None, ChannelHead::at(Sequence(400)), policy),
        DigestPlan::Fold {
            after: None,
            through: Sequence(60),
        }
    );

    let folded = SessionMessage {
        sequence: Sequence(60),
        author: SessionAuthor::Agent {
            id: "solver".into(),
            label: "Solver".into(),
        },
        content: "B(g,10^18) = 79414112".into(),
        audience: Audience::Desk,
        elided: None,
    };
    let request = DigestRequest {
        conversation: conversation.clone(),
        prior: None,
        messages: vec![folded.clone()],
        through: Sequence(60),
        budget_chars: 64,
    };
    let account = accept_digest(None, &request, "the room verified B at 10^18").expect("accepted");
    assert_eq!(account.through, Sequence(60));
    assert_eq!(account.generation, 1);
    assert_eq!(account.covered, 1);

    // An account that covers no more than the one it replaces is refused, and
    // that refusal is not a crate error: the held account still stands.
    assert_eq!(
        accept_digest(Some(&account), &request, "same ground"),
        Err(DigestRejection::Regressed {
            through: Sequence(60),
            held: Sequence(60),
        })
    );

    // A turn is composed of the account plus the rows it does not cover.
    let live = SessionMessage {
        sequence: Sequence(61),
        ..folded.clone()
    };
    let history = apply_digest(Some(&account), &[folded, live.clone()]);
    assert_eq!(
        history.digest.as_deref(),
        Some("the room verified B at 10^18")
    );
    assert_eq!(history.covered_through, Some(Sequence(60)));
    assert_eq!(history.messages, vec![live]);

    let stored: ChannelDigest =
        serde_json::from_str(&serde_json::to_string(&account).expect("serializes"))
            .expect("deserializes");
    assert_eq!(stored, account);
    assert_eq!(stored.conversation, conversation);
}
