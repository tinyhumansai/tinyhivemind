# Implement hive language integrations

Binding specifications: [hive language integrations](../specs/hive-language-integrations.md),
[executable tools](../specs/executable-tools.md).
Decision: [ADR 0034](../adr/0034-the-hive-language-defines-the-deployment.md).

Every step ends with the four contract commands green.

## 0. OpenHuman at upstream main

Move `vendor/openhuman` and the workspace git pins (OpenHuman, tinytools,
tinymemory, tinyflows) to the commits OpenHuman main vendors, mirror its root
`[patch]` tables under `vendor/openhuman/`, and port the adapter to the moved
APIs: `HookOutput` for tool-hook decisions, `security::turn_origin`,
`agent_registry`, `tinymemory_tools::{Identity, validate_root}`,
`tinyflows_schedule::validate_schedule`, `cron::scheduler::run` with a
`DispatchCtx`, and `tinymemory_api::Error` as the memory error.

## 1. Language types

Add `document/integration/` with one file per section (`runtime`, `provider`,
`mcp`, `skill`, `tool`, `toolpack`, `permission`, `profile`, `hook`,
`settings` for `session`/`jev`/`features`, `workflow`) and the shared
`SecretRef` and `Limits`. Extend `Manifest` and `Seat` (and `SeatWire`).
Keep `tools/` and `skills/` files in `parse`. Pin each wire shape with a serde
test before writing validation; serialization writes every field so the
unknown-field pass still works.

## 2. Validation

Add `validation/integration.rs`: identities, references, documents, bounds and
the `integration` rule family (new `Error::Integration` and `Rule::Integration`).
One failing fixture per rule.

## 3. Editing

Add `PatchClass::Integration`, route the new sections, seat integration fields
and `tools/`/`skills/` files to it in `patch` and `check`, and test that a
constitution without it refuses such an edit.

## 4. Lowering

Resolve every reference: `LoweredSeat` carries the merged profile and seat
integration (model with provider, MCP servers, skills, executable tools,
hooks, toolpack groups, permissions, limits, context bodies); `LoweredHive`
carries `integrations` (runtime, session, Jev, features, workflows, and the
declared sections). Test precedence and toolpack expansion.

## 5. `tinyhivemind-exec`

New crate: `ExecTool` (a `tinytools::Tool` over a lowered tool, a package root
and an environment), the argv/stdin/output convention, environment scrubbing,
timeouts that kill the process group, and `HookRunner` for the three events.
Fixture scripts under `src/test/fixtures/`.

## 6. OpenHuman deployment from packages

Replace `config/` (parser, `HiveConfig`, directory loading) with a
`language` lowering from `&[LoweredHive]` plus package roots to the internal
deployment plan the `deploy` module already builds from. Attach exec tools and
hooks to each seat, map toolpack groups to `ToolGroups`, `session` and `jev`
to the native agent config, features to integration switches, and workflows
to cron jobs. Check cross-package agreement and permission narrowing.
Convert `examples/openhuman/hives/*` to packages.

## 7. Examples and documentation

New lang example packages (`script-tools`, `integrations`, `broken/*`), a
`run_package` adapter example, offline integration tests that run them, and
every README, spec index, wiki page and roadmap row brought in line.
