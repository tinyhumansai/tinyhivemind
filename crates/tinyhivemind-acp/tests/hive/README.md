# Hive

| File | Purpose |
| --- | --- |
| `model.rs` | A model in Chat Completions, Responses and Anthropic Messages that reads each rendered turn and calls `hivemind_post` and the same call again, then `hivemind_complete`; a seat offered `write_file` posts once and writes, retries, and tries two paths outside its workspace before it completes |
