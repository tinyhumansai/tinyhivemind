# pe1006_hive package

The `pe1006` hive: `theory`, `solver`, `checker`, `lead` and `researcher`, each
allowed file, shell and MCP tools with `run_code` and `ask_docs` denied, 12
iterations, temperature zero, and its own `tinyhive-<seat>` hive tool server
with shared memory verbs. The researcher composes `context/research.md`; every
other seat composes `context/sealed.md`. The runtime asks the host for its
memory engine. See the [package index](../README.md).

| File | Purpose |
| --- | --- |
| `hive.json` | Seats, desk, policies, runtime, model and MCP servers, in canonical form |
| `constitution.json` | Default caps: 60 turns, width 8 |
| `memory.json` | Memory root; no identities or bindings |
| `roles/` | One charter per role |
| `seats/` | One prompt per seat |
| `context/` | Seat context and the binary's workspace templates; see its README |
