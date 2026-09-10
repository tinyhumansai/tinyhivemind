//! Unit tests for the federation-wide digest: what it costs, where its rows
//! land, and the one property that makes broadcasting a reading safe.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::board::AskChannel;
use super::member::SwarmSim;
use super::*;
use crate::federation::Federation;
use crate::policy::tuned_policy;
use tinyhivemind_hive::dispatch::{DispatchConversation, DispatchKey};
use tinyhivemind_hive::referral::ReferralKind;

/// A referral of `kind`, `content` "arriving from" `from_desk` at `member`,
/// on the desk conversation `member` itself sits on. Deliberately minimal —
/// only the fields [`SwarmSim::answer`] actually reads are given real
/// values, and the rest carry the cheapest value that type-checks.
fn incoming(kind: ReferralKind, from_desk: &str, content: &str) -> Referral {
    Referral {
        key: DispatchKey {
            trigger_sequence: 0,
        },
        kind,
        source_id: "asker".to_owned(),
        target_id: "answerer".to_owned(),
        content: content.to_owned(),
        from: DispatchConversation {
            desk_id: from_desk.to_owned(),
            thread_root: None,
        },
        to: DispatchConversation {
            desk_id: "answerer-desk".to_owned(),
            thread_root: None,
        },
        origin: None,
        child_hop: 1,
    }
}

/// A small federation with distinct decoys, deliberate rather than clamped.
fn federation() -> Federation {
    Federation::generate(7, 4, 4, 8, 0, 110)
}

/// The policy a desk of this federation deliberates at.
fn desk_policy(federation: &Federation) -> EpisodePolicy {
    tuned_policy(federation.desks[0].members.len())
}

/// The referral policy the swarm arms run at: one round trip.
fn referrals() -> ReferralPolicy {
    ReferralPolicy {
        enabled: true,
        max_hops: 2,
        reach: tinyhivemind_hive::referral::ReferralReach::Desks,
        returns: true,
    }
}

#[test]
fn a_digest_costs_one_call_per_desk_however_many_peers_hear_it() {
    let federation = federation();
    let policy = desk_policy(&federation);
    let report = run_swarm(
        &federation,
        &policy,
        referrals(),
        Exchange::from_caps(2, 1),
        "Decide.",
        false,
    )
    .expect("runs");

    // One publication per desk, not one per peer. That asymmetry is the whole
    // reason the mechanism is worth having at a hundred desks.
    let desks = federation.desks.len();
    assert_eq!(report.digests, u32::try_from(desks).unwrap());
}

#[test]
fn a_digest_is_off_unless_the_caller_asks_for_it() {
    // "Off" has to be shown by contrast with "on", or the assertion is a run
    // compared against itself. So the same federation is run both ways and the
    // two are required to differ in exactly the places a digest touches.
    let federation = federation();
    let policy = desk_policy(&federation);
    let run = |exchange| {
        run_swarm(
            &federation,
            &policy,
            referrals(),
            exchange,
            "Decide.",
            false,
        )
        .expect("runs")
    };
    let quiet = run(Exchange::from_caps(2, 0));
    let publishing = run(Exchange::from_caps(2, 1));

    assert_eq!(quiet.digests, 0, "nothing is published unless asked for");
    assert_eq!(
        publishing.digests,
        u32::try_from(federation.desks.len()).unwrap(),
        "and asking for it publishes once per desk",
    );

    // The rows a digest writes cross channels, so the quiet arm must show
    // strictly fewer crossings — which is what makes `digests: 0` mean the
    // code path was not taken rather than merely not counted.
    assert!(
        publishing.crossings > quiet.crossings,
        "publishing writes rows on every peer: {} against {}",
        publishing.crossings,
        quiet.crossings,
    );
}

