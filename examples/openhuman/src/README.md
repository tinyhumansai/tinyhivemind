# OpenHuman example source

| File | Purpose |
| --- | --- |
| `lib.rs` | Package reading and deployment, and exact experiment MCP effect classification |
| `test.rs` | Exact MCP effect classification of per-seat hive servers, failing closed on unknown servers and verbs |
| `main.rs` | Supplied-agent and dynamic-registration proof |
| `proof/` | Offline topology, catalogue and management assertions |
| `bin/` | Package-backed basic, multi-hive, DeepSWE, PE1006 and Euler executables |

The standalone crate keeps its own lockfile and target directory. See the
[parent guide](../README.md) for commands and host constraints.
