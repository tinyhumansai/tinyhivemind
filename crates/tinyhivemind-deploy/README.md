# tinyhivemind-deploy

Starts a hive's seats on their harness processes under one coordinator: one
hive tool server for every seat, the `OpenClaw` Gateways its `OpenClaw` seats
need, and each seat on the harness its package declares, OpenHuman, Claude
Code, Codex or OpenClaw. It is step 5 of
[the process harness plan](../../docs/plans/2026-11-25-process-harnesses.md),
and links no harness ([ADR 0035](../../docs/adr/0035-a-harness-is-a-process.md)).

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate |
| [`tests/`](tests/README.md) | Live hives on the pinned agents |
