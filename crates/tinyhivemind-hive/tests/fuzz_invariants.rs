//! Deterministic fuzz-style public API invariants for the trace and quorum folds.

#![allow(clippy::expect_used)]

use tinyhivemind_hive::{
    QuorumPolicy, Sequence, SessionAuthor, SessionMessage, TRACE_CAP, read, standings,
};

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
    const LINES: [&str; 16] = [
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
        assert!(traces.len() <= messages.len() * TRACE_CAP);
    }
}