#[test]
fn a_digest_carries_information_and_never_support() {
    // The safety property. A published row is authored by a member of the
    // *publishing* desk and appended to every peer's journal, and
    // `episode::step` folds a trace only from a current member of the desk it
    // is folding. So the row reaches every reader and reaches no standing.
    let federation = federation();
    let policy = desk_policy(&federation);
    let report = run_swarm(
        &federation,
        &policy,
        referrals(),
        Exchange::from_caps(2, 1),
        "Decide.",
        true,
    )
    .expect("runs");

    // Every desk still reached an ending of its own rather than being carried
    // by another desk's rows: a digest that could deposit support would let a
    // desk converge on a topic none of its own members ever backed.
    assert_eq!(report.desks.len(), federation.desks.len());
    for desk in &report.desks {
        assert_eq!(desk.ending, crate::run::Ending::Converged);
        let decided = desk.decided.as_ref().expect("a converged desk decided");
        // Whatever it settled on is one of the options on the slate, and the
        // desks did not all inherit one publisher's answer wholesale.
        assert!(federation.topics.contains(decided));
    }

    // The published rows are in the transcript, so the information is really
    // crossing rather than being quietly dropped.
    let published = report
        .trace
        .iter()
        .filter(|line| line.contains("!evidence") && line.contains("reads"))
        .count();
    assert!(published > 0, "a digest writes rows a reader can see");
}

#[test]
fn a_digest_is_bounded_by_its_own_count_and_cannot_run_the_loop() {
    // Two rounds means two publications per desk and no more, whatever else
    // the federation is doing. Without a bound the settle phase would report
    // progress forever and the run would not terminate.
    let federation = federation();
    let policy = desk_policy(&federation);
    let report = run_swarm(
        &federation,
        &policy,
        referrals(),
        Exchange::from_caps(2, 2),
        "Decide.",
        false,
    )
    .expect("runs");
    let desks = u32::try_from(federation.desks.len()).unwrap();
    assert_eq!(report.digests, desks.saturating_mul(2));
}

/// Drive a federation through the concurrent scheduler, which `run_swarm`
/// never selects: a simulated turn is arithmetic, so the simulated path always
/// passes `jobs: 1`. Everything the concurrent pass does differently is
/// therefore untested unless a test asks for it directly.
fn drive_concurrently(federation: &Federation, exchange: Exchange, jobs: usize) -> SwarmReport {
    let channels = channels(federation);
    let policy = desk_policy(federation);
    let mut seated: Vec<SwarmSim> = federation
        .agents
        .iter()
        .map(|agent| {
            let mut agent = agent.clone();
            agent.set_quorum(policy.quorum);
            SwarmSim::new(federation, agent)
        })
        .collect();
    let mut members = group_by_desk(&channels, seated.iter_mut().map(|member| member as _));
    drive_swarm(
        &channels,
        &mut members,
        &SwarmRun {
            policy: &policy,
            referrals: referrals(),
            exchange,
            jobs,
        },
        "Decide.",
        false,
    )
    .expect("runs")
}

#[test]
fn the_concurrent_pass_still_answers_the_referrals_it_routes() {
    // The regression this guards: answering a referral is a model call, so it
    // was moved out of the scheduler's sequential settle phase and into the
    // concurrent stage beside the turns. If that move dropped an answer, the
    // questions would still cross and nothing would come back.
    let federation = federation();
    let exchange = Exchange::from_caps(2, 0);
    let report = drive_concurrently(&federation, exchange, 4);

    assert!(
        report.crossings > 0,
        "the arm under test is the one where questions cross",
    );
    assert_eq!(
        report.desks.len(),
        federation.desks.len(),
        "every desk reaches an ending rather than hanging on an answer",
    );
    // Nothing was left queued for a desk that had already finished, which is
    // what a dropped or late answer would show up as.
    assert_eq!(report.stranded, 0);
}

