# Hive

| File | Purpose |
| --- | --- |
| `model.rs` | A model in Chat Completions, Responses and Anthropic Messages that reads each rendered turn and calls `hivemind_post` and the same call again, then `hivemind_complete`; a seat offered `write_file` posts once and writes, retries, and tries two paths outside its workspace before it completes; started with a `Delegation`, it has the lead ask the seats named, each of them complete in the conversation the ask opened, and the lead complete once a turn shows it every answer |
| `edit.rs` | `live_*`: every harness editing files only through the host's `write_file` tool, each in its own workspace: a retried write runs once, writes that climb out of the workspace or are absolute are refused, a supervised seat's writes go to the host's approver, and a read-only seat and a denied seat write nothing; once with OpenHuman, Claude Code and Codex writing, and once with an OpenClaw writer beside them |
| `delegation.rs` | `live_*`: lead-only episodes. Only the lead starts; it asks the seats on the other harnesses, each answers in that conversation through its own model API, the lead completes on a later turn showing every answer, and a member nobody asked never runs. Each of OpenHuman, Claude Code and Codex leads the other two; then OpenClaw leads, and is asked by Codex, with all four in the hive |
