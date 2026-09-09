//! The desk loop: open the room, choose who answers, run a turn, post it,
//! route the reply — until the room closes.
//!
//! One function on purpose. This is the host written out as one story — open
//! the desk, choose who answers, run a turn, post it, route the reply — and
//! the value of an example host is that a reader can follow that order
//! without chasing five helpers. `crosstalk` makes the same trade with
//! `too_many_arguments`.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    sync::PoisonError,
    time::Duration,
};

use tinyhivemind::{
    BrevityPolicy, BriefedTeammate, ChannelDigest, ChannelHead, Conversation, DigestOutcome,
    DigestPolicy, Digester, LogMessage, MentionDispatchContext, MentionDispatchOutcome, Sequence,
    SessionAuthor, SessionQuery, TeamBriefing, apply_digest,
    aside::{Audience, Viewer},
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        dispatch_mention,
    },
    initialize_session,
    mention::{MentionAuthor, resolve},
    pins::{PIN_LIMIT, fold_pins},
    refold,
    responder::{ResponderRequest, SelectionPolicy, choose_responder},
    sharing::{SharingPlan, SharingQuery, SharingState, initialized_state, prepare_delta},
    speech::{CommitRequest, addressed_peers, commit_utterance},
};

use crate::{
    BoxError, agent, aside, chat,
    cli::Options,
    deskfile, digest, log, mcp, memory,
    notebook::{files_written, read_notebook},
    prompt::{TurnPrompt, compose_prompt},
    queue::{DeskQueue, PendingTurn},
    room, turn,
};

/// How long a seat gets to write the message it never got round to writing.
const WRAP_UP_TIMEOUT: Duration = Duration::from_secs(600);

/// Where a turn's tool calls to the room are collected, under the workspace.
///
/// One file, truncated before every turn, so a turn drains only its own calls.
/// It is not a second journal: the transcript is still the only record, and
/// the host is still what writes it.
const OUTBOX: &str = ".desk/outbox.jsonl";

/// Where the host says whose turn is running, for the server serving it.
const TURN: &str = ".desk/turn";

/// How long the room's standing account may be, in characters.
///
/// Roughly a page: enough to carry what has been established and by whom, and
/// small enough that it never competes with the live conversation for the
/// seat's attention.
const ACCOUNT_CHARS: usize = 4000;

