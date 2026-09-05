//! Deterministic fuzz-style public API invariants for the trace and quorum folds.

#![allow(clippy::expect_used)]

use tinyhivemind_hive::{
    Conversation, DirectoryPolicy, EpisodePolicy, EpisodeState, QuorumPolicy, SalienceWeights,
    Sequence, SessionAuthor, SessionMessage, TRACE_CAP,
    attention::{BidContext, bids},
    desk::{Desk, DeskSet, ResponderMode},
    directory, read,
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
                at,
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
