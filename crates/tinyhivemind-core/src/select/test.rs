//! Unit tests for the shared selection ranking.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    Candidate, DENSITY_SCALE, MatchField, MatchKind, Pattern, rank, regex_source, score,
    score_pattern,
};

#[test]
fn scores_an_exact_match_highest() {
    let matched = score("alice", "Alice").expect("exact match");
    assert_eq!(matched.kind, MatchKind::Exact);
    assert_eq!(matched.score, MatchKind::Exact.base() + DENSITY_SCALE);
    assert_eq!(matched.offset, 0);
}

#[test]
fn ranks_a_prefix_above_a_word_prefix_above_a_substring() {
    let prefix = score("ship", "shipping").expect("prefix");
    let word = score("ship", "we ship weekly").expect("word prefix");
    let inside = score("ship", "airship").expect("substring");
    assert_eq!(prefix.kind, MatchKind::Prefix);
    assert_eq!(word.kind, MatchKind::WordPrefix);
    assert_eq!(inside.kind, MatchKind::Substring);
    assert!(prefix.score > word.score);
    assert!(word.score > inside.score);
}

#[test]
fn reports_the_offset_of_a_word_prefix_over_an_earlier_substring() {
    let matched = score("ship", "airship ships").expect("word prefix wins");
    assert_eq!(matched.kind, MatchKind::WordPrefix);
    assert_eq!(matched.offset, 8);
}

#[test]
fn matches_an_in_order_subsequence_last() {
    let matched = score("rlm", "recall limits messages").expect("subsequence");
    assert_eq!(matched.kind, MatchKind::Subsequence);
    assert_eq!(matched.offset, 0);
    assert!(matched.score < MatchKind::Substring.base());
}

#[test]
fn rejects_a_query_that_is_not_a_subsequence() {
    assert!(score("zzz", "recall").is_none());
}

#[test]
fn rejects_a_blank_query_and_blank_text() {
    assert!(score("   ", "anything").is_none());
    assert!(score("query", "   ").is_none());
}

#[test]
fn rejects_a_query_longer_than_the_text() {
    assert!(score("engineering", "eng").is_none());
}

#[test]
fn prefers_the_denser_of_two_equal_kinds() {
    let tight = score("ship", "ships").expect("prefix");
    let loose = score("ship", "shipping schedules").expect("prefix");
    assert_eq!(tight.kind, loose.kind);
    assert!(tight.score > loose.score);
}

#[test]
fn ranks_a_label_match_above_a_detail_match() {
    let candidates = [
        Candidate::new("bob", "Bob").with_detail("keeps the shipping calendar"),
        Candidate::new("shipping", "Shipping"),
    ];
    let hits = rank("shipping", &candidates, 8);
    assert_eq!(hits[0].id, "shipping");
    assert_eq!(hits[0].field, MatchField::Label);
    assert_eq!(hits[1].id, "bob");
    assert_eq!(hits[1].field, MatchField::Detail);
    // Half of a word-prefix tier plus its density: (600 + 29) / 2.
    assert_eq!(
        hits[1].score,
        u32::midpoint(MatchKind::WordPrefix.base(), 29)
    );
}

#[test]
fn reports_an_id_match_when_the_label_does_not_match() {
    let candidates = [Candidate::new("alice", "Nakamura")];
    let hits = rank("alice", &candidates, 8);
    assert_eq!(hits[0].field, MatchField::Id);
    assert_eq!(hits[0].kind, MatchKind::Exact);
}

#[test]
fn breaks_a_tie_by_offset_then_length_then_candidate_order() {
    let candidates = [
        Candidate::new("second", "ship it"),
        Candidate::new("first", "ship"),
    ];
    let hits = rank("ship", &candidates, 8);
    assert_eq!(hits[0].id, "first");
    assert_eq!(hits[1].id, "second");
}

#[test]
fn keeps_at_most_the_limit_and_nothing_at_zero() {
    let candidates = [
        Candidate::new("a", "ship one"),
        Candidate::new("b", "ship two"),
        Candidate::new("c", "ship three"),
    ];
    assert_eq!(rank("ship", &candidates, 2).len(), 2);
    assert!(rank("ship", &candidates, 0).is_empty());
}

