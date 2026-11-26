# tinyhivemind-host

The part of running a hive's seats that does not depend on the agent harness:
the hive tools and the belt that executes them, the host's send and management
policies, turn scope and prompt, package loading and the deployment plan, and
supervised harness processes.

It links no harness and names no harness type
([ADR 0035](../../docs/adr/0035-a-harness-is-a-process.md));
`.github/scripts/assert-pure.sh` asserts the first. It opens no store and keeps
no transcript: the coordinator in [`tinyhivemind-hives`](../tinyhivemind-hives/README.md)
owns both. It spawns processes, so it is not one of the pure crates.

An adapter serves `tools::Kind` to a model, turns each call into a `ToolCall`
bound to a seat, and hands it to a `Belt`. The actor and session are always the
host's. [`tinyhivemind-openhuman`](../tinyhivemind-openhuman/README.md) is the
linked adapter; it re-exports this crate's types under its own paths.

The repository [README](../../README.md) says how this fits the rest, and
[the spec](../../docs/specs/harnesses.md) where it is going.

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate's modules |
