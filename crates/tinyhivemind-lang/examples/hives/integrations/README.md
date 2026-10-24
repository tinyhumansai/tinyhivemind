# integrations

Every integration section of `hive.json` in one small package: a two-seat
release desk (a lead and a scout) that shows what a hive's agents run with,
not only who they are. Together with
[`examples/broken/integrations`](../../broken/integrations/README.md) it pins
what `check` accepts and what it reports.

| Path | Purpose |
| --- | --- |
| `hive.json` | The roster plus `runtime`, `providers`, `models`, `mcp`, `skills`, `tools`, `toolpacks`, `permissions`, `profiles`, `hooks`, `session`, `jev`, `features` and `workflows` |
| `constitution.json` | Default caps; `permitted_edits` is empty, so a self-edit may change nothing |
| `memory.json` | No memory |
| `roles/` | Lead and researcher charters |
| `seats/` | One prompt per seat |
| `context/` | `style.md`, a shared document the hive composes into prompts |
| `tools/` | Executable tool sources: `grep_repo.sh` and `word_count.py` are model-callable tools, `guard_shell.sh` is the script behind the `no_force_push` hook |
| `skills/release-notes/` | A skill bundle; `SKILL.md` is required by validation |

What the sections show:

- `runtime` sets a concurrency of 2, a default provider, model and toolpack,
  and a 120 s / $0.50 turn limit that every seat inherits.
- `lead` takes the `editor` profile (a skill and a delegate, `summariser`) and
  the `no_force_push` hook; `scout` takes the `reader` profile, the `fast`
  model and the `read_only` permission set.
- `repo` is a toolpack bundling two tools and the `issues` MCP server, and
  turning the `web3` and `scheduling` tool groups off.
- `no_force_push` is a fail-closed `pre_tool_use` hook on `shell` that runs the
  `guard_shell` tool; `nightly-digest` is a cron workflow aimed at the hive.

`parse` keeps the files under `tools/` and `skills/` as package files, so
scripts take part in the package's content identity. Run it with
`cargo run -p tinyhivemind-lang --example hive_check -- <this directory>`;
the OpenHuman adapter deploys it with its
[`run_package`](../../../../tinyhivemind-openhuman/examples/run_package/README.md)
example.
