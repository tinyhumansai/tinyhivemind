//! Unit tests for the transactive-memory directory fold.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::trace::read;
use tinyhivemind::{SessionAuthor, SessionMessage};

fn said(sequence: u64, author: &str, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: author.into(),
            label: author.into(),
        },
        content: content.into(),
    }
}

/// Fold a transcript at its last sequence, with a window wide enough to hold
/// the whole of it unless a test says otherwise.
fn fold(transcript: &[SessionMessage], priors: &[AgentThreshold]) -> Directory {
    let at = transcript
        .last()
        .map_or(Sequence(0), |message| message.sequence);
    directory(&read(transcript), at, &wide(), priors).expect("folds")
}

fn wide() -> DirectoryPolicy {
    DirectoryPolicy {
        window: 100,
        ..DirectoryPolicy::DEFAULT
    }
}

fn entry<'a>(folded: &'a Directory, agent: &str, topic: &str) -> &'a DirectoryEntry {
    folded
        .entries()
        .iter()
        .find(|entry| entry.agent_id == agent && entry.topic.as_str() == topic)
        .unwrap_or_else(|| panic!("no entry for {agent} on {topic} in {:?}", folded.entries()))
}

// --- Wire forms ---

#[test]
fn the_policy_pins_its_wire_form() {
    let value = serde_json::to_value(DirectoryPolicy::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "half_life": 20,
            "specialisation": 30,
            "credibility": 20,
            "prior": 10,
            "discredit": 20,
            "window": 30,
            "floor": 1_000,
        }),
    );
    assert_eq!(
        serde_json::from_value::<DirectoryPolicy>(value).expect("deserializes"),
        DirectoryPolicy::DEFAULT,
    );
    assert_eq!(DirectoryPolicy::default(), DirectoryPolicy::DEFAULT);
}

#[test]
fn an_entry_pins_its_wire_form() {
    let entry = DirectoryEntry {
        agent_id: "archivist".into(),
        topic: "pool".into(),
        specialisation: 900,
        credibility: 520,
        weight: 1_420,
    };
    let value = serde_json::to_value(&entry).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "agent_id": "archivist",
            "topic": "pool",
            "specialisation": 900,
            "credibility": 520,
            "weight": 1_420,
        }),
    );
    assert_eq!(
        serde_json::from_value::<DirectoryEntry>(value).expect("deserializes"),
        entry,
    );
}

#[test]
fn a_directory_is_transparently_its_entries() {
    let folded = fold(
        &[said(1, "archivist", "!evidence #pool It caps at twenty.")],
        &[],
    );
    let value = serde_json::to_value(&folded).expect("serializes");
    assert!(
        value.is_array(),
        "a directory is its entry list on the wire"
    );
    assert_eq!(
        value,
        serde_json::to_value(folded.entries()).expect("serializes"),
    );
    assert_eq!(
        serde_json::from_value::<Directory>(value).expect("deserializes"),
        folded,
    );
    assert!(Directory::default().entries().is_empty());
}

#[test]
fn a_decoded_directory_is_sorted_into_topic_and_agent_order() {
    // The wire form is the entry array, and a sender may write it in any
    // order. `top`, `topics` and `lines` all read the entries as sorted, so
    // decoding restores that rather than trusting what arrived.
    let unsorted = serde_json::json!([
        entry_value("scout", "pool", 200),
        entry_value("archivist", "cache", 100),
        entry_value("archivist", "pool", 300),
    ]);
    let decoded = serde_json::from_value::<Directory>(unsorted).expect("deserializes");
    let order: Vec<(&str, String)> = decoded
        .entries()
        .iter()
        .map(|entry| (entry.agent_id.as_str(), entry.topic.to_string()))
        .collect();
    assert_eq!(
        order,
        vec![
            ("archivist", "cache".to_owned()),
            ("archivist", "pool".to_owned()),
            ("scout", "pool".to_owned()),
        ]
    );
    // And `topics` reports each topic once, which is the invariant the sort
    // exists for: it compares each entry with the previous one only.
    assert_eq!(
        decoded
            .topics()
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["cache".to_owned(), "pool".to_owned()]
    );
}

#[test]
fn a_decoded_directory_rejects_a_repeated_pair() {
    let repeated = serde_json::json!([
        entry_value("archivist", "pool", 300),
        entry_value("archivist", "pool", 100),
    ]);
    let error = serde_json::from_value::<Directory>(repeated).expect_err("a repeated pair");
    assert!(
        error.to_string().contains("duplicate directory entry"),
        "unexpected error: {error}"
    );
}

