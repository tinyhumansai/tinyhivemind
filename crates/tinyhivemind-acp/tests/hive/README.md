# Hive

| File | Purpose |
| --- | --- |
| `model.rs` | A model in Chat Completions, Responses and Anthropic Messages that reads each rendered turn and calls `hivemind_post`, the same call again, then `hivemind_complete` |
