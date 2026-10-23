# Checked-in OpenHuman hives

Every hive here is a [hive language package](../../../docs/specs/hive-language.md):
`hive.json`, `constitution.json` and `memory.json`, one `roles/<role>.md`
charter per role, one `seats/<seat>.md` prompt per seat, and `context/*.md`
documents. The adapter deploys them with `HiveDeployment::from_packages`
([integration spec](../../../docs/specs/hive-language-integrations.md)).

| Directory | Hive and seats |
| --- | --- |
| `basic_hive/` | Hive `release`: read-only Alice and Bob; ephemeral workspace and continuing host sessions |
| `deepswe_hive/` | Hive `deepswe`: lead, implementer, tester and reviewer; the `deepswe` workspace server plus one hive tool server per seat, 16 iterations, temperature zero, width four |
| `pe1006_hive/` | Hive `pe1006`: five mathematical/research seats; host CortexDB engine, sealed/research context and original workspace templates |
| `multi_hive/` | Two packages, hives `a` and `b`, deployed together: four persistent seats, one shared with narrower review authority in `b` |

Each `hive.json` is canonical: it is the complete serialization of its decoded
manifest, and `tests/manifests.rs` fails if it drifts. Check a package after
editing it, from the repository root:

```sh
cargo run -q -p tinyhivemind-lang --example hive_check -- examples/openhuman/hives/basic_hive
```

A binary reads its package with `PackageSource::read`, edits the decoded
package, and deploys it; nothing it changes is written back. Run-only values —
the deployment id, the workspace, every MCP server's executable path and
arguments, the model name, and the memory engine when CortexDB is unavailable —
are set that way. Each seat has its own `tinyhive-<seat>` server because its
arguments (`--agent <seat>`, its outbox) differ per seat, and a package
declares a server once.

No package carries a literal credential. `multi_hive` names its provider key as
`{"env": "OPENROUTER_API_KEY"}`; offline runs drop the provider and supply a
trusted loopback route through `BuildOptions`, so no operator credential is
needed.

The Euler binary resolves its configured native CortexDB engine and injects it
through `BuildOptions.memory_engine`. If native memory is off because its
credential is unavailable, it sets the package's engine to `disabled` and
continues with durable Markdown memory. The deterministic lowering test
substitutes the in-process reference engine and never inspects operator
credentials.

The DeepSWE and Euler hosts retain their pure completion drivers. The declared
routing policy feeds those drivers; the deployment policy view supplies the
DeepSWE episode width. Coordinator membership and roles come from the same
packages. Direct native turns are admitted by the pure driver rather than the
coordinator's separate global concurrency gate.

See the [example guide](../README.md) for commands and the native denial/safe
recovery behavior demonstrated by `multi_hive`.
