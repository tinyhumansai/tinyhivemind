//! Deterministic fuzz-style public API invariants for the trace and quorum folds.

#![allow(clippy::expect_used)]

use tinyhivemind::aside::Audience;
use tinyhivemind_hive::{
    Conversation, DirectoryPolicy, EpisodePolicy, EpisodeState, ExchangePolicy, ExchangeRound,
    ExchangeState, NoExchangeReason, QuorumPolicy, SalienceWeights, Sequence, SessionAuthor,
    SessionMessage, TRACE_CAP,
    attention::{BidContext, bids},
    desk::{Desk, DeskSet, ResponderMode},
    directory, exchange, read,
    roster::{Roster, RosterMember},
    standings, step,
};

const MEMBERS: [&str; 4] = ["agent-0", "agent-1", "agent-2", "agent-3"];

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 7;
    *state ^= *state >> 9;
    *state
}

fn author(index: u64) -> SessionAuthor {
    SessionAuthor::Agent {
        id: format!("agent-{}", index % 4),
        label: format!("Agent {}", index % 4),
    }
}

fn content(state: &mut u64) -> String {
    const LINES: [&str; 18] = [
        "!propose #stage",
        "!support #stage ^1",
        "!object >1 ^2",
        "!refute #stage ^1",
        "!refute #stage",
        "!refute ^1",
        "!question",
        "!commit #stage",
        "not a marker !support",
        "```\n!propose #hidden\n```",
        "~~~\n!support #hidden ^1\n~~~",
        "!unknown #ignored",
        "!support #ship ^3 ^3",
        "!defer #stage",
        "!defer",
        "😀",
        "é",
        "\n",
    ];
    let count = usize::try_from(next(state) % 24).expect("bounded count");
    (0..count)
        .map(|_| {
            let index = usize::try_from(next(state) % LINES.len() as u64).expect("index fits");
            LINES[index]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn arbitrary_transcripts_have_stable_well_formed_and_idempotent_folds() {
    let mut state = 0x05ee_da11_ce55_u64;
    let policy = QuorumPolicy {
        threshold: 2,
        window: 100,
        require_grounded: true,
        ..QuorumPolicy::DEFAULT
    };
    // The same corpus under the narrowing policy, so citation-chain resolution
    // is fuzzed for termination on cycles and self-citations too.
    let evidential = QuorumPolicy {
        require_evidential: true,
        ..policy
    };

    let people = roster_members();
    let rooms = desks();
    let retired: Vec<String> = Vec::new();
    let roster = Roster::new(&people, &[], &retired);
    let desk_set = DeskSet::new(&rooms, &[], &[], &[], &retired);

    for case in 0..256_u64 {
        let messages: Vec<SessionMessage> = (0..8_u64)
            .map(|index| SessionMessage {
                sequence: Sequence(case * 16 + index),
                author: author(index),
                content: content(&mut state),
                audience: Audience::Desk,
                elided: None,
            })
            .collect();
        let traces = read(&messages);

        for pair in traces.windows(2) {
            assert!((pair[0].sequence, pair[0].offset) <= (pair[1].sequence, pair[1].offset));
        }
        for trace in &traces {
            let message = messages
                .iter()
                .find(|message| message.sequence == trace.sequence)
                .expect("trace sequence comes from the supplied transcript");
            assert!(message.content.is_char_boundary(trace.offset));
            assert_eq!(message.content[trace.offset..].chars().next(), Some('!'));
            assert!(trace.text.starts_with('!'));
        }

        let doubled_and_reversed: Vec<_> = traces
            .iter()
            .rev()
            .chain(traces.iter().rev())
            .cloned()
            .collect();
        let at = messages.last().expect("nonempty transcript").sequence;
        assert_eq!(
            standings(&traces, at, &policy).expect("valid policy"),
            standings(&doubled_and_reversed, at, &policy).expect("valid policy"),
        );
        assert_eq!(
            standings(&traces, at, &evidential).expect("valid policy"),
            standings(&doubled_and_reversed, at, &evidential).expect("valid policy"),
        );
        // The directory is folded on the same address and must be just as
        // order-independent: a redelivered or reordered medium folds to the
        // same estimate of who knows what.
        let known = DirectoryPolicy {
            window: 100,
            ..DirectoryPolicy::DEFAULT
        };
        assert_eq!(
            directory(&traces, at, &known, &[]).expect("valid policy"),
            directory(&doubled_and_reversed, at, &known, &[]).expect("valid policy"),
        );

        // The attention market folds the same medium and must be just as
        // order-independent: a bid is an argmax over addressed traces, so a
        // redelivered or reordered one must not double a member's urge or
        // move the topic the room is stuck on.
        let weights = SalienceWeights::DEFAULT;
        let members: Vec<&str> = MEMBERS.to_vec();
        let market = |folded: &[tinyhivemind_hive::Trace]| {
            let standings = standings(folded, at, &policy).expect("valid policy");
            let folded_directory = directory(folded, at, &known, &[]).expect("valid policy");
            bids(&BidContext {
                traces: folded,
                standings: &standings,
                members: &members,
                thresholds: &[],
                at: at.into(),
                weights: &weights,
                dominance_cap: 50,
                repetition_cap: 3,
                quorum: &policy,
                directory: Some(&folded_directory),
                directory_policy: Some(&known),
                defer_cap: Some(2),
            })
            .expect("valid policy")
        };
        assert_eq!(market(&traces), market(&doubled_and_reversed));

        // And `step` over the same transcript redelivered message by message,
        // which is how a host actually meets a duplicate.
        let redelivered: Vec<SessionMessage> = messages
            .iter()
            .flat_map(|message| [message.clone(), message.clone()])
            .collect();
        let episode = EpisodePolicy {
            directory: Some(known),
            defer_cap: Some(2),
            ..EpisodePolicy::DEFAULT
        };
        assert_eq!(
            step(&opened(), &messages, &roster, &desk_set, &episode).expect("valid policy"),
            step(&opened(), &redelivered, &roster, &desk_set, &episode).expect("valid policy"),
        );

        assert!(traces.len() <= messages.len() * TRACE_CAP);
    }
}

/// An aside interleaved anywhere in an arbitrary transcript leaves `step`
/// exactly where it was.
///
/// The order-independence above is idempotence under redelivery and
/// reordering. This is a different property and the one a concurrent aside
/// rests on: **addition**. A host that appends a private row alongside the
/// turn that authored it must not be able to move the floor, the standings,
/// the sequence they fold at, the phase, or the budget — whatever that row
/// says, and however many of them there are. `live_traces` drops a non-desk
/// row before any of that, and `spent` counts turns rather than rows.
///
/// The aside rows carry the *same* fuzzed grammar as the desk rows, so the
/// corpus includes private `!propose`, `!support` and `!commit` lines that
/// would carry real weight if the filter ever slipped.
#[test]
fn asides_interleaved_into_an_arbitrary_transcript_do_not_move_the_episode() {
    let mut state = 0xc0ff_eea5_1de5_u64;
    let people = roster_members();
    let rooms = desks();
    let retired: Vec<String> = Vec::new();
    let roster = Roster::new(&people, &[], &retired);
    let desk_set = DeskSet::new(&rooms, &[], &[], &[], &retired);
    let episode = EpisodePolicy {
        directory: Some(DirectoryPolicy::DEFAULT),
        defer_cap: Some(2),
        ..EpisodePolicy::DEFAULT
    };

    for case in 0..256_u64 {
        // Desk rows on even sequences, so an aside always has an odd sequence
        // of its own to land on between two of them.
        let desk: Vec<SessionMessage> = (0..8_u64)
            .map(|index| SessionMessage {
                sequence: Sequence(case * 32 + index * 2),
                author: author(index),
                content: content(&mut state),
                audience: Audience::Desk,
                elided: None,
            })
            .collect();

        let mut interleaved: Vec<SessionMessage> = Vec::new();
        for (index, message) in desk.iter().enumerate() {
            interleaved.push(message.clone());
            // Not every turn carries one, so runs of desk rows are covered
            // too.
            if next(&mut state).is_multiple_of(3) {
                continue;
            }
            let members: Vec<String> = MEMBERS
                .iter()
                .skip(usize::try_from(next(&mut state) % 4).expect("bounded index"))
                .take(2)
                .map(|id| (*id).to_owned())
                .collect();
            interleaved.push(SessionMessage {
                sequence: Sequence(case * 32 + index as u64 * 2 + 1),
                author: message.author.clone(),
                content: content(&mut state),
                audience: Audience::Aside { members },
                elided: None,
            });
        }

        assert_eq!(
            step(&opened(), &desk, &roster, &desk_set, &episode).expect("valid policy"),
            step(&opened(), &interleaved, &roster, &desk_set, &episode).expect("valid policy"),
        );
    }
}

/// Driving exchange rounds to exhaustion terminates, stays inside the policy's
/// worst case, and never moves the episode.
///
/// The three properties the mechanism is sold on, asserted together against a
/// host that runs every round it is offered and writes a row for every member
/// the round names — the most a well-behaved host can spend.
#[test]
fn exchange_rounds_terminate_inside_their_budget_without_moving_the_episode() {
    let people = roster_members();
    let rooms = desks();
    let retired: Vec<String> = Vec::new();
    let roster = Roster::new(&people, &[], &retired);
    let desk_set = DeskSet::new(&rooms, &[], &[], &[], &retired);
    let episode = EpisodePolicy::DEFAULT;
    let floor: Vec<SessionMessage> = (0..4_u64)
        .map(|index| SessionMessage {
            sequence: Sequence(index * 2),
            author: author(index),
            content: "!propose #stage".to_owned(),
            audience: Audience::Desk,
            elided: None,
        })
        .collect();
    let before = step(&opened(), &floor, &roster, &desk_set, &episode).expect("valid policy");

    for contact_cap in 1..4_u32 {
        for round_cap in 1..4_u32 {
            let policy = ExchangePolicy {
                enabled: true,
                contact_cap,
                round_cap,
            };
            let mut transcript = floor.clone();
            let mut written = 0_u32;
            let mut next = 1_u64;
            let mut opened_rounds = ExchangeState::opened();
            // A bound well above any legitimate one, so a fold that failed to
            // charge spend fails this test rather than hanging it.
            for _ in 0..64 {
                let round = exchange(
                    &policy,
                    &opened(),
                    opened_rounds,
                    &transcript,
                    &roster,
                    &desk_set,
                )
                .expect("valid snapshots");
                let ExchangeRound::Open {
                    members,
                    remaining,
                    next: carried,
                } = round
                else {
                    break;
                };
                opened_rounds = carried;
                assert!(remaining > 0, "an open round must have budget left");
                for member in &members {
                    transcript.push(SessionMessage {
                        sequence: Sequence(next * 2 + 1),
                        author: SessionAuthor::Agent {
                            id: member.clone(),
                            label: member.clone(),
                        },
                        content: format!("!aside @peer !support #stage ^0 {member}"),
                        audience: Audience::Aside {
                            members: vec!["agent-0".to_owned()],
                        },
                        elided: None,
                    });
                    next += 1;
                    written += 1;
                }
                transcript.sort_by_key(|message| message.sequence);
            }

            let seats = u32::try_from(MEMBERS.len()).expect("small");
            let worst = seats.saturating_mul(contact_cap).min(round_cap * seats);
            assert!(
                written <= worst,
                "wrote {written} rows against a worst case of {worst}",
            );
            assert!(written > 0, "a policy with budget must open one round");
            assert_eq!(
                exchange(
                    &policy,
                    &opened(),
                    opened_rounds,
                    &transcript,
                    &roster,
                    &desk_set,
                )
                .expect("valid snapshots"),
                ExchangeRound::Closed {
                    // A host writing one row per named member each round takes
                    // `min(contact_cap, round_cap)` rounds to exhaust itself,
                    // so the round cap is what bit whenever it is the smaller
                    // of the two — and the fold checks it first on a tie.
                    reason: if round_cap <= contact_cap {
                        NoExchangeReason::RoundsSpent
                    } else {
                        NoExchangeReason::ContactsSpent
                    },
                },
                "the round must close once the budget is gone",
            );

            // And the whole point: the episode cannot tell any of it happened,
            // even though every private row carries a `!support` that would
            // carry real weight on the desk.
            assert_eq!(
                step(&opened(), &transcript, &roster, &desk_set, &episode).expect("valid policy"),
                before,
            );
        }
    }
}

/// The four agents the corpus authors as, as a roster.
fn roster_members() -> Vec<RosterMember> {
    MEMBERS
        .iter()
        .map(|id| RosterMember {
            id: (*id).to_owned(),
            name: Some((*id).to_owned()),
        })
        .collect()
}

fn desks() -> Vec<Desk> {
    vec![Desk {
        id: "room".into(),
        name: "Room".into(),
        description: None,
        members: MEMBERS.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Auto,
    }]
}

fn opened() -> EpisodeState {
    EpisodeState::opened(
        Conversation {
            desk_id: "room".into(),
            desk_name: "Room".into(),
            thread_root: None,
        },
        Sequence(0),
    )
}
