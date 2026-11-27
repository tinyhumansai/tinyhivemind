# Hive

| File | Purpose |
| --- | --- |
| `model.rs` | A model in Chat Completions, Responses and Anthropic Messages that reads each rendered turn and calls `hivemind_post` and the same call again, then `hivemind_complete`; a seat offered `write_file` posts once and writes, retries, and tries two paths outside its workspace before it completes; started with a `Delegation`, it has the lead ask the seats named, each of them complete in the conversation the ask opened, and the lead complete once a turn shows it every answer |
| `delegation.rs` | `live_*`: a lead-only episode with each harness as the lead. Only the lead starts; it asks the seats on the other two harnesses, each answers in that conversation through its own model API, the lead completes on a later turn showing both answers, and a member nobody asked never runs |