// One function on purpose. This is the host written out as one story — open the
// desk, choose who answers, run a turn, post it, route the reply — and the
// value of an example host is that a reader can follow that order without
// chasing five helpers. `crosstalk` makes the same trade with
// `too_many_arguments`.
#[allow(clippy::too_many_lines)]
pub(crate) async fn run(options: Options) -> Result<(), BoxError> {
    let spec = deskfile::parse(&fs::read_to_string(&options.desk)?)?;
    fs::create_dir_all(&options.workspace)?;

    let room = room::Room::new(&spec);
    let roster = room.roster();
    let desks = room.desks();
    roster.validate()?;
    desks.validate()?;

    let conversation = Conversation {
        desk_id: spec.id.clone(),
        desk_name: spec.name.clone(),
        thread_root: None,
    };
    let transcript = log::JsonlLog::open(&options.transcript)?;
    // The room is a tool the seat calls, not a fence it writes. The server is
    // this binary re-executed; the outbox is one file, truncated per turn.
    let serving = mcp::Serving {
        outbox: options.workspace.join(OUTBOX),
        transcript: options.transcript.clone(),
        desk: Some(options.desk.clone()),
        turn: Some(options.workspace.join(TURN)),
    };
    let outbox = serving.outbox.clone();
    let agent_config = match std::env::current_exe() {
        Ok(exe) => Some(mcp::config_block(
            &exe,
            &serving,
            options.opencode_config.as_deref(),
        )),
        Err(error) => {
            println!(
                "!! cannot find this binary to serve the desk tools ({error}); seats will fall \
                 back to the post fence"
            );
            options.opencode_config.clone()
        }
    };
    let runner = agent::AgentRunner::new(
        &options.agent_cmd,
        &options.workspace.to_string_lossy(),
        agent_config,
        options.timeout,
        Some(
            options
                .transcript
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("desk-raw"),
        ),
    );
    let store = match (&options.cortex_base, &options.cortex_key) {
        (Some(base), Some(key)) => Some(memory::Memory::new(
            base,
            key,
            &options.library_scope,
            &options.session_scope,
            Duration::from_secs(120),
        )),
        _ => None,
    };
    let wrapup = chat::Chat::new(
        &options.router_base,
        &options.router_key,
        &options.router_model,
        WRAP_UP_TIMEOUT,
    );
    // The room's own memory. Everything older than the live window is one
    // bounded account, rewritten as the room moves, so a seat spends its window
    // on the live conversation rather than on its own scrollback.
    let folder = options.fold_account.then(|| {
        digest::RoomDigester::new(chat::Chat::new(
            &options.router_base,
            &options.router_key,
            &options.router_model,
            WRAP_UP_TIMEOUT,
        ))
    });
    // Two triggers. `fold_after` says the room has *moved*; `--fold-tokens`
    // says its scrollback has grown expensive, and on this desk the second
    // arrives long before the first — run 28 solved PE 1006 in 23 rows and
    // 604k tokens, which is a fold the row count would never have planned.
    let account_policy = DigestPolicy {
        keep_live: options.window,
        fold_after: options.fold_after,
        budget_chars: ACCOUNT_CHARS,
        ..DigestPolicy::from_token_budget(options.fold_tokens)
    };
    let mut account: Option<ChannelDigest> = None;
    // One CLI session per seat, and one watermark per seat: a seat that has
    // spoken before is caught up with `prepare_delta` rather than re-read the
    // whole window it already holds.
    let mut sessions: HashMap<String, String> = HashMap::new();
    let mut shared: HashMap<String, SharingState> = HashMap::new();
    let queue = DeskQueue::default();
    let policy = MentionDispatchPolicy {
        enabled: true,
        max_hops: options.max_hops,
    };

    // The chair opens the room. A person's message cannot dispatch a turn —
    // only an agent reply can — so the responder ladder chooses who answers it.
    //
    // Appended once. A resumed transcript already opens with it, and every
    // seat's prompt carries the standing brief regardless of what scrolled,
    // so appending it again spends a window row on a thing that was already
    // unavoidable — by run 26 five copies filled half of every seat's window.
    let brief = fs::read_to_string(&options.task)?;
    let mut sequence = if transcript.len() == 0 {
        let sequence = transcript.append(
            Some(spec.id.clone()),
            SessionAuthor::Person {
                id: spec.person_id.clone(),
                label: spec.person_label.clone(),
            },
            &brief,
            Audience::Desk,
        )?;
        println!("[{sequence:?}] {} opened the desk", spec.person_label);
        sequence
    } else {
        let sequence = Sequence(transcript.len() as u64);
        println!("[{sequence:?}] resumed; the brief is already the opening row");
        sequence
    };

    let opening_mentions = resolve(
        &brief,
        None,
        &MentionAuthor::Person {
            id: spec.person_id.clone(),
        },
        &roster,
        &desks,
    );
    let decision = choose_responder(
        None,
        &ResponderRequest {
            message: brief.clone(),
            chat: Some(spec.id.clone()),
            mentions: opening_mentions,
            orchestrator_id: spec.agents[0].id.clone(),
            selection_policy: SelectionPolicy::Disabled,
        },
        &roster,
        &desks,
        &[],
    )
    .await?;
    println!(
        "  responder ladder: {} via {:?}",
        decision.responder_id, decision.rung
    );
    queue
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push_back(PendingTurn {
            target_id: decision.responder_id,
            trigger: brief.clone(),
            hop: 0,
        });

    let mut turns = 0_usize;
    let mut rounds = 0_usize;
    let mut next_seat = 0_usize;
    let mut since_chair = 0_usize;
    let mut tokens = 0_u64;
    while turns < options.max_turns {
        // The chair speaks on a cadence, not only when the room falls silent.
        // A chain of two seats naming each other never goes quiet, so a purely
        // reactive chair never gets a word in — and this desk spent six turns
        // refining a settled fact while the one open implementation step went
        // unbuilt, with nobody whose job it was to say so.
        let due = options.chair_every > 0 && since_chair >= options.chair_every;
        let job = if due {
            None
        } else {
            queue
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop_front()
        };
        let job = if let Some(job) = job {
            job
        } else {
            {
                // The chain stopped. The chair either closes the desk or nudges
                // somebody: whose turn it is when nobody was mentioned is a host
                // policy, not something the library decides.
                //
                // It goes back to whoever spoke last, not round-robin to the
                // next seat. A chain dies most often because a seat ran out of
                // time mid-task and its landed message named nobody — and that
                // seat is exactly the one holding the unfinished work. Handing
                // the turn to a different seat there costs a full turn of
                // re-orientation and loses the thread. Round-robin is the
                // fallback for a room where nobody has spoken yet.
                if rounds >= options.rounds {
                    println!("-- chain empty and rounds exhausted; closing the desk");
                    break;
                }
                rounds += 1;
                since_chair = 0;
                let spoken: Vec<String> = transcript
                    .rows()
                    .into_iter()
                    .filter_map(|row| match row.author {
                        SessionAuthor::Agent { id, .. } => Some(id),
                        _ => None,
                    })
                    .collect();
                // Alternate. Even rounds go back to whoever spoke last, because
                // a chain usually dies with that seat holding the unfinished
                // work. Odd rounds go to a seat that has never spoken, because
                // one-message-one-turn lets a pair that keeps naming each other
                // run a four-seat desk between them: over sixteen turns here,
                // two seats spoke and the other two never did. A desk whose
                // checker never checks is not a desk.
                let starved = spec
                    .agents
                    .iter()
                    .find(|seat| !spoken.iter().any(|who| who == &seat.id));
                let seat = match (rounds % 2, starved) {
                    (1, Some(seat)) => seat,
                    _ => spoken
                        .last()
                        .and_then(|id| spec.agent(id))
                        .unwrap_or_else(|| {
                            let seat = &spec.agents[next_seat % spec.agents.len()];
                            next_seat += 1;
                            seat
                        }),
                };
                let nudge = format!(
                    "@{} the room has gone quiet — round {rounds}. Read NOTES.md and the \
                     transcript, then post one concrete step or result and name the seat \
                     you need next. If the room has been round the same loop twice, say so \
                     and change what it is doing.",
                    seat.id
                );
                sequence = transcript.append(
                    Some(spec.id.clone()),
                    SessionAuthor::Person {
                        id: spec.person_id.clone(),
                        label: spec.person_label.clone(),
                    },
                    &nudge,
                    Audience::Desk,
                )?;
                println!("[{}] chair nudge -> @{}", sequence.0, seat.id);
                PendingTurn {
                    target_id: seat.id.clone(),
                    trigger: nudge,
                    hop: 0,
                }
            }
        };

        let Some(seat) = spec.agent(&job.target_id) else {
            println!("!! no seat named {}", job.target_id);
            continue;
        };
        turns += 1;
        since_chair += 1;

        // Fold what has scrolled out of reach into the room's account, before
        // anything is composed from it. A fold that fails costs the room its
        // compaction and nothing else: the window is already correct without
        // one.
        // Two things the library cannot derive and this host can. The
        // character count is what the size trigger reads: only the host sees a
        // row's size as it appends it, and reading the log back every turn to
        // decide whether to compact it would cost more than the compaction
        // saves. The pins are the room's own claim that a message does not
        // scroll away, and a fold told nothing about them would quietly undo
        // one.
        let rows = transcript.rows();
        let head = ChannelHead {
            sequence: Sequence(transcript.len() as u64),
            unfolded_chars: unfolded_chars(&rows, account.as_ref(), options.window),
        };
        let board = fold_pins(&rows, &Viewer::Operator, PIN_LIMIT);
        match refold(
            &transcript,
            folder.as_ref().map(|folder| folder as &dyn Digester),
            &conversation,
            account.as_ref(),
            head,
            &board,
            account_policy,
        )
        .await?
        {
            DigestOutcome::Folded(next) => {
                println!(
                    "   room account: generation {} now covers {} messages through [{}] \
                     ({} chars, folded at {} unfolded chars)",
                    next.generation,
                    next.covered,
                    next.through.0,
                    next.text.chars().count(),
                    head.unfolded_chars,
                );
                account = Some(next);
            }
            DigestOutcome::Rejected { reason } => {
                println!("   !! the room account was refused: {reason:?}");
            }
            DigestOutcome::Unavailable => println!("   !! no folder; the room account stands"),
            DigestOutcome::Current => {}
        }

        let viewer = Viewer::Agent {
            id: seat.id.clone(),
        };
        let query = SessionQuery {
            conversation: conversation.clone(),
            before: None,
            window: options.window,
            viewer: viewer.clone(),
        };
        let briefing = TeamBriefing {
            viewer_id: seat.id.clone(),
            desk_id: spec.id.clone(),
            desk_name: spec.name.clone(),
            teammates: spec
                .agents
                .iter()
                .filter(|other| other.id != seat.id)
                .map(|other| BriefedTeammate {
                    id: other.id.clone(),
                    label: other.label.clone(),
                    role: Some(other.role.clone()),
                    description: None,
                })
                .collect(),
            brevity: BrevityPolicy::DEFAULT,
            asides: aside::ASIDES,
        };
        // Off by default, and that default is the finding. Resuming a seat's
        // CLI session looks like free continuity, but the session keeps every
        // prior turn: the request payload grows without bound until a single
        // call takes minutes and then never returns at all. The room got
        // slower turn by turn and finally stalled on every one. A fresh
        // session answers at once, and the context a seat actually needs is
        // the bounded projection this host already assembles plus the files
        // in the shared workspace — which is what the library's own
        // `prepare_delta` is for.
        let resumed = options
            .resume_sessions
            .then(|| sessions.get(&seat.id).cloned())
            .flatten();
        let plan = match (resumed.as_ref(), shared.get(&seat.id)) {
            (Some(_), Some(state)) => Some(
                prepare_delta(
                    &transcript,
                    &SharingQuery {
                        desired_conversation: &conversation,
                        current_conversation: &conversation,
                        state,
                        before: sequence,
                        viewer: &viewer,
                    },
                )
                .await?,
            ),
            _ => None,
        };
        let (window, briefing_text, catching_up) = if let Some(SharingPlan::Delta(delta)) = plan {
            shared.insert(seat.id.clone(), delta.next_state);
            (delta.messages, None, true)
        } else {
            let session = initialize_session(&transcript, &query, briefing).await?;
            shared.insert(
                seat.id.clone(),
                initialized_state(conversation.clone(), sequence),
            );
            // The dispatch-aware form, because this host knows both halves
            // the plain one has to withhold: the policy in force and the hop
            // this turn is at. Without it a seat is never told that naming a
            // teammate is what runs them next, which is the whole hand-off
            // mechanism — and it is told nothing at all when it is at the cap,
            // which is also correct.
            let text = session
                .briefing
                .system_text_with_dispatch(MentionDispatchContext {
                    policy,
                    hop: job.hop,
                });
            (session.history, Some(text), false)
        };
        let recalled = store
            .as_ref()
            .map(|store| store.recall(&job.trigger))
            .unwrap_or_default();

        // A row the account already stands for is not also shown in full, so
        // the window is spent on the live conversation. A seat that is only
        // being caught up already holds the older history in its own session,
        // so the account is not repeated to it.
        let composed = apply_digest(if catching_up { None } else { account.as_ref() }, &window);
        let history = composed.messages;
        let notebook = read_notebook(&options.workspace, &seat.id);
        // Who has spoken and when, folded from the transcript rather than
        // read out of the window: the window is bounded and a seat that fell
        // out of it is exactly the seat nobody thinks to call on.
        let spoken: BTreeMap<String, u64> = transcript
            .rows()
            .into_iter()
            .filter_map(|row| match row.author {
                SessionAuthor::Agent { id, .. } => Some((id, row.sequence.0)),
                _ => None,
            })
            .collect();
        let prompt = compose_prompt(&TurnPrompt {
            briefing: briefing_text.as_deref(),
            account: composed.digest.as_deref(),
            history: &history,
            seat,
            job: &job,
            recalled: &recalled,
            notebook: notebook.as_deref(),
            seats: &spec.agents,
            spoken: &spoken,
        });
        println!(
            "[turn {turns}] @{} ({} chars of prompt, {} {} message(s){}, notebook {})",
            seat.id,
            prompt.len(),
            history.len(),
            if catching_up { "new" } else { "of history" },
            if resumed.is_some() { ", resumed" } else { "" },
            match &notebook {
                Some(text) => format!("{} chars", text.chars().count()),
                None => "none".to_string(),
            }
        );
        // Say whose turn is running, so the server serving this seat's tools
        // can price a `desk_dm` against the aside policy while the seat is
        // still able to act on the answer.
        if let Some(path) = &serving.turn {
            mcp::open_turn(path, &seat.id);
        }
        let Some((output, said)) = turn::deliver(
            &turn::Delivery {
                runner: &runner,
                wrapup: &wrapup,
                outbox: &outbox,
                label: format!("turn-{turns:03}-{}", seat.id),
                seat_id: &seat.id,
                resumed: resumed.as_deref(),
            },
            &prompt,
            &mut sessions,
            &mut tokens,
        )
        .await?
        else {
            continue;
        };
        println!(
            "   {:?}, {} tokens, {} read(s), tools: {}",
            output.elapsed,
            output.tokens,
            output.reads,
            if output.tools.is_empty() {
                "none".to_string()
            } else {
                output.tools.join(",")
            }
        );
        for line in output.message.lines().take(6) {
            println!("   | {line}");
        }

        // What the seat said becomes a row through one fold in the library:
        // the text, who may read it, the mentions dispatch routes on, and
        // whether the desk was reported finished. The host's part is the two
        // counts the fold cannot derive — how much of an open aside is spent,
        // and whether an earlier one is still owed a settlement — both folded
        // out of the journal, because this host keeps no state the journal
        // does not already carry.
        let rows = transcript.rows();
        let committed = commit_utterance(&CommitRequest {
            utterance: &said.utterance,
            speaker_id: &seat.id,
            conversation: &DispatchConversation {
                desk_id: spec.id.clone(),
                thread_root: None,
            },
            aside: aside::ASIDES,
            spent: aside::spent_in_aside(&rows),
            unsettled: aside::unsettled_aside(
                &rows,
                &seat.id,
                &addressed_peers(&said.utterance, &seat.id, &roster, &desks),
            ),
            roster: &roster,
            desks: &desks,
        })?;
        if let Some(reason) = committed.refusal {
            println!("   aside refused: {reason:?} — the row stays desk-visible");
        }
        if let Audience::Aside { members } = &committed.audience {
            println!("   aside to @{}", members.join(", @"));
        }
        sequence = transcript.append(
            Some(spec.id.clone()),
            SessionAuthor::Agent {
                id: seat.id.clone(),
                label: seat.label.clone(),
            },
            &committed.content,
            committed.audience.clone(),
        )?;
        if let Some(store) = store.as_ref() {
            store.capture(&seat.id, sequence.0, &output.message);
        }
        // Feedthrough: the room is told what the turn wrote, by the host, in
        // one row. A peer can then open the file instead of asking, and a
        // result that missed the post is still pointed at. The notebook is the
        // seat's own and is not announced.
        let written = files_written(&output.files_written, &seat.id);
        if !written.is_empty() {
            let note = format!("@{} wrote {}", seat.id, written.join(", "));
            println!("   {note}");
            sequence = transcript.append(
                Some(spec.id.clone()),
                SessionAuthor::System {
                    kind: "workspace".into(),
                    label: "workspace".into(),
                },
                &note,
                Audience::Desk,
            )?;
        }

        // A seat asked to close. The row is already in the transcript, so the
        // desk ends holding the message rather than losing it — and the chain
        // is not dispatched, because a finished desk has nobody to hand a turn
        // to. Without this the chair nudges a delivered room once per remaining
        // round: in run 28 that was nine turns of `@lead` restating the same
        // answer to a prompt that could not be told the work was done.
        if committed.closing {
            println!("   -- seat reports the work finished; closing the desk");
            break;
        }

        let outcome = dispatch_mention(
            &queue,
            policy,
            &MentionDispatchInput {
                key: DispatchKey {
                    trigger_sequence: sequence.0,
                },
                conversation: DispatchConversation {
                    desk_id: spec.id.clone(),
                    thread_root: None,
                },
                author_id: seat.id.clone(),
                content: committed.content.clone(),
                mentions: committed.mentions,
                hop: job.hop,
            },
            &roster,
        )
        .await?;
        match outcome {
            MentionDispatchOutcome::Enqueued => println!("   -> one child turn enqueued"),
            MentionDispatchOutcome::Already => println!("   -> duplicate, not enqueued"),
            MentionDispatchOutcome::Refused { reason } => println!("   -> refused: {reason:?}"),
            MentionDispatchOutcome::NotDispatched { reason } => {
                println!("   -> no child turn: {reason:?}");
            }
        }
    }

    println!(
        "\ndesk closed: {turns} turns, {} rows, {tokens} tokens reported",
        transcript.len()
    );
    Ok(())
}

