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
    Conversation, EpisodePolicy, EpisodeState, HiveStep, QuorumPolicy, SessionAuthor,
    SessionMessage, Sequence, Visibility,
    desk::{Desk, DeskSet, ResponderMode},
    project_for,
    roster::{Roster, RosterMember},
    step,
};

const MEMBERS: [&str; 4] = ["planner", "scout", "critic", "archivist"];

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
    let mut journal = vec![SessionMessage {
        sequence: Sequence(1),
        author: SessionAuthor::Operator,
        content: "How should we roll this out?".into(),
    }];

    // Two proposals, a grounded objection that silences one advocate, and the
    // room settles on the survivor.
    let mut scripts: Vec<(&str, VecDeque<&str>)> = vec![
        ("planner", ["!propose #stage Stage it behind a flag."].into()),
        ("scout", ["!propose #ship Ship it all at once."].into()),
        (
            "critic",
            [
                "!support #stage ^2 Staging bounds the blast radius.",
                "!object >3 ^2 That precedent was a different system.",
            ]
            .into(),
        ),
        (
            "archivist",
            ["!support #ship ^3 We have shipped like this before."].into(),
        ),
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
    let mut state = EpisodeState::opened(conversation, Sequence(1));

    println!("Engineering desk — {} members, budget {} turns\n", MEMBERS.len(), policy.turn_budget);

    loop {
        let roster = Roster::new(&members, &[], &[]);
        let desk_set = DeskSet::new(&desks, &[], &[], &[], &[]);
        let decision = match step(&state, &journal, &roster, &desk_set, &policy) {
            Ok(decision) => decision,
            Err(error) => {
                eprintln!("episode failed: {error}");
                return;
            }
        };

        let turn = match decision {
            HiveStep::Speak { turn } => *turn,
            HiveStep::Converged { topic, standing } => {
                println!(
                    "\n  converged on #{topic} — carried by {}",
                    standing.supporters.join(", "),
                );
                if !standing.silenced.is_empty() {
                    println!("  silenced by objection: {}", standing.silenced.join(", "));
                }
                return;
            }
            HiveStep::Deadlocked { topics } => {
                let names: Vec<String> = topics.iter().map(ToString::to_string).collect();
                println!("\n  deadlocked between #{}", names.join(" and #"));
                return;
            }
            HiveStep::Exhausted { spent } => {
                println!("\n  budget exhausted after {spent} turns, no decision");
                return;
            }
            HiveStep::Idle => {
                println!("\n  nobody had anything to say");
                return;
            }
        };

        let visible = project_for(&turn, &journal);
        let content = scripts
            .iter_mut()
            .find(|(id, _)| *id == turn.agent_id)
            .and_then(|(_, script)| script.pop_front())
            .unwrap_or("!question Nothing further from me.");

        let sight = match turn.visibility {
            Visibility::Blind => "blind",
            Visibility::Full => "full",
        };
        println!(
            "  {:>9}  {:<9} {:<5} saw {}/{}  {content}",
            turn.agent_id,
            format!("{:?}", turn.reason).to_lowercase(),
            sight,
            visible.len(),
            journal.len(),
        );

        journal.push(SessionMessage {
            sequence: Sequence(journal.len() as u64 + 1),
            author: SessionAuthor::Agent {
                id: turn.agent_id.clone(),
                label: turn.agent_id.clone(),
            },
            content: content.to_owned(),
        });
        state = turn.next_state;
    }
}
