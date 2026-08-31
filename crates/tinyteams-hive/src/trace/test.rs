//! Unit tests for the stigmergic grammar and its read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn agent(id: &str) -> SessionAuthor {
    SessionAuthor::Agent {
        id: id.into(),
        label: id.into(),
    }
}

fn said(sequence: u64, author: SessionAuthor, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author,
        content: content.into(),
    }
}

fn only(body: &str) -> Trace {
    let traces = resolve(body, None, &agent("planner"), Sequence(7));
    assert_eq!(traces.len(), 1, "expected exactly one trace in {body:?}");
    traces.into_iter().next().unwrap()
}

fn assert_wire_round_trip<T>(value: &T, expected: serde_json::Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Eq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_value(value).expect("serializes"), expected);
    assert_eq!(
        serde_json::from_value::<T>(expected).expect("deserializes"),
        *value
    );
}

#[test]
fn a_trace_pins_its_wire_form() {
    let trace = Trace {
        sequence: Sequence(7),
        author: agent("planner"),
        kind: TraceKind::Support,
        topic: Some(TopicId("stage".into())),
        target: Some(Sequence(3)),
        cites: vec![Sequence(1), Sequence(2)],
        text: "!support #stage >3 ^1 ^2 because".into(),
        offset: 4,
    };
    assert_wire_round_trip(
        &trace,
        serde_json::json!({
            "sequence": 7,
            "author": { "type": "agent", "id": "planner", "label": "planner" },
            "kind": "support",
            "topic": "stage",
            "target": 3,
            "cites": [1, 2],
            "text": "!support #stage >3 ^1 ^2 because",
            "offset": 4,
        }),
    );
}

#[test]
fn every_trace_kind_pins_its_wire_spelling() {
    for (kind, spelling) in [
        (TraceKind::Propose, "propose"),
        (TraceKind::Support, "support"),
        (TraceKind::Object, "object"),
        (TraceKind::Evidence, "evidence"),
        (TraceKind::Question, "question"),
        (TraceKind::Commit, "commit"),
    ] {
        assert_wire_round_trip(&kind, serde_json::json!(spelling));
    }
}

#[test]
fn an_optional_trace_field_is_required_in_json() {
    let missing = serde_json::json!({
        "sequence": 7,
        "author": { "type": "agent", "id": "planner", "label": "planner" },
        "kind": "question",
        "target": null,
        "cites": [],
        "text": "!question",
        "offset": 0,
    });
    assert!(serde_json::from_value::<Trace>(missing).is_err());
}

#[test]
fn every_marker_spelling_is_recognized() {
    for (body, kind) in [
        ("!propose #a", TraceKind::Propose),
        ("!support #a", TraceKind::Support),
        ("!object >1", TraceKind::Object),
        ("!evidence ^1", TraceKind::Evidence),
        ("!question", TraceKind::Question),
        ("!commit #a", TraceKind::Commit),
    ] {
        assert_eq!(only(body).kind, kind, "for {body:?}");
    }
}

#[test]
fn a_body_without_a_marker_deposits_nothing() {
    assert!(resolve("Just talking it over.", None, &agent("a"), Sequence(1)).is_empty());
    assert!(resolve("", None, &agent("a"), Sequence(1)).is_empty());
    assert!(resolve("!shout at everyone", None, &agent("a"), Sequence(1)).is_empty());
    assert!(resolve("!", None, &agent("a"), Sequence(1)).is_empty());
}

#[test]
fn a_marker_must_lead_its_line() {
    assert!(resolve("I would !propose #a", None, &agent("a"), Sequence(1)).is_empty());
    assert_eq!(only("   !propose #a").kind, TraceKind::Propose);
}

#[test]
fn an_indented_marker_reports_the_offset_of_its_bang() {
    let trace = only("  !question");
    assert_eq!(trace.offset, 2);
    assert_eq!(trace.text, "!question");
}

#[test]
fn a_marker_on_a_later_line_reports_a_utf8_byte_offset() {
    let traces = resolve("café ☕\n!propose #a", None, &agent("a"), Sequence(1));
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].offset, "café ☕\n".len());
}

#[test]
fn a_marker_inside_a_fenced_block_is_masked() {
    let body = "before\n```\n!propose #hidden\n```\n!propose #real";
    let traces = resolve(body, None, &agent("a"), Sequence(1));
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].topic, Some(TopicId("real".into())));
}

#[test]
fn an_unclosed_fence_masks_to_the_end_of_the_body() {
    let body = "```\n!propose #hidden\n!support #hidden ^1";
    assert!(resolve(body, None, &agent("a"), Sequence(1)).is_empty());
}

#[test]
fn a_backticked_marker_is_not_line_leading_and_needs_no_masking() {
    assert!(resolve("`!propose #a`", None, &agent("a"), Sequence(1)).is_empty());
}

