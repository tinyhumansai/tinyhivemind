# Adapter modules

| Path | Responsibility |
| --- | --- |
| `deploy/` | One-runtime assembly from hive packages (`HiveDeployment::from_packages`): captured permissions, approvals, workflows, executable tools and script hooks |
| `config/` | Crate-internal deployment plan and its permission validation; not an input format (`HiveConfig` is retired) |
| `host/` | Supplied handle registration, continuing sessions, host hooks and management |
| `tools/` | Stable native specifications, argument validation and bound execution |
| `memory/` | Hive-shared OpenHuman memory: the hive root, per-seat memory agent ids, seat binding, and core `Recall`/`Remember` |
| `language/` | `PackageSource` (a checked package and its directory), the package-to-plan lowering, and portable conversions for coordinator settings, seat requests and memory bindings |
| `journal/` | Optional in-memory host log used by standalone research examples |
| `offline/` | Feature gated loopback backend and host runtime configuration fixtures |
| `error.rs` | Typed adapter errors |
| `lib.rs` | Public exports and registration example |

The host supplies runtime construction ports; `language/` reads hive packages and
`config/` is the internal plan they lower into for MCP, skills, memory and prompts; `memory/` only decides which root and memory agent
id each seat is bound to. Durable scheduling and episode state belong to `tinyhivemind-hives`.

A hive package is the only deployment input. `language::PackageSource::load`
reads and checks one directory; `deploy::HiveDeployment::from_packages` lowers
one or more of them onto a single runtime and attaches what the plan does not
carry: the packages' executable tools and script hooks (run by
`tinyhivemind-exec`), staged skill bundles, and session and Jev settings.