/// Characters of foldable, desk-visible content the room's account does not
/// yet cover.
///
/// Private rows are skipped, because an account may not contain one: a long
/// aside must not be able to spend the room's summarization budget on content
/// the fold is forbidden to carry. The live tail — the newest `keep_live` rows
/// — is skipped too: `refold` can never fold it, so counting it toward the
/// size trigger would spend a provider call summarizing older rows while an
/// oversized recent post, the actual cause, sails through untouched. See
/// `docs/specs/folding-by-size.md`'s "Both triggers are thresholds on
/// foldable content, never on the live tail."
fn unfolded_chars(rows: &[LogMessage], account: Option<&ChannelDigest>, keep_live: usize) -> usize {
    let folded = account.map_or(0, |digest| digest.through.0);
    let ceiling = (rows.len() as u64).saturating_sub(keep_live as u64);
    rows.iter()
        .filter(|row| {
            row.sequence.0 > folded && row.sequence.0 <= ceiling && row.audience.is_desk()
        })
        .map(|row| row.content.chars().count())
        .sum()
}

#[cfg(test)]
mod test {
    //! `unfolded_chars` in isolation: the live tail must never be able to
    //! trip the size trigger on its own, because `refold` can never fold it.

    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

    use super::unfolded_chars;
    use tinyhivemind::{LogMessage, Sequence, SessionAuthor, aside::Audience};