#[test]
fn the_concurrent_pass_is_deterministic_in_its_width() {
    // Rows land in desk order, and within a desk in the order the library
    // authorized them, whatever order the calls return in. So the number of
    // workers is a wall-clock knob and nothing else.
    let federation = federation();
    let exchange = Exchange::from_caps(2, 1);
    let narrow = drive_concurrently(&federation, exchange, 2);
    let wide = drive_concurrently(&federation, exchange, 16);

    assert_eq!(narrow.decided, wide.decided);
    assert_eq!(narrow.turns, wide.turns);
    assert_eq!(narrow.crossings, wide.crossings);
    assert_eq!(narrow.digests, wide.digests);
    assert_eq!(narrow.off_floor_asks, wide.off_floor_asks);
    let endings = |report: &SwarmReport| {
        report
            .desks
            .iter()
            .map(|desk| (desk.name.clone(), desk.ending, desk.decided.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(endings(&narrow), endings(&wide));
}

#[test]
fn an_ask_cap_of_zero_means_on_the_floor_in_every_arm() {
    // `--ask-cap 0` is documented as putting asking back on the floor. Read
    // literally as `OffFloor { cap: 0 }` it means *no asking at all*, which is
    // the siloed control wearing the off-floor arm's label — and the live
    // driver mapped it while the simulated arms did not.
    assert_eq!(Exchange::from_caps(0, 0), Exchange::ON_FLOOR);
    assert_eq!(
        Exchange::from_caps(2, 1),
        Exchange {
            asking: AskChannel::OffFloor { cap: 2 },
            digest: 1,
        },
    );

    // On the floor, a desk may ask every peer once; the collapse that fixes
    // is what `--ask-cap` above zero exists for.
    let federation = federation();
    let policy = desk_policy(&federation);
    let on_floor = run_swarm(
        &federation,
        &policy,
        referrals(),
        Exchange::from_caps(0, 0),
        "Decide.",
        false,
    )
    .expect("runs");
    assert_eq!(on_floor.off_floor_asks, 0, "nothing is asked off the floor");
}

#[test]
fn planting_puts_the_cure_for_a_desk_on_another_desk() {
    // The whole structure of the experiment. A desk that held the fact about
    // its own decoy could answer its blind spot alone, and the arms would
    // measure nothing about crossing a channel.
    let plain = federation();
    assert!(
        plain.agents.iter().all(|agent| agent.ruled_out.is_empty()),
        "a federation holds no facts until they are planted",
    );

    let planted = plain.planted();
    assert!(planted.evidence);
    for (index, desk) in planted.desks.iter().enumerate() {
        // Nobody on this desk may hold the fact that would cure this desk...
        let seats: Vec<&crate::sim::SimAgent> = planted
            .agents
            .iter()
            .filter(|agent| desk.members.contains(&agent.id))
            .collect();
        assert!(
            !seats
                .iter()
                .any(|agent| agent.ruled_out.contains(&desk.decoy)),
            "desk {} can cure its own blind spot",
            desk.name,
        );

        // ...and the next desk round must.
        let holder = &planted.desks[(index + 1) % planted.desks.len()];
        let holds = planted
            .agents
            .iter()
            .filter(|agent| holder.members.contains(&agent.id))
            .filter(|agent| agent.ruled_out.contains(&desk.decoy))
            .count();
        assert_eq!(
            holds,
            holder.members.len(),
            "the holding desk knows what it holds, on every seat",
        );
    }
}

#[test]
fn a_fact_crosses_a_channel_and_a_reading_does_not_carry_it() {
    // The wire-level claim: with facts planted, a desk's outgoing line says
    // what it can disqualify; without them the same line is exactly the line
    // the harness always wrote.
    let plain = federation();
    let planted = plain.planted();
    let policy = desk_policy(&plain);
    let exchange = Exchange::from_caps(2, 1);

    let quiet = run_swarm(&plain, &policy, referrals(), exchange, "Decide.", true).expect("runs");
    assert!(
        !quiet.trace.iter().any(|line| line.contains("rules out")),
        "no facts exist, so none are stated",
    );

    let carrying =
        run_swarm(&planted, &policy, referrals(), exchange, "Decide.", true).expect("runs");
    assert!(
        carrying.trace.iter().any(|line| line.contains("rules out")),
        "a planted fact reaches the wire",
    );
    // The cost is a clause, not a call: the same asks and the same digests.
    assert_eq!(carrying.off_floor_asks, quiet.off_floor_asks);
    assert_eq!(carrying.digests, quiet.digests);
}

#[test]
fn a_wrong_fact_names_the_truth_and_spreads_the_same_way() {
    // The adversarial half. A fact does not average, which is why it survives
    // a shared bias — and why a mistaken one is worse than a mistaken opinion:
    // it discounts the right answer for every desk it reaches, undiluted.
    // Planting them all wrong is the extreme, and it must actually reach the
    // wire or the robustness sweep beside it measures nothing.
    let federation = federation();
    let wrong = federation.planted_with(1_000, 7);
    assert!(wrong.evidence);
    let ruled_out: Vec<&TopicId> = wrong
        .agents
        .iter()
        .flat_map(|agent| agent.ruled_out.iter())
        .collect();
    assert!(!ruled_out.is_empty(), "planting produced facts to check");
    assert!(
        ruled_out.iter().all(|topic| **topic == federation.truth),
        "every planted fact names the truth at a thousand per mille",
    );

    let report = run_swarm(
        &wrong,
        &desk_policy(&federation),
        referrals(),
        Exchange::from_caps(2, 1),
        "Decide.",
        true,
    )
    .expect("runs");
    let truth = format!("rules out #{}", federation.truth);
    assert!(
        report.trace.iter().any(|line| line.contains(&truth)),
        "a wrong fact crosses exactly as a right one does",
    );
}

#[test]
fn planting_no_wrong_facts_leaves_the_truth_alone() {
    // The control for the test above: at zero noise nothing disqualifies the
    // answer, so a difference in the sweep is the noise and not the planting.
    let federation = federation();
    let planted = federation.planted_with(0, 7);
    assert!(
        !planted
            .agents
            .iter()
            .any(|agent| agent.ruled_out.contains(&federation.truth)),
        "a true fact never names the truth",
    );
}

/// Regression for the referral answer dropping a fact clause on either hop.
///
/// `readings` and `facts` are parsed independently of each other by design —
/// see `swarm/format.rs` — which means an `answer` built only from `readings`
/// silently discards any `rules out` clause in what it is answering. That is
/// exactly what forwarding and returning a referral both used to do, so
/// `--evidence` over a referral (rather than a digest) measured nothing about
/// a fact crossing a channel: `swarm` and `swarm°` were carrying discounted
/// numeric opinions and nothing more.
#[test]
fn answer_forward_preserves_the_asking_desks_fact() {
    let federation = federation();
    let seat = federation.agents[0].clone();
    let mut member = SwarmSim::new(&federation, seat);

    // The question, as `ask` would have written it: the asking desk's own
    // reading, and a fact it holds.
    let question = incoming(
        ReferralKind::Forward,
        "platform",
        "@#mobile We are about to back #a here. Platform reads #a at 40, #b at 100. \
         Platform rules out #a.",
    );
    let answer = member.answer(&question, &[]).expect("answers");

    assert!(
        answer.contains("Platform rules out #a"),
        "the asking desk's own fact must survive the forward hop: {answer}",
    );
    assert!(
        answer.contains("Platform reads"),
        "the asking desk's reading is unaffected by the fix: {answer}",
    );
}

/// The companion regression: the far desk's fact, carried home on the return
/// hop, must not be dropped either.
#[test]
fn answer_return_preserves_the_far_desks_fact() {
    let federation = federation();
    let seat = federation.agents[0].clone();
    let mut member = SwarmSim::new(&federation, seat);

    // What the far desk (Mobile) answered on the forward hop: its own
    // reading and its own fact, exactly as `answer(Forward)` now writes it.
    let carried_answer = incoming(
        ReferralKind::Return,
        "mobile",
        "Mobile reads #a at 50, #b at 90. Mobile rules out #b.",
    );
    let answer = member.answer(&carried_answer, &[]).expect("answers");

    assert!(
        answer.contains("Mobile rules out #b"),
        "the far desk's fact must survive the return hop: {answer}",
    );
    assert!(
        answer.contains("Mobile reads"),
        "the far desk's reading is unaffected by the fix: {answer}",
    );
}

/// Regression for the free-information control pooling readings but not the
/// facts a planted `--evidence` run adds.
///
/// `swarm::pooled` is the ceiling every bounded arm is measured against: it
/// hands every desk every other desk's information for free. Before this fix
/// it only ever imported numeric slates, so under `--evidence` a member
/// pooled with the ceiling still held only the one fact planted on its own
/// desk — the exact thing a real exchange (digest or referral) is supposed to
/// beat it by *not* withholding.
#[test]
fn pooled_hands_over_every_other_desks_facts() {
    let plain = federation();
    let planted = plain.planted();
    let result = pooled(&planted);

    for (index, desk) in planted.desks.iter().enumerate() {
        for member in &desk.members {
            let Some(seat) = result.seat_of(member) else {
                panic!("every planted member has a seat");
            };
            let held = &result.agents[seat].ruled_out;
            for (other, other_desk) in planted.desks.iter().enumerate() {
                if other == index {
                    continue;
                }
                assert!(
                    held.contains(&other_desk.decoy),
                    "{member} pooled with the ceiling must hold {}'s fact about {}",
                    other_desk.name,
                    other_desk.decoy,
                );
            }
        }
    }
}