/// One entry as it appears on the wire, with only its weight varying.
fn entry_value(agent_id: &str, topic: &str, weight: i64) -> serde_json::Value {
    serde_json::json!({
        "agent_id": agent_id,
        "topic": topic,
        "specialisation": weight,
        "credibility": 0,
        "weight": weight,
    })
}

// --- Malformed policies ---

#[test]
fn a_zero_half_life_is_rejected() {
    let policy = DirectoryPolicy {
        half_life: 0,
        ..DirectoryPolicy::DEFAULT
    };
    let error = directory(&[], Sequence(1), &policy, &[]).expect_err("zero half life");
    assert_eq!(error.to_string(), "directory half life must not be zero");
}

#[test]
fn a_zero_window_is_rejected() {
    let policy = DirectoryPolicy {
        window: 0,
        ..DirectoryPolicy::DEFAULT
    };
    let error = directory(&[], Sequence(1), &policy, &[]).expect_err("zero window");
    assert_eq!(error.to_string(), "directory window must not be zero");
}

#[test]
fn a_duplicate_prior_record_is_rejected() {
    let priors = [
        AgentThreshold::new("archivist", 0),
        AgentThreshold::new("archivist", 5),
    ];
    let error =
        directory(&[], Sequence(1), &DirectoryPolicy::DEFAULT, &priors).expect_err("duplicate");
    assert_eq!(error.to_string(), "duplicate agent threshold `archivist`");
}

// --- The two estimators ---

#[test]
fn an_empty_medium_folds_to_an_empty_directory() {
    let folded = fold(&[], &[]);
    assert!(folded.entries().is_empty());
    assert!(folded.topics().is_empty());
    assert!(folded.lines().is_empty());
    assert_eq!(folded.weight("archivist", &"pool".into()), 0);
    assert!(folded.top(&"pool".into()).is_none());
    assert!(folded.top_among(&"pool".into(), &["archivist"]).is_none());
}

#[test]
fn topiced_evidence_makes_a_specialist() {
    let folded = fold(
        &[said(
            1,
            "archivist",
            "!evidence #pool The pool caps at twenty.",
        )],
        &[],
    );
    let held = entry(&folded, "archivist", "pool");
    assert_eq!(held.specialisation, 1_000);
    assert_eq!(held.credibility, 0);
    // (1000 * 30) / 10.
    assert_eq!(held.weight, 3_000);
    assert!(folded.knows("archivist", &"pool".into(), &wide()));
}

#[test]
fn untopiced_evidence_earns_credibility_from_the_topic_that_cites_it() {
    let folded = fold(
        &[
            said(1, "archivist", "!evidence In-flight requests sit at 24."),
            said(2, "planner", "!support #pool ^1 So the cap is the problem."),
        ],
        &[],
    );
    // Nothing attaches the fact to a topic by itself, so the archivist has no
    // specialisation; the citer's topic is what makes it credible.
    let held = entry(&folded, "archivist", "pool");
    assert_eq!(held.specialisation, 0);
    assert!(held.credibility > 0);
    assert!(held.weight > 0);
}

#[test]
fn citing_your_own_deposit_earns_nothing() {
    let folded = fold(
        &[
            said(1, "archivist", "!evidence In-flight requests sit at 24."),
            said(
                2,
                "archivist",
                "!support #pool ^1 So the cap is the problem.",
            ),
        ],
        &[],
    );
    // The support is a grounded deposit and specialises, but the self-citation
    // adds no credibility at all.
    assert_eq!(entry(&folded, "archivist", "pool").credibility, 0);
}

#[test]
fn a_refutation_citing_a_deposit_credits_its_author_on_the_refuted_topic() {
    let folded = fold(
        &[
            said(1, "planner", "!propose #retries ^0 Retry storm."),
            said(
                2,
                "archivist",
                "!evidence The retry flag has been off for a week.",
            ),
            said(3, "auditor", "!refute #retries ^2 The path is off."),
        ],
        &[],
    );
    // The room used the archivist's fact to kill `retries`, so that is the
    // topic the archivist is credible on.
    assert!(entry(&folded, "archivist", "retries").credibility > 0);
}

