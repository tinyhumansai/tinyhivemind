# Can OpenHuman, OpenClaw and Codex run as child processes in a hive?

- **Date:** 2026-11-25
- **Status:** Recorded
- **Spec:** [harnesses](../specs/harnesses.md)
- **Decision:** [ADR 0035](../adr/0035-a-harness-is-a-process.md),
  [ADR 0036](../adr/0036-hive-tools-are-served-over-loopback-mcp.md)
- **Code:** [the companion directory](2026-11-25-process-harnesses/README.md): the
  fixture, the exact configs, environment, flags, versions and checksums, and the
  commands that repeat the gate and the measurements. It also lists which spike
  scripts were kept and which were not. The conformance suite in the
  [plan](../plans/2026-11-25-process-harnesses.md) is the durable replacement.

## Question and gate

Written before the runs: can each harness, as a supervised process, list and call
`hivemind_post` over HTTP MCP as the right seat, keep a session, cancel, and report
usage? OpenHuman or OpenClaw failing to call a hive tool or keep a session stops
the work. A Codex failure would drop only Codex.

## Method

A scripted model (Chat Completions and Responses) and a bearer-checked MCP stub on
loopback, shared by all three. Versions: openhuman-core 0.64.15 (release, sha256
checked), OpenClaw 2026.10.33 on Node 24.22.3, codex-cli 0.171.0, Linux x86_64. No
real model and no login: the fixture ignored the key on model requests, so no
provider credential was tested. A mock proves wiring and says nothing of model behaviour.
Each harness was tested by a separate agent with the same checklist, on a shared
machine, so timings are indicative.

## Result: the gate passes

| | OpenHuman `web_chat` | OpenClaw Gateway | Codex app-server |
| --- | --- | --- | --- |
| Hive tool, right seat | yes | yes, with tool search off | yes, with a pinned catalog |
| Same session after restart | yes | yes | yes |
| Per-turn usage | yes | yes, second `agent` response | difference of thread totals |
| Cancel closes the model call | 1 ms | about 30 ms | 4 ms |
| kill -9 mid-turn | no replay | **resumes by itself in about 5 s** | no replay |
| Durable idempotency key | no | no, in-process only | no |
| Second message while busy | interrupts | queues | steers |
| Idle memory | 53 MB RSS, 23 PSS | 1.4 to 1.8 GB PSS | 127 MB RSS |
| Memory after turns | about 144 MB RSS, 100 PSS | 10 to 150 MB more | 160 MB RSS, two threads |
| Cold start | 0.28 to 0.83 s | 6.3 to 7.9 s | 64 ms |
| Median turn, plain | 45 ms | about 200 ms | 19 ms |
| Median turn, one tool call | not recorded | about 230 ms | 30 ms |
| Tools at strictest setting | hive tool and `juice_retrieve` | hive tool only | hive tool and three MCP resource readers |

### Linked baseline

Vendored OpenHuman `5f8b7c23`, release build, one agent, a native `hivemind_post`,
the same scripted model; three process runs, median of turns 4 to 8 in each.

| | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Boot | 1217 ms | 364 ms | 362 ms |
| Idle RSS / PSS | 63 / 60 MB | 60 / 57 MB | 61 / 59 MB |
| RSS / PSS after 16 turns | 174 / 172 MB | 163 / 161 MB | 174 / 172 MB |
| Plain turn | 95 ms | 99 ms | 99 ms |
| Tool turn | 120 ms | 128 ms | 122 ms |

**This is not like for like.** The linked agent offered about 47 tools in a 38 KB
request, where the binary offered one or two. The revisions differ, and the load
average was 35. So it does not show that a process turn is faster than a linked
one. It supports only that the process boundary did not add an order of magnitude.
The linked cost is also shared: further seats join one process, while each
OpenHuman or Codex seat adds a process.

## What changed the design

1. **A bearer is not enough.** OpenClaw's crash replay would carry a valid bearer
   into a later turn. State-changing calls must name their turn.
2. **A denylist is not enough.** With `"*"` and a denylist, OpenHuman's
   `mcp_call_tool` posted as the other seat. Exact allowlists were enforced.
3. **Defaults hide tools.** OpenClaw's tool search and Codex's model catalog each
   hid the hive tool. OpenHuman's first turn ran before MCP was warm. The
   effective tool list must be checked at start.
4. **OpenHuman fails quietly.** Bad TOML resets to defaults, a busy port moves, and
   `upsert_custom` drops `tool_rules`, which 0.64.15 lacks. A custom agent stops at
   eight iterations, reported by `web_chat` as a normal end. `agent_chat` skips
   approvals.
5. **Per-seat OpenClaw is too large** at its measured footprint. Two seats in one
   Gateway did not see or call each other's tool, though one process holds both
   bearers.
6. **Process groups miss children.** OpenClaw's broker has its own session, and
   Codex shell children outlived interrupt and kill -9.

## Side effects of the runs

- OpenClaw's first run wrote a log under `/tmp/openclaw` and fetched its hosted
  model catalog, before both were configured off.
- Codex defaults fetched a plugin repository from GitHub. With the final config an
  idle thread was traced for 25 s and used loopback only.
- OpenHuman logged its hosted provider for one prompt before `primary_cloud` was
  set. Whether a request left the machine was not established. Only mock text and
  a dummy key were involved. Later live runs block egress.

## Not tested

Real models; provider credentials, and any provider other than one custom endpoint
per harness; compaction and long sessions; concurrent seats under load; provider
rate limits; OpenClaw's documented sub-agent auto-archive; memory per extra agent
in one Gateway; harness built-in tools under a sandbox; OpenHuman's
`tool_rules` in any release; macOS and ARM; coordinator-run turns over a real
harness. Some early OpenClaw outputs were not saved to files.
