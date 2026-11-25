# Adapter modules

| Path | Responsibility |
| --- | --- |
| `deploy/` | One-runtime assembly from hive packages (`HiveDeployment::from_packages`): captured permissions, approvals, workflows, executable tools and script hooks |
| `config/` | Crate-internal: the host's deployment plan, validated under OpenHuman's schedule parser and built-in ids |
| `host/` | Supplied handle registration, continuing sessions, host hooks and management |
| `tools/` | The host's hive tools as native tools, with registration and activation checks |
| `memory/` | Hive-shared OpenHuman memory: the hive root, per-seat memory agent ids, seat binding, and core `Recall`/`Remember` |
| `language/` | Re-exports of the host's `PackageSource` and conversions, and OpenHuman memory bindings |
| `offline/` | Feature gated loopback backend and host runtime configuration fixtures |
| `error/` | Typed adapter errors and the host error conversions |
| `lib.rs` | Public exports, including the host's `journal`, and registration example |

The harness-neutral half of this adapter is
[`tinyhivemind-host`](../../tinyhivemind-host/README.md): tool vocabulary and
dispatch, policy requests, turn scope and prompt, packages and the plan they
lower into for MCP, skills, memory and prompts. `memory/` only decides which root and memory agent
id each seat is bound to. Durable scheduling and episode state belong to `tinyhivemind-hives`.

A hive package is the only deployment input. `language::PackageSource::load`
reads and checks one directory; `deploy::HiveDeployment::from_packages` lowers
one or more of them onto a single runtime and attaches what the plan does not
carry: the packages' executable tools and script hooks (run by
`tinyhivemind-exec`), staged skill bundles, and session and Jev settings.
