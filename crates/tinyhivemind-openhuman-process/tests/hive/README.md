# Hive

| File | Purpose |
| --- | --- |
| `model.rs` | A Chat Completions model that reads each rendered turn and calls `hivemind_post`, the same call again, then `hivemind_complete`, recording what it was offered and what each call returned. |
