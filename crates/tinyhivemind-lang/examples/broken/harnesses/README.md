# broken/harnesses

The [`mixed-harnesses`](../../hives/mixed-harnesses/README.md) package with
eight deliberate faults, one per harness diagnostic. `hive_check` reports ten
findings: the provider credential and the runtime sandbox are each rejected
twice, once by the openclaw seat and once by the codex seat, with that kind's
reason. The report is pinned by
`the_broken_harnesses_package_reports_each_harness_finding_once` in
`../../hive_check/test.rs`.

| Fault | Where | Rule |
| --- | --- | --- |
| Seat `lead` names the undeclared harness `ghost` | `hive.json` `/seats/0/harness` | `reference` |
| The harness id `codex` is declared twice | `hive.json` `/harnesses/2/id` | `identity` |
| The `codex` harness asks for `isolation: deployment`, which only `openclaw` accepts | `hive.json` `/harnesses/1/isolation` | `integration` |
| The delegated `helper` profile names a harness | `hive.json` `/profiles/1/harness` | `integration` |
| The `coder` profile delegates to `helper`, and `codex` rejects subagents | `hive.json` `/profiles/0/subagents` | `integration` |
| The `local` provider carries a `credential`, rejected on every kind until a conformance test proves it | `hive.json` `/providers/0/credential` | `integration` |
| `runtime.sandbox` is `read_only`, which no harness can apply to the host's executable tools | `hive.json` `/runtime/sandbox` | `integration` |
| `word_count` declares an argument named `call`, which the host adds to every served tool | `hive.json` `/tools/0/args/properties/call` | `integration` |

| Path | Purpose |
| --- | --- |
| `hive.json` | The faulty manifest |
| `constitution.json`, `memory.json` | As in `mixed-harnesses` |
| `roles/`, `seats/` | Charters and prompts |
| `tools/` | `word_count.py` |
