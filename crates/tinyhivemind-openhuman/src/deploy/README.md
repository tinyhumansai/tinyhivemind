# Manifest deployment

`HiveDeployment::from_packages` deploys one or more checked hive packages
(`language::PackageSource`) onto one native runtime: reusable templates, fixed
seats and configured coordinator hives, one hive per package. `BuildOptions`
carries host storage, native configuration, provider, tools, consent, approval
and effect-classifier ports. Credentials resolve during construction and are
omitted from deployment and factory diagnostics. (`build` and `build_with`,
which took the retired `HiveConfig`, are test-only.)

A seat id that several packages declare is one agent whose integration must
agree across them. Beyond the native definition, `packages.rs` attaches what a
package declares and the plan does not carry: each seat's executable tools
(`tinyhivemind_exec::ProgramTool`, added to every turn's tool belt and made
visible), its `pre_tool_use` and `post_tool_use` script hooks (a native
`ToolHook`), its skill bundles (a staged copy of `skills/` when the seat selects
a subset), and the first package's `session` and `jev` settings applied to the
runtime config. `ProgramEffects` classifies those tools for the permission gate
by their declared `side_effects` and defers every other tool to the host's
classifier. `stop` script hooks are run by `hooks::wrap_turn` on a finished reply
and only observe it: a turn is authorized by the coordinator's round, so a stop
hook cannot start another.

A toolpack's tool groups apply under the runtime's ceiling (`profile::groups`):
an agent may narrow the runtime's disclosure of a group but a mode more exposed
than the runtime's is refused, as is an unknown group id.

| File | Responsibility |
| --- | --- |
| `mod.rs` | Assembly (`from_packages`), runtime/seat/policy access, pending decisions and scheduler lifetime, and the per-turn tool belt |
| `packages.rs` | Package extras: executable tools as native tools, script hooks, skill staging, session and Jev settings, and the `ProgramEffects` classifier |
| `types.rs` | Typed errors, secret resolver, host ports and pure hive policy view |
| `runtime.rs` | Native runtime settings |
| `compatibility.rs` | Preflight checks for unsupported native representations |
| `memory_root.rs` | Injective logical root encoding for native namespaces |
| `profile.rs` | Templates, prompts, fixed memory roots, seat definitions and toolpack group modes under the runtime ceiling |
| `permission.rs` | Additive baseline, membership, child and pure approval gates |
| `rules.rs` | Argument rules and authenticated tool/context metadata |
| `hooks.rs` | Captured turn overlays, model budgets, parking, outbound policy and observed `stop` hooks |
| `factory.rs` | Typed dynamic seats using the same catalog and runtime |
| `workflow.rs` | Native agent jobs and weakly captured coordinator system jobs |
| `test/` | Offline construction, effects, approvals, jobs and cancellation fixtures |

Membership policies intersect the runtime, profile and seat baseline. The runner
binds continuing sessions before installing hooks; native cron sessions cannot
inherit an unrelated active hive's permissions. Tool rules requiring unavailable
metadata refuse the call. `EffectClassifier::subject` and `rule_context` supply
trusted application metadata; authored arguments cannot replace actor, session,
hive, episode or scheduled-job identity. Unknown effects refuse execution.
Declared `delegate_<profile>` tools coordinate internally; child calls retain
parent and child restrictions.

Native tool denials abort a turn and remain failed/interrupted coordinator
records. Hosts can explicitly release a seat with a safe note and enqueue new
completion work, retaining the failure without replaying its effect.

Ask decisions create a correlated adapter pending request and invoke the optional
native `ApprovalHandler` explicitly. The attempted call is refused and coordinator
work parks. Await the answer with `wait_decision`, then use `release` to provide a
host resumption note. Release does not replay the attempted effect or mint a
standing grant. It atomically matches the completed parked reservation; early
answers and stale approvals cannot release running or later work, even when
the later turn resumes the same input. Hosts can supply an `ApprovalContext` to share roster, epochs,
consent state and sequence ordering with the application.

Seat memory bindings are fixed across hives. They use the stable seat id under
`deployment:<deployment_id>` by default, or the profile/seat override. Each
configured hive's default shared-memory root is
`deployment:<deployment_id>/team:<hive_id>`; deployments remain isolated even
when they use one memory engine. An explicit hive root opts into sharing.
OpenHuman agent ids use `<deployment_id>-<seat_id>` unless explicitly configured. Hive
`Remember` entries use a separate store rooted per hive; coordinator turns
recall from that captured hive before invoking the native agent. `native_memory_root` encodes every logical UTF-8 root below
`project:hivemind` using 64-byte hex chunks as `team` segments; roots over
384 bytes fail before boot. This includes roots resembling native namespaces.
Hive shared
memory is accessed separately through `memory(hive_id)`. `policy(hive_id)` exposes
pure `EpisodePolicy` and `DivisionPolicy`; completion scheduling consumes its
own configured coordinator, routing and conduct settings.

Native seat cron runs an isolated session and retains baseline tool/approval and
iteration restrictions. It has no coordinator membership overlay or parking
hook, and cannot send into conductor work. Configured native seat cron timeout
or model-budget combinations fail before runtime creation because those native
paths lack the runner's enforcement ports. Hive jobs enqueue durable scheduled
messages and explicitly scope each resulting turn as cron automation. Scheduled
external effects are refused for builtin, MCP and host tools.

Registration starts no scheduler. `start_scheduler` starts the native scheduler
explicitly after assembly; deployment drop aborts it. Coordinator admission and
native scheduler admission have independent ceilings, both configured by
`runtime.concurrency`; direct agent turns bypass coordinator admission.
System handlers capture weak host state, allowing deployment drop to release the
process singleton runtime.

Preflight rejects settings the native API cannot faithfully represent: multiple
skill roots, SSE MCP, an empty MCP allowlist, transport-inapplicable MCP fields,
child instance-only settings, differing per-hive retention, and model budgets
without positive conservative per-call reservation bounds. SQLite session storage
uses native storage-backed session stores and requires a workspace directory.
