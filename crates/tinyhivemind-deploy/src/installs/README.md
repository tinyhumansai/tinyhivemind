# Installs

The host maps a harness kind to a pinned program; a package names only the
kind. OpenHuman needs `openhuman-core`; Claude Code `node` and
`claude-agent-acp`; Codex `node`, `codex-acp` and the Codex binary, whose
catalog is written per seat for its model; OpenClaw `node`, `openclaw.mjs` and
the seat's Gateway.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Installs`, `Pinned`, `CodexInstall`; the check and the harness per seat |
| `test.rs` | Each kind's missing installs, and a Codex catalog that cannot be written |
