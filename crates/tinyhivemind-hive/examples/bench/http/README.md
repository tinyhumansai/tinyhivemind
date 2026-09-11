# `http/`

The direct-HTTP backend: driving a seat over a chat-completions endpoint with
`curl`, rather than through an agent CLI. Selected by `--api-base`.

| file | what it holds |
| --- | --- |
| `usage.rs` | what a run spent and how a seat's total reaches the table: `Usage`, the shared `UsageHandle`, and `usage_of`, which recovers a poisoned handle rather than skipping it — a cost report that silently undercounts is the one thing a spend column may not do |
| `test.rs` | the wire-format and request-shaping tests |

`../http.rs` above them holds the request itself: the two wire formats, the
config, the seat types, `ask` (which retries) and `ask_once` (which does not,
for calibration probes — see [`../COST.md`](../COST.md)).
