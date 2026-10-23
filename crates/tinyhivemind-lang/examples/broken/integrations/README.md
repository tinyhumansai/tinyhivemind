# broken/integrations

The [`integrations`](../../hives/integrations/README.md) package with five
deliberate faults, so `hive_check` reports one finding per rule. The report is
pinned by `the_broken_integrations_package_reports_each_rule_once` in
`../../hive_check/test.rs`.

| Fault | Where | Rule |
| --- | --- | --- |
| Seat `lead` names the undeclared model `turbo` | `hive.json` `/seats/0/model` | `reference` |
| `tools/word_count.py` is absent though `word_count` runs it | `hive.json` `/tools/1/command` | `missing_document` |
| The delegated `summariser` profile sets the model `fast` | `hive.json` `/profiles/2/model` | `integration` |
| `grep_repo`'s `argv` names `{depth}`, which is no argument | `hive.json` `/tools/0/argv` | `integration` |
| `session_store` is `sqlite` but `runtime.workspace` is unset | `hive.json` `/runtime/session_store` | `integration` |

| Path | Purpose |
| --- | --- |
| `hive.json` | The faulty manifest |
| `constitution.json`, `memory.json` | As in `integrations` |
| `roles/`, `seats/`, `context/` | Charters, prompts and the shared style document |
| `tools/` | The two scripts that remain: `grep_repo.sh`, `guard_shell.sh` |
| `skills/release-notes/` | The skill bundle |
