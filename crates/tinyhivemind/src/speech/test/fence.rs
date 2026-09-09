//! The delimiter, and the spelling that cost a run.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::speech::fence::extract_post;

#[test]
fn reads_the_fence_the_brief_asks_for() {
    assert_eq!(
        extract_post("thinking out loud\n<<<POST\nB holds at 10^18\nPOST>>>\n"),
        "B holds at 10^18",
    );
}

#[test]
fn accepts_the_symmetric_closing_fence() {
    // Run 26 lost a verified sublinear recursion to exactly this: the last
    // marker was read as an opener and the room received `>>>`.
    assert_eq!(
        extract_post("<<<POST>>>\nB(x,n) matches at 160 pairs\n<<<POST>>>"),
        "B(x,n) matches at 160 pairs",
    );
    assert_ne!(extract_post("<<<POST>>> body <<<POST>>>"), ">>>");
}

#[test]
fn the_last_opener_wins_so_quoting_the_instruction_costs_nothing() {
    assert_eq!(
        extract_post(
            "I was told to wrap it in <<<POST and POST>>>.\n<<<POST\nthe real answer\nPOST>>>"
        ),
        "the real answer",
    );
}

#[test]
fn text_with_no_fence_at_all_is_still_what_the_seat_said() {
    assert_eq!(
        extract_post("  the tools were not attached, so: B holds  "),
        "the tools were not attached, so: B holds",
    );
    assert_eq!(extract_post("   "), "");
}

#[test]
fn an_opener_with_nothing_after_it_yields_nothing_rather_than_a_marker() {
    assert_eq!(extract_post("<<<POST"), "");
    assert_eq!(extract_post("<<<POST>>>"), "");
}

#[test]
fn takes_the_last_block_when_a_seat_posts_twice() {
    assert_eq!(
        extract_post("<<<POST\nfirst draft\nPOST>>>\n<<<POST\nsecond and final\nPOST>>>"),
        "second and final",
    );
}

#[test]
fn keeps_an_unterminated_block_rather_than_dropping_it() {
    assert_eq!(
        extract_post("narration\n<<<POST\n@lead ran out of room"),
        "@lead ran out of room",
    );
}

#[test]
fn takes_the_marked_block_over_surrounding_narration() {
    assert_eq!(
        extract_post("thinking out loud\n<<<POST\n@checker ready\nPOST>>>\ndone"),
        "@checker ready",
    );
}
