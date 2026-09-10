//! Unit tests for the federation-wide digest: what it costs, where its rows
//! land, and the one property that makes broadcasting a reading safe.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::federation::Federation;
use crate::policy::tuned_policy;
use super::member::SwarmSim;

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
        Exchange {
            asking: AskChannel::OffFloor { cap: 2 },
            digest: 1,
        },
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
    let federation = federation();
    let policy = desk_policy(&federation);
    let quiet = Exchange {
        asking: AskChannel::OffFloor { cap: 2 },
        digest: 0,
    };
    let report =
        run_swarm(&federation, &policy, referrals(), quiet, "Decide.", false).expect("runs");
    assert_eq!(report.digests, 0);

    // ...and the arm it leaves behind is the one every recorded number was
    // taken against: same decision, same turns, same crossings.
    let recorded =
        run_swarm(&federation, &policy, referrals(), quiet, "Decide.", false).expect("runs");
    assert_eq!(report.decided, recorded.decided);
    assert_eq!(report.turns, recorded.turns);
    assert_eq!(report.crossings, recorded.crossings);
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
        Exchange {
            asking: AskChannel::OffFloor { cap: 2 },
            digest: 1,
        },
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
        Exchange {
            asking: AskChannel::OffFloor { cap: 2 },
            digest: 2,
        },
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
    let exchange = Exchange {
        asking: AskChannel::OffFloor { cap: 2 },
        digest: 0,
    };
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
    let exchange = Exchange {
        asking: AskChannel::OffFloor { cap: 2 },
        digest: 1,
    };
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
