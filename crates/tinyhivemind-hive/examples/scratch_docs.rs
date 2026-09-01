use tinyhivemind::{Sequence, SessionAuthor, SessionMessage};
use tinyhivemind_hive::{
    attention::{bids, floor_holder, AgentThreshold, BidContext},
    quorum::{consensus, standings, QuorumPolicy},
    salience::{importance, salience, SalienceWeights},
    trace::{read, TraceKind},
};

fn agent(id: &str) -> SessionAuthor {
    SessionAuthor::Agent { id: id.into(), label: id.into() }
}
fn said(seq: u64, who: &str, content: &str) -> SessionMessage {
    SessionMessage { sequence: Sequence(seq), author: agent(who), content: content.into() }
}
fn show(label: &str, msgs: &[SessionMessage], at: u64, policy: &QuorumPolicy) {
    let st = standings(&read(msgs), Sequence(at), policy).unwrap();
    println!("\n== {label} (at seq {at}) ==");
    for s in &st {
        println!("  #{:<6} supporters={:?} silenced={:?} support={}",
                 s.topic, s.supporters, s.silenced, s.support);
    }
    println!("  consensus = {:?}", consensus(&st, policy));
}

fn main() {
    let w = SalienceWeights::DEFAULT;
    println!("weights: recency={} importance={} relevance={} half_life={}",
             w.recency, w.importance, w.relevance, w.half_life);
    println!("importance by kind: propose={} support={} object={} question={} commit={}",
             importance(TraceKind::Propose), importance(TraceKind::Support),
             importance(TraceKind::Object), importance(TraceKind::Question),
             importance(TraceKind::Commit));

    // 1. decay: two identical supports, 40 sequences apart
    let msgs = [
        said(1, "a", "!support #stage ^0 Old point."),
        said(41, "b", "!support #stage ^0 New point."),
    ];
    let t = read(&msgs);
    println!("\n== two !support traces, read at seq 41, relevance 50 ==");
    for tr in &t {
        let s = salience(tr, Sequence(41), &w, 50).unwrap();
        println!("  seq {:<3} distance {:<3} salience {}", tr.sequence.0, 41 - tr.sequence.0, s.0);
    }
    println!("\n== one !propose read further and further away ==");
    let one = [said(1, "a", "!propose #stage Stage it.")];
    let t1 = read(&one);
    for at in [1u64, 10, 20, 40, 80, 200] {
        println!("  distance {:<4} salience {}", at - 1, salience(&t1[0], Sequence(at), &w, 50).unwrap().0);
    }

    let policy = QuorumPolicy { threshold: 2, window: 30, require_grounded: true };

    // 2. quorum reached cleanly
    let a = vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "scout", "!propose #ship Ship it all at once."),
        said(3, "critic", "!support #stage ^1 Staging bounds the blast radius."),
    ];
    show("one supporter each, nothing carries", &a, 3, &policy);
    let mut b = a.clone();
    b.push(said(4, "auditor", "!support #stage ^1 And it is reversible."));
    show("a second grounded supporter carries #stage", &b, 4, &policy);

    // 3. ungrounded support is ignored
    let mut c = a.clone();
    c.push(said(4, "auditor", "!support #stage I agree."));
    show("the same support without a citation", &c, 4, &policy);

    // 4. cross-inhibition breaks a tie
    let mut d = b.clone();
    d.push(said(5, "scout", "!support #ship ^2 One cutover is simpler."));
    d.push(said(6, "auditor", "!support #ship ^2 Agreed, fewer moving parts."));
    show("both carry: deadlocked", &d, 6, &policy);
    let mut e = d.clone();
    e.push(said(7, "planner", "!object >6 ^1 The regions are not independent."));
    show("!object >6 silences auditor on #ship", &e, 7, &policy);

    // 5. bids
    let traces = read(&b);
    let st = standings(&traces, Sequence(5), &policy).unwrap();
    let members: Vec<&str> = vec!["planner", "scout", "critic", "auditor"];
    let thresholds = vec![
        AgentThreshold::new("planner", 0),
        AgentThreshold::new("scout", 0),
        AgentThreshold::new("critic", 0),
        AgentThreshold::new("auditor", 99_000),
    ];
    let ctx = BidContext {
        traces: &traces, standings: &st, members: &members, thresholds: &thresholds,
        at: Sequence(5), weights: &w, dominance_cap: 50, repetition_cap: 3, window: 30,
    };
    let bd = bids(&ctx).unwrap();
    println!("\n== bids at seq 5 (auditor's threshold is 99000) ==");
    for x in &bd { println!("  {:<8} urge={:<7} reason={:?}", x.agent_id, x.urge, x.reason); }
    println!("  floor = {:?}", floor_holder(&bd).map(|x| x.agent_id.clone()));
}