#[test]
fn a_marker_parses_its_topic_target_and_citations() {
    let trace = only("!object #stage >12 ^3 ^4 the control is missing");
    assert_eq!(trace.topic, Some(TopicId("stage".into())));
    assert_eq!(trace.target, Some(Sequence(12)));
    assert_eq!(trace.cites, [Sequence(3), Sequence(4)]);
    assert!(trace.grounded());
}

#[test]
fn a_repeated_citation_is_recorded_once() {
    assert_eq!(only("!support #a ^3 ^3 ^4").cites, [Sequence(3), Sequence(4)]);
}

#[test]
fn only_the_first_topic_and_target_are_taken() {
    let trace = only("!object #first #second >1 >2");
    assert_eq!(trace.topic, Some(TopicId("first".into())));
    assert_eq!(trace.target, Some(Sequence(1)));
}

#[test]
fn an_empty_or_unparsable_qualifier_is_ignored() {
    let trace = only("!propose # >x ^y");
    assert_eq!(trace.topic, None);
    assert_eq!(trace.target, None);
    assert!(trace.cites.is_empty());
    assert!(!trace.grounded());
}

#[test]
fn a_trace_without_citations_is_not_grounded() {
    assert!(!only("!support #a").grounded());
}

#[test]
fn a_non_agent_author_has_no_agent_id() {
    let operator = resolve("!question", None, &SessionAuthor::Operator, Sequence(1));
    assert_eq!(operator[0].agent_id(), None);
    let system = resolve(
        "!question",
        None,
        &SessionAuthor::System {
            kind: "workflow".into(),
            label: "Workflow".into(),
        },
        Sequence(1),
    );
    assert_eq!(system[0].agent_id(), None);
    let person = resolve(
        "!question",
        None,
        &SessionAuthor::Person {
            id: "p1".into(),
            label: "Ada".into(),
        },
        Sequence(1),
    );
    assert_eq!(person[0].agent_id(), None);
    assert_eq!(only("!question").agent_id(), Some("planner"));
}

#[test]
fn a_body_deposits_at_most_the_trace_cap() {
    let body = "!question\n".repeat(TRACE_CAP + 10);
    assert_eq!(
        resolve(&body, None, &agent("a"), Sequence(1)).len(),
        TRACE_CAP
    );
}

#[test]
fn a_supplied_list_is_authoritative_and_revalidated() {
    let body = "!propose #real";
    let supplied = vec![
        Trace {
            sequence: Sequence(99),
            author: agent("impostor"),
            kind: TraceKind::Propose,
            topic: Some(TopicId("rewritten".into())),
            target: None,
            cites: vec![Sequence(5)],
            text: "!propose #real".into(),
            offset: 0,
        },
        Trace {
            sequence: Sequence(99),
            author: agent("impostor"),
            kind: TraceKind::Commit,
            topic: None,
            target: None,
            cites: Vec::new(),
            text: "never authored".into(),
            offset: 40,
        },
    ];

    let traces = resolve(body, Some(supplied), &agent("planner"), Sequence(7));

    // The unauthored trace is dropped; the authored one keeps its supplied
    // topic and grounds but is re-attributed to the real author and sequence.
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].topic, Some(TopicId("rewritten".into())));
    assert_eq!(traces[0].agent_id(), Some("planner"));
    assert_eq!(traces[0].sequence, Sequence(7));
}

#[test]
fn an_empty_supplied_list_deposits_nothing() {
    assert!(resolve("!propose #a", Some(Vec::new()), &agent("a"), Sequence(1)).is_empty());
}

#[test]
fn reading_a_transcript_orders_traces_by_sequence_then_offset() {
    let transcript = [
        said(3, agent("critic"), "!support #a ^1"),
        said(1, agent("planner"), "!propose #a\n!evidence ^0"),
        said(2, agent("scout"), "ordinary conversation"),
    ];
    let traces = read(&transcript);
    assert_eq!(traces.len(), 3);
    assert_eq!(
        traces
            .iter()
            .map(|trace| (trace.sequence.0, trace.kind))
            .collect::<Vec<_>>(),
        [
            (1, TraceKind::Propose),
            (1, TraceKind::Evidence),
            (3, TraceKind::Support),
        ],
    );
}

#[test]
fn a_transcript_of_ordinary_conversation_folds_to_an_empty_medium() {
    let transcript = [
        said(1, SessionAuthor::Operator, "What should we do?"),
        said(2, agent("planner"), "I think we should stage it."),
    ];
    assert!(read(&transcript).is_empty());
}

#[test]
fn a_topic_id_displays_and_converts_from_both_string_forms() {
    let from_str: TopicId = "stage".into();
    let from_string: TopicId = String::from("stage").into();
    assert_eq!(from_str, from_string);
    assert_eq!(from_str.as_str(), "stage");
    assert_eq!(from_str.to_string(), "stage");
}
