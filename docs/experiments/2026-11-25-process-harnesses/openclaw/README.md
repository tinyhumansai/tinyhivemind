# OpenClaw run

| File | Does |
| --- | --- |
| `mkcfg.mjs` | writes the Gateway config: one seat (`a`, `b`) or both (`multi`) |
| `gw.mjs` | WebSocket protocol 4 client: Ed25519 device identity, `ready`, `turn` |
| `run.mjs` | the gate and the measurements for both seats in one Gateway |

- Started as `node openclaw.mjs gateway`, with the cleared environment in `run.mjs`.
- Tool search and code mode are off, `tools.allow` is the seat's server and the deny
  groups are in `mkcfg.mjs`. With tool search on, the hive tool was hidden.
- The provider is `openai-completions` with the literal key `dummy`.
- Usage is on the second response to the same `agent` request, read from the frames.
- `left after SIGKILL` counts children that outlived the Gateway; `run.mjs` then
  kills them.
