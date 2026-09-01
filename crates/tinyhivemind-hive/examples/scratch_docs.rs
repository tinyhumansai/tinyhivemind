use tinyhivemind::{Sequence, SessionAuthor, SessionMessage};
use tinyhivemind_hive::{
    attention::{bids, floor_holder, AgentThreshold, BidContext},
    quorum::{consensus, standings, QuorumPolicy},
    salience::{salience, SalienceWeights},
    trace::read,
};

fn agent(id: &str) -> SessionAuthor {
    SessionAuthor::Agent { id: id.into(), label: id.into() }
}
fn said(seq: u64, who: &str, content: &str) -> SessionMessage {
    SessionMessage { sequence: Sequence(seq), author: agent(who), content: content.into() }
}

fn main() {
    let w = SalienceWeights::DEFAULT;

    // 1. decay
    let msg = [said(1, "planner", "!propose #stage Stage the rollout.")];
    let t = read(&msg);
    println!("== decay of a !propose at seq 1, relevance 50 ==");
    for at in [1u64, 5, 10, 20, 40, 60, 100] {
        let s = salience(&t[0], Sequence(at), &w, 50).unwrap();
        println!("at={:<4} distance={:<4} salience={}", at, at - 1, s.0);
    }

    // 2. quorum + cross-inhibition
    let base = vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "scout", "!propose #ship Ship it all at once."),
        said(3, "critic", "!support #stage ^1 Staging bounds the blast radius."),
        said(4, "auditor", "!support #ship ^2 One cutover is simpler to reason about."),
        said(5, "planner", "!support #stage ^1 And it is reversible."),
    ];
    let policy = QuorumPolicy { threshold: 2, window: 30, require_grounded: true };
    let st = standings(&read(&base), Sequence(5), &policy).unwrap();
    println!("\n== standings at seq 5, threshold 2 ==");
    for s in &st {
        println!("#{:<6} supporters={:?} silenced={:?} support={}", s.topic, s.supporters, s.silenced, s.support);
    }
    println!("consensus = {:?}", consensus(&st, &policy));

    let mut tied = base.clone();
    tied.push(said(6, "critic", "!support #ship ^2 Fine, one cutover."));
    let st2 = standings(&read(&tied), Sequence(6), &policy).unwrap();
    println!("\n== after seq 6, both carry ==");
    for s in &st2 { println!("#{:<6} supporters={:?}", s.topic, s.supporters); }
    println!("consensus = {:?}", consensus(&st2, &policy));

    let mut objected = tied.clone();
    objected.push(said(7, "planner", "!object >6 ^1 The regions are not independent."));
    let st3 = standings(&read(&objected), Sequence(7), &policy).unwrap();
    println!("\n== after !object >6 ==");
    for s in &st3 {
        println!("#{:<6} supporters={:?} silenced={:?}", s.topic, s.supporters, s.silenced);
    }
    println!("consensus = {:?}", consensus(&st3, &policy));

    // 3. bids
    let traces = read(&base);
    let members = ["planner", "scout", "critic", "auditor"];
    let members: Vec<&str> = members.to_vec();
    let thresholds = vec![
        AgentThreshold::new("planner", 0),
        AgentThreshold::new("scout", 0),
        AgentThreshold::new("critic", 0),
        AgentThreshold::new("auditor", 0),
    ];
    let ctx = BidContext {
        traces: &traces,
        standings: &st,
        members: &members,
        thresholds: &thresholds,
        at: Sequence(6),
        weights: &w,
        dominance_cap: 50,
        repetition_cap: 3,
        window: 30,
    };
    let b = bids(&ctx).unwrap();
    println!("\n== bids at seq 6 ==");
    for x in &b { println!("{:<8} urge={:<6} reason={:?}", x.agent_id, x.urge, x.reason); }
    println!("floor = {:?}", floor_holder(&b).map(|x| x.agent_id.clone()));
}