#[test]
fn an_objection_debits_credibility_but_never_below_zero() {
    let cited = [
        said(1, "archivist", "!evidence #pool The pool caps at twenty."),
        said(2, "planner", "!support #pool ^1 So the cap is the problem."),
    ];
    let clean = fold(&cited, &[]);
    let mut objected = cited.to_vec();
    objected.push(said(3, "critic", "!object >1 That reading is stale."));
    let debited = fold(&objected, &[]);
    assert!(
        debited.weight("archivist", &"pool".into()) < clean.weight("archivist", &"pool".into()),
    );

    // Piling objections on cannot drive a member negative.
    let mut piled = objected.clone();
    for sequence in 4..12 {
        piled.push(said(sequence, "critic", "!object >1 Still stale."));
    }
    let floored = fold(&piled, &[]);
    assert_eq!(entry(&floored, "archivist", "pool").credibility, 0);
    assert!(floored.weight("archivist", &"pool".into()) >= 0);
}

#[test]
fn an_ungrounded_position_deposits_nothing() {
    let folded = fold(
        &[
            said(1, "planner", "!propose #pool Raise the cap."),
            said(2, "critic", "!support #pool I agree."),
        ],
        &[],
    );
    assert!(
        folded.entries().is_empty(),
        "a conclusion with no grounds is the cheapest thing to emit and must earn nothing",
    );
}

// --- The prior ---

#[test]
fn priors_alone_reproduce_the_declared_affinity() {
    let priors = [AgentThreshold {
        affinity: vec![("pool".into(), 100), ("retries".into(), 40)],
        ..AgentThreshold::new("archivist", 0)
    }];
    let folded = fold(&[], &priors);
    // 100 * 10 * 10 / 10, and 40 * 10 * 10 / 10.
    assert_eq!(entry(&folded, "archivist", "pool").weight, 1_000);
    assert_eq!(entry(&folded, "archivist", "retries").weight, 400);
    assert!(folded.knows("archivist", &"pool".into(), &wide()));
    assert!(!folded.knows("archivist", &"retries".into(), &wide()));
}

#[test]
fn an_undeclared_affinity_contributes_no_prior() {
    let priors = [AgentThreshold::new("archivist", 0)];
    assert!(fold(&[], &priors).entries().is_empty());

    // And a member who declared nothing is not lifted to the neutral 50 the
    // salience multiplier uses.
    let folded = fold(
        &[said(1, "archivist", "!evidence #pool It caps at twenty.")],
        &priors,
    );
    assert_eq!(entry(&folded, "archivist", "pool").weight, 3_000);
}

// --- Decay and the window ---

#[test]
fn a_deposit_outside_the_window_contributes_nothing() {
    let transcript = [
        said(1, "archivist", "!evidence #pool It caps at twenty."),
        said(80, "planner", "!question What now?"),
    ];
    let policy = DirectoryPolicy {
        window: 10,
        ..DirectoryPolicy::DEFAULT
    };
    let folded = directory(&read(&transcript), Sequence(80), &policy, &[]).expect("folds");
    assert!(folded.entries().is_empty());
}

#[test]
fn an_older_deposit_weighs_less_than_a_newer_one() {
    let transcript = [
        said(1, "archivist", "!evidence #pool It caps at twenty."),
        said(40, "scout", "!evidence #pool It caps at twenty."),
    ];
    let folded = directory(&read(&transcript), Sequence(40), &wide(), &[]).expect("folds");
    assert!(
        entry(&folded, "scout", "pool").weight > entry(&folded, "archivist", "pool").weight,
        "decay is what stops the first speaker holding the directory forever",
    );
    assert_eq!(
        folded
            .top(&"pool".into())
            .map(|held| held.agent_id.as_str()),
        Some("scout")
    );
}

#[test]
fn a_long_transcript_saturates_at_the_ceiling() {
    let transcript: Vec<SessionMessage> = (1..400)
        .map(|sequence| said(sequence, "archivist", "!evidence #pool It caps at twenty."))
        .collect();
    let policy = DirectoryPolicy {
        half_life: u32::MAX,
        window: u32::MAX,
        specialisation: u16::MAX,
        ..DirectoryPolicy::DEFAULT
    };
    let folded = directory(&read(&transcript), Sequence(399), &policy, &[]).expect("folds");
    assert_eq!(entry(&folded, "archivist", "pool").weight, WEIGHT_CEILING);
}

// --- Fold properties ---

#[test]
fn the_fold_is_commutative_under_shuffled_traces() {
    let transcript = [
        said(1, "archivist", "!evidence #pool It caps at twenty."),
        said(2, "planner", "!support #pool ^1 So raise it."),
        said(3, "critic", "!object >1 Stale."),
        said(4, "scout", "!defer #retries Not my area."),
    ];
    let traces = read(&transcript);
    let mut reversed = traces.clone();
    reversed.reverse();
    assert_eq!(
        directory(&traces, Sequence(4), &wide(), &[]).expect("folds"),
        directory(&reversed, Sequence(4), &wide(), &[]).expect("folds"),
    );
}

