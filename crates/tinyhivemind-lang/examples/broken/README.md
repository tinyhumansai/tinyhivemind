# Broken packages

Packages that deliberately fail `hive_check`, one finding per rule they
exercise, so the report a host or editor sees is pinned by a test.

| Directory | Findings |
| --- | --- |
| `integrations/` | The `integrations` example with an undeclared seat model (`reference`), `tools/word_count.py` removed (`missing_document`), a delegated profile that sets a model, an `argv` placeholder naming no argument, and `sqlite` sessions without a workspace (`integration`). Its [README](integrations/README.md) lists each fault and where it is located |

```sh
cargo run -p tinyhivemind-lang --example hive_check -- crates/tinyhivemind-lang/examples/broken/integrations
```