#[test]
fn returns_nothing_when_no_candidate_matches() {
    let candidates = [Candidate::new("alice", "Alice")];
    assert!(rank("zzzz", &candidates, 8).is_empty());
}

#[test]
fn reads_a_delimited_regular_expression_source() {
    assert_eq!(regex_source("/^ship(ped)?/"), Some("^ship(ped)?"));
    assert_eq!(regex_source("  /ship/  "), Some("ship"));
    assert_eq!(regex_source("shipped"), None);
    assert_eq!(regex_source("/"), None);
    assert_eq!(regex_source("//"), None);
}

#[test]
fn scores_a_text_pattern_exactly_as_a_query() {
    let pattern = Pattern::Text("ship");
    assert_eq!(
        score_pattern(&pattern, "shipping"),
        score("ship", "shipping")
    );
}

#[test]
fn converts_a_query_into_a_text_pattern() {
    let pattern = Pattern::from("ship");
    assert_eq!(
        score_pattern(&pattern, "shipping"),
        score("ship", "shipping")
    );
}

#[test]
fn pins_the_wire_form_of_a_text_match() {
    let matched = score("ship", "airship").expect("substring");
    let json = serde_json::to_value(matched).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!({ "kind": "substring", "score": 457, "offset": 3 })
    );
}

#[test]
fn pins_the_wire_form_of_a_match_field() {
    assert_eq!(
        serde_json::to_value(MatchField::Detail).expect("serializes"),
        serde_json::json!("detail")
    );
}

#[cfg(feature = "regex")]
mod expressions {
    use super::super::rank_pattern;
    use super::{Candidate, MatchKind, Pattern, score_pattern};
    use regex::Regex;

    fn compiled(source: &str) -> Regex {
        Regex::new(source).expect("test expression compiles")
    }

    #[test]
    fn reads_a_span_covering_the_whole_text_as_exact() {
        let expression = compiled("^ship(ped)?$");
        let matched = score_pattern(&Pattern::Regex(&expression), "shipped").expect("matches");
        assert_eq!(matched.kind, MatchKind::Exact);
        assert_eq!(matched.offset, 0);
    }

    #[test]
    fn reads_a_leading_span_as_a_prefix_and_an_interior_span_as_a_substring() {
        let expression = compiled("ship");
        let prefix = score_pattern(&Pattern::Regex(&expression), "shipping").expect("prefix");
        let inside = score_pattern(&Pattern::Regex(&expression), "airship").expect("substring");
        assert_eq!(prefix.kind, MatchKind::Prefix);
        assert_eq!(inside.kind, MatchKind::Substring);
        assert_eq!(inside.offset, 3);
    }

    #[test]
    fn reads_a_span_after_a_boundary_as_a_word_prefix() {
        let expression = compiled("shi[a-z]+");
        let matched =
            score_pattern(&Pattern::Regex(&expression), "we shipped it").expect("word prefix");
        assert_eq!(matched.kind, MatchKind::WordPrefix);
        assert_eq!(matched.offset, 3);
    }

    #[test]
    fn scores_a_zero_width_match_as_the_weakest_substring() {
        let expression = compiled("^");
        let matched = score_pattern(&Pattern::Regex(&expression), "anything").expect("matches");
        assert_eq!(matched.kind, MatchKind::Substring);
        assert_eq!(matched.score, MatchKind::Substring.base());
    }

    #[test]
    fn returns_nothing_when_the_expression_does_not_match() {
        let expression = compiled("^ship$");
        assert!(score_pattern(&Pattern::Regex(&expression), "shipping").is_none());
    }

    #[test]
    fn ranks_candidates_against_an_expression() {
        let expression = compiled("(?i)^ali");
        let candidates = [
            Candidate::new("bob", "Bob"),
            Candidate::new("alice", "Alice"),
        ];
        let hits = rank_pattern(&Pattern::Regex(&expression), &candidates, 8);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "alice");
    }

    #[test]
    fn converts_a_compiled_expression_into_a_pattern() {
        let expression = compiled("ship");
        let pattern = Pattern::from(&expression);
        assert!(score_pattern(&pattern, "shipping").is_some());
    }
}
