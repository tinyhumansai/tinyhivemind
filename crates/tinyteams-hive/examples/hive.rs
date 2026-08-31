//! Run one deliberation episode and print its trace.
//!
//! ```sh
//! cargo run -p tinyteams-hive --example hive
//! ```
//!
//! The agents here are scripted, so the run is deterministic and needs no
//! model. What it shows is the shape of the protocol: who took each turn and
//! why, what that turn was allowed to see, and how the room terminated.

use std::collections::VecDeque;

use tinyteams_hive::{
    Conversation, EpisodePolicy, EpisodeState, HiveStep, Phase, QuorumPolicy, Sequence,
    SessionAuthor, SessionMessage, Visibility,
    desk::{Desk, DeskSet, ResponderMode},
    project_for,
    quorum::{ConsensusState, consensus, standings},
    roster::{Roster, RosterMember},
    step,
    trace::read,
};

const MEMBERS: [&str; 5] = ["planner", "scout", "critic", "archivist", "auditor"];

fn main() {
    let members: Vec<RosterMember> = MEMBERS
        .iter()
        .map(|id| RosterMember {
            id: (*id).to_owned(),
            name: Some((*id).to_owned()),
        })
        .collect();
    let desks = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: Some("Ship the rollout".into()),
        members: MEMBERS.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Auto,
    }];
    let conversation = Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    };

    // The host owns the journal. The library never appends to it.
    //
    // The room already holds a genuine tie: two proposals, two grounded
    // supporters each. An additive vote cannot break this, which is the point.
    let journal = vec![
        message(1, SessionAuthor::Operator, "How should we roll this out?"),
        speech(2, "planner", "!propose #stage Stage it behind a flag."),
        speech(3, "scout", "!propose #ship Ship it all at once."),
        speech(
            4,
            "critic",
            "!support #stage ^2 Staging bounds the blast radius.",
        ),
        speech(
            5,
            "archivist",
            "!support #ship ^3 We have shipped like this before.",
        ),
    ];

    // Only auditor has backed neither side, so only auditor can break the tie —
    // and it does so by objecting to an *advocate*, not to an option.
    let scripts: Vec<(&str, VecDeque<&str>)> = vec![
        (
            "auditor",
            ["!object >5 ^2 That precedent was a different system."].into(),
        ),
        (
            "planner",
            ["!evidence ^2 The flag is already wired up."].into(),
        ),
        (
            "scout",
            ["!evidence ^3 The last big-bang release held."].into(),
        ),
        (
            "archivist",
            ["!question Fair — what is the rollback path?"].into(),
        ),
        ("critic", ["!evidence ^2 Rollback is one flag flip."].into()),
    ];

    let policy = EpisodePolicy {
        turn_budget: 10,
        quorum: QuorumPolicy {
            threshold: 2,
            window: 100,
            require_grounded: true,
        },
        ..EpisodePolicy::DEFAULT
    };

    println!(
        "Engineering desk — {} members, budget {} turns\n",
        MEMBERS.len(),
        policy.turn_budget
    );
    run(
        &members,
        &desks,
        journal,
        scripts,
        EpisodeState::opened(conversation, Sequence(1)),
        &policy,
    );
}

/// Drive the episode to termination, printing one line per turn.
fn run(
    members: &[RosterMember],
    desks: &[Desk],
    mut journal: Vec<SessionMessage>,
    mut scripts: Vec<(&str, VecDeque<&str>)>,
    mut state: EpisodeState,
    policy: &EpisodePolicy,
) {
    loop {
        let roster = Roster::new(members, &[], &[]);
        let desk_set = DeskSet::new(desks, &[], &[], &[], &[]);
        let decision = match step(&state, &journal, &roster, &desk_set, policy) {
            Ok(decision) => decision,
            Err(error) => {
                eprintln!("episode failed: {error}");
                return;
            }
        };

        let HiveStep::Speak { turn } = decision else {
            report(&decision);
            return;
        };
        let turn = *turn;

        let visible = project_for(&turn, &journal);
        let content = scripts
            .iter_mut()
            .find(|(id, _)| *id == turn.agent_id)
            .and_then(|(_, script)| script.pop_front())
            .unwrap_or("!question Nothing further from me.");

        println!(
            "  {:>9}  {:<9} {:<5} saw {}/{}  {content}",
            turn.agent_id,
            format!("{:?}", turn.reason).to_lowercase(),
            match turn.visibility {
                Visibility::Blind => "blind",
                Visibility::Full => "full",
            },
            visible.len(),
            journal.len(),
        );

        let next = u64::try_from(journal.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        journal.push(speech(next, &turn.agent_id, content));
        state = turn.next_state;
    }
}

/// Explain how the room finished.
fn report(step: &HiveStep) {
    match step {
        HiveStep::Converged { topic, standing } => {
            println!(
                "\n  converged on #{topic} — carried by {}",
                standing.supporters.join(", "),
            );
            if !standing.silenced.is_empty() {
                println!("  silenced by objection: {}", standing.silenced.join(", "));
            }
            println!("\n  The room started tied. No vote was subtracted from a");
            println!("  proposal; one objection silenced a rival's *advocate*,");
            println!("  and the survivor carried.");
        }
        HiveStep::Deadlocked { topics } => {
            let names: Vec<String> = topics.iter().map(ToString::to_string).collect();
            println!("\n  deadlocked between #{}", names.join(" and #"));
        }
        HiveStep::Exhausted { spent } => {
            println!("\n  budget exhausted after {spent} turns, no decision");
        }
        HiveStep::Idle => println!("\n  nobody had anything to say"),
        HiveStep::Speak { .. } => unreachable!("a speaking step is not terminal"),
    }
}

fn message(sequence: u64, author: SessionAuthor, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author,
        content: content.to_owned(),
    }
}

fn speech(sequence: u64, id: &str, content: &str) -> SessionMessage {
    message(
        sequence,
        SessionAuthor::Agent {
            id: id.to_owned(),
            label: id.to_owned(),
        },
        content,
    )
}
