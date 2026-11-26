# Broken packages

Packages that deliberately fail `hive_check`, one finding per rule they
exercise, so the report a host or editor sees is pinned by a test.

| Directory | Findings |
| --- | --- |
| `integrations/` | The `integrations` example with an undeclared seat model (`reference`), `tools/word_count.py` removed (`missing_document`), a delegated profile that sets a model, an `argv` placeholder naming no argument, and `sqlite` sessions without a workspace (`integration`). Its [README](integrations/README.md) lists each fault and where it is located |
| `harnesses/` | The `mixed-harnesses` example with an undeclared harness (`reference`), a duplicate harness id (`identity`), and `integration` findings for `deployment` isolation on `codex`, a harness on a delegated profile, subagents on `codex`, a provider credential, a runtime sandbox and a tool argument named `call` (ten findings: two faults are reported once per rejecting kind). Its [README](harnesses/README.md) lists each one |

```sh
cargo run -p tinyhivemind-lang --example hive_check -- crates/tinyhivemind-lang/examples/broken/integrations
cargo run -p tinyhivemind-lang --example hive_check -- crates/tinyhivemind-lang/examples/broken/harnesses
```
