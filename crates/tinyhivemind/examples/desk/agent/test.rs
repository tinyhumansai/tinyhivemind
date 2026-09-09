//! Unit tests for folding one agent CLI's event stream into a turn.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::parse_events;

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