#[test]
fn the_fold_is_idempotent_under_redelivered_traces() {
    let transcript = [
        said(1, "archivist", "!evidence #pool It caps at twenty."),
        said(2, "planner", "!support #pool ^1 So raise it."),
    ];
    let traces = read(&transcript);
    let doubled: Vec<_> = traces.iter().chain(traces.iter()).cloned().collect();
    assert_eq!(
        directory(&traces, Sequence(2), &wide(), &[]).expect("folds"),
        directory(&doubled, Sequence(2), &wide(), &[]).expect("folds"),
    );
}

// --- Queries ---

#[test]
fn top_among_breaks_ties_by_desk_order() {
    // Two members the host declared equally relevant weigh the same, so the
    // tie is broken by the order the desk supplied.
    let priors = [
        AgentThreshold {
            affinity: vec![("pool".into(), 50)],
            ..AgentThreshold::new("archivist", 0)
        },
        AgentThreshold {
            affinity: vec![("pool".into(), 50)],
            ..AgentThreshold::new("critic", 0)
        },
    ];
    let folded = fold(&[], &priors);
    assert_eq!(
        folded.weight("archivist", &"pool".into()),
        folded.weight("critic", &"pool".into()),
    );
    assert_eq!(
        folded.top_among(&"pool".into(), &["critic", "archivist"]),
        Some("critic"),
    );
    assert_eq!(
        folded.top_among(&"pool".into(), &["archivist", "critic"]),
        Some("archivist"),
    );
    // `top` breaks the same tie by agent id instead, so it is roster-free.
    assert_eq!(
        folded
            .top(&"pool".into())
            .map(|held| held.agent_id.as_str()),
        Some("archivist"),
    );
    // A member with no entry is not a candidate at all.
    assert!(folded.top_among(&"pool".into(), &["scout"]).is_none());
    assert_eq!(folded.topics(), [&TopicId::from("pool")]);
}

#[test]
fn knows_is_false_below_the_floor() {
    let priors = [AgentThreshold {
        affinity: vec![("pool".into(), 10)],
        ..AgentThreshold::new("archivist", 0)
    }];
    let folded = fold(&[], &priors);
    assert_eq!(folded.weight("archivist", &"pool".into()), 100);
    assert!(!folded.knows("archivist", &"pool".into(), &wide()));
    let lenient = DirectoryPolicy {
        floor: 100,
        ..wide()
    };
    assert!(folded.knows("archivist", &"pool".into(), &lenient));
    // A member with no entry at all weighs zero, so any floor above zero
    // keeps it out.
    assert_eq!(folded.weight("scout", &"pool".into()), 0);
    assert!(!folded.knows(
        "scout",
        &"pool".into(),
        &DirectoryPolicy { floor: 1, ..wide() }
    ));
}

#[test]
fn lines_name_every_holder_in_weight_order() {
    let folded = fold(
        &[
            said(1, "archivist", "!evidence #pool It caps at twenty."),
            said(2, "critic", "!propose #pool ^1 Raise it."),
        ],
        &[],
    );
    let lines = folded.lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert!(line.starts_with("#pool: archivist "), "{line}");
    assert!(line.contains(" · critic "), "{line}");
    assert!(line.contains("(spec "), "{line}");
    // Descending weight is what makes the rendering readable as a directory.
    let archivist = folded.weight("archivist", &"pool".into());
    let critic = folded.weight("critic", &"pool".into());
    assert!(archivist > critic, "{archivist} vs {critic}");
}

// --- Deferral ---

#[test]
fn a_deferral_zeroes_its_authors_weight_on_that_topic() {
    let deposited = [
        said(1, "archivist", "!evidence #pool It caps at twenty."),
        said(2, "archivist", "!evidence #retries The flag is off."),
    ];
    let held = fold(&deposited, &[]);
    assert!(held.weight("archivist", &"pool".into()) > 0);

    let mut deferred = deposited.to_vec();
    deferred.push(said(3, "archivist", "!defer #pool Ask the scout."));
    let folded = fold(&deferred, &[]);
    let entry = entry(&folded, "archivist", "pool");
    // The deposit is still recorded; only the estimate is withdrawn.
    assert!(entry.specialisation > 0);
    assert_eq!(entry.weight, 0);
    assert!(!folded.knows("archivist", &"pool".into(), &wide()));
    // And only on the topic that was deferred.
    assert!(folded.weight("archivist", &"retries".into()) > 0);
}
