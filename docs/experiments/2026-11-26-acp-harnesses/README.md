# Repeating the ACP harness runs

Companion to [the record](../2026-11-26-acp-harnesses.md).

| File | Does |
| --- | --- |
| `fixture.mjs` | the 2026-11-25 fixture plus Anthropic's Messages API: a scripted model and an MCP stub that maps `seat-a-token` to seat `a` |
| `acp.mjs` | a minimal ACP client over stdio, and process-tree memory |
| `measure.mjs` | one harness's gate and measurements, with its per-harness setup |

The Codex and OpenClaw setups reuse the spike's `codex/catalog.mjs`,
`codex/config.toml.in` and `openclaw/mkcfg.mjs`.

## Run

Install each harness in its own directory under `<pkgs>`:

```sh
npm install @agentclientprotocol/claude-agent-acp@0.112.0   # in <pkgs>/claude
npm install @agentclientprotocol/codex-acp@2.12.1           # in <pkgs>/codex
npm install openclaw@2026.10.33                             # in <pkgs>/openclaw
```

Then, per harness, start a fixture and measure against it:

```sh
node fixture.mjs 19701 /abs/runs/claude-fx.log &
node measure.mjs claude   /abs/pkgs /abs/runs/claude   19701 /abs/runs/claude-fx.log
node measure.mjs codex    /abs/pkgs /abs/runs/codex    19702 /abs/runs/codex-fx.log
node measure.mjs openclaw /abs/pkgs /abs/runs/openclaw 19703 /abs/runs/openclaw-fx.log 19803
```

Each line printed is one JSON measurement. Egress is not blocked by these
scripts.
