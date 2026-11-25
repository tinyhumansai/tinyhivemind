# mixed-harnesses

One floor of three seats, each on a different harness, that `check` accepts
under the [harness capability rules](../../../../../docs/specs/harnesses.md#capabilities).
It is the smallest package a process deployment could run: everything it
sets is a `yes` row of the table, and nothing it sets is rejected.

| Path | Purpose |
| --- | --- |
| `hive.json` | The roster, a `harnesses` section, and integration settings every kind honours |
| `constitution.json` | Default caps; `permitted_edits` is empty |
| `memory.json` | No memory |
| `roles/` | Lead, researcher and builder charters |
| `seats/` | One prompt per seat |
| `tools/` | `word_count.py`, the one executable tool |

What it shows:

- `harnesses` declares `claw`, an OpenClaw harness shared by the deployment
  (`isolation: deployment`), and `codex`, which takes its default of one
  instance per seat.
- `lead` names no harness, so it is an `openhuman` seat; `scout` names
  `claw` itself; `builder` gets `codex` from its `coder` profile. A seat's own
  `harness` beats its profile's, which beats `runtime.harness`.
- The one provider is `openai` with an explicit loopback `endpoint` and no
  `credential`, the only provider shape every kind is known to run. The
  runtime sandbox is `none`, the toolpack turns no built-in tool group on,
  and `session` and `jev` are left at their defaults.

Because it declares harnesses, every seat, the implicit `openhuman` one
included, is held to the capability table. The linked OpenHuman adapter
refuses to deploy it: until the process deployment exists, a seat on any
harness but `openhuman` cannot start.
