//! Unit tests for folding one agent CLI's event stream into a turn.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::{extract_post, parse_events};

#[test]
fn counts_reads_and_records_each_written_path_once() {
    let stream = concat!(
        r#"{"type":"tool_use","part":{"tool":"read","state":{"input":{"filePath":"/ws/NOTES.md"}}}}"#,
        "\n",
        r#"{"type":"tool_use","part":{"tool":"read","state":{"input":{"filePath":"/ws/a.py"}}}}"#,
        "\n",
        r#"{"type":"tool_use","part":{"tool":"write","state":{"input":{"filePath":"/ws/b.py","content":"x"}}}}"#,
        "\n",
        r#"{"type":"tool_use","part":{"tool":"edit","state":{"input":{"filePath":"/ws/b.py","oldString":"x","newString":"y"}}}}"#,
        "\n",
        r#"{"type":"tool_use","part":{"tool":"bash","state":{"input":{"command":"python3 b.py"}}}}"#,
        "\n",
        r#"{"type":"tool_use","part":{"tool":"write","state":{"input":{"filePath":"/ws/notebooks/solver.md","content":"n"}}}}"#,
        "\n",
        r#"{"type":"text","part":{"text":"<<<POST\n@checker b.py runs\nPOST>>>"}}"#,
    );
    let turn = parse_events(stream);
    assert_eq!(turn.reads, 2);
    assert_eq!(turn.files_written, ["/ws/b.py", "/ws/notebooks/solver.md"]);
    assert_eq!(
        turn.tools,
        ["read", "read", "write", "edit", "bash", "write"]
    );
    assert_eq!(turn.message, "@checker b.py runs");
}

#[test]
fn takes_the_marked_block_over_surrounding_narration() {
    let raw = "thinking out loud\n<<<POST\n@checker ready\nPOST>>>\ndone";
    assert_eq!(extract_post(raw), "@checker ready");
}

#[test]
fn falls_back_to_the_whole_text_when_unmarked() {
    assert_eq!(extract_post("  plain answer \n"), "plain answer");
}

#[test]
fn reads_a_symmetric_fence_a_seat_wrote_instead() {
    let raw = "<<<POST>>>\n@checker the recursion is sublinear\n<<<POST>>>";
    assert_eq!(extract_post(raw), "@checker the recursion is sublinear");
}

#[test]
fn takes_the_last_block_when_a_seat_posts_twice() {
    let raw = "<<<POST\nfirst draft\nPOST>>>\n<<<POST\nsecond and final\nPOST>>>";
    assert_eq!(extract_post(raw), "second and final");
}

#[test]
fn keeps_an_unterminated_block_rather_than_dropping_it() {
    let raw = "narration\n<<<POST\n@lead ran out of room";
    assert_eq!(extract_post(raw), "@lead ran out of room");
}