    fn row(sequence: u64, content: &str) -> LogMessage {
        LogMessage {
            sequence: Sequence(sequence),
            chat_id: None,
            parent: None,
            author: SessionAuthor::Agent {
                id: "solver".into(),
                label: "SOLVER".into(),
            },
            content: content.into(),
            audience: Audience::Desk,
        }
    }

    #[test]
    fn an_oversized_live_tail_does_not_trip_the_size_trigger_alone() {
        // Every row here is inside `keep_live`: `refold` cannot fold any of
        // them, so a huge live post must not count toward the size trigger.
        let rows = vec![row(1, &"x".repeat(50)), row(2, &"y".repeat(500_000))];
        assert_eq!(
            unfolded_chars(&rows, None, 30),
            0,
            "nothing is foldable yet, so nothing should be counted",
        );
    }

    #[test]
    fn only_rows_at_or_below_the_ceiling_are_counted() {
        // keep_live = 1 leaves row 1 foldable and row 2 live.
        let rows = vec![row(1, &"a".repeat(10)), row(2, &"b".repeat(500_000))];
        assert_eq!(
            unfolded_chars(&rows, None, 1),
            10,
            "only the foldable row's characters count, not the live tail's",
        );
    }

    #[test]
    fn a_folded_watermark_still_excludes_rows_at_or_before_it() {
        let rows = vec![row(1, &"a".repeat(10)), row(2, &"b".repeat(20))];
        let account = super::ChannelDigest {
            conversation: super::Conversation {
                desk_id: "pe1006".into(),
                desk_name: "PE 1006".into(),
                thread_root: None,
            },
            through: Sequence(1),
            covered: 1,
            generation: 1,
            text: "account so far".into(),
        };
        assert_eq!(
            unfolded_chars(&rows, Some(&account), 0),
            20,
            "row 1 is already folded; only row 2 is unfolded and foldable",
        );
    }

    #[test]
    fn a_private_row_never_counts_even_when_it_would_be_foldable() {
        let mut private = row(1, &"z".repeat(500_000));
        private.audience = Audience::Aside {
            members: vec!["checker".into()],
        };
        let rows = vec![private];
        assert_eq!(
            unfolded_chars(&rows, None, 0),
            0,
            "an account may not contain a private row, so it must not trigger one either",
        );
    }
}
