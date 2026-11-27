# Installs

The host maps a harness kind to a pinned program; a package names only the
kind. OpenHuman needs `openhuman-core`; Claude Code `node` and
`claude-agent-acp`; Codex `node`, `codex-acp` and the Codex binary, whose
catalog is written per seat for its model; OpenClaw `node`, `openclaw.mjs` and
the seat's Gateway.

The check also runs the capability check of the seat's harness client, the
one it would make when the seat starts, so a seat its harness refuses as
declared (no model, or a provider of the wrong kind) is refused before
anything starts.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Installs`, `Pinned`, `CodexInstall`; the check and the harness per seat |
| `test.rs` | Each kind's missing installs, a seat its harness refuses as declared, and a Codex catalog that cannot be written |
