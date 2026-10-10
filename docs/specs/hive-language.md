# Hive definition language

Status: Accepted for the initial host-neutral format. Implements issue #112.
Implementation: [plan](../plans/2026-10-10-hive-language.md).
Boundary: [ADR 0032](../adr/0032-hive-language-owns-the-wire-format.md).

## Purpose and ownership

A hive package describes one configuration as data. The host supplies an
in-memory map of relative paths to strings; this library opens no files,
connects no clients, creates no agents, and persists nothing. The host owns
credentials, evaluation, activation, memory contents, and lineage storage.
Canonical manifests use JSON. Markdown carries prompts and role charters.
The initial frontmatter accepts JSON (a YAML-compatible subset); arbitrary
YAML, TOML and sampled families are outside this version.

## Package and wire contract

`hive.json` has `$schema`, `version`, identity, roles, seats, desks, goals,
missions, topology/referrals, context references, and policies. All references
resolve within the package. `constitution.json` pins hard constraints, turn
and width budgets, permitted edit classes, read-only memory identities,
protected namespace prefixes, contamination canaries, and evaluation,
acceptance and telemetry settings. It is supplied by the host and immutable
to every editor operation. Unknown fields and unsupported schema/version
values fail rather than disappearing during a round trip. The `$schema` value
`https://tinyhumans.ai/schemas/hive/v1` identifies this wire version; parsing
does not fetch that URI.

`seats/<id>.md` contains structured frontmatter (role, template, nonsecret
config, model, tools, MCP and skills) and a prompt body. Roles have independent
charters in `roles/<id>.md`. Shared `context/*.md` documents use stable item
identities, so deltas add, replace or remove one item instead of rewriting
all accumulated context. Seat/role metadata is canonical in the manifest;
frontmatter, when supplied, must agree with it. Declared seat prompts, role
charters and context references must use their respective package directories,
so document purpose cannot bypass edit-class authority.

Policies reuse core policy structs. Division and conduct gain serde. The
coordinator's options gain serde in the coordinator crate; the pure language
owns a host-neutral options representation that converts in the adapter.
No dependency from the language to hives or OpenHuman is permitted.

## Validation and lowering

Reject duplicate/empty identities, unresolved roles, seats, desks, goals,
missions, context and memory references, unsafe relative paths, malformed
namespaces, and invalid policy bounds. Topology is a graph and may contain
cycles; goal decomposition is acyclic. Widths and budgets must be nonzero.
The language requires `round_width <= revealed_width`, as requested in #112.
This is stricter than core, whose default is blind width 4 and revealed width
1: packages must explicitly choose compatible widths. Core defaults are
unchanged. Constitution caps apply regardless of edit class.

Lowering returns existing `Desk`, role-populated `RouteCandidate`, and core
policies, plus host-neutral seat creation/configuration, coordinator settings
and memory binding data. Goals, missions, graph links and declared shared
context are retained for the host to install alongside those runtime values.
Adapter helpers convert these to `CoordinatorOptions`,
`ManagementRequest`, and the existing OpenHuman `MemoryBinding`. Applying a
candidate is a host decision between turns or sessions; an in-flight turn
continues using its captured snapshot. No automatic live mutation occurs.

## Memory

`memory.json` declares identities with namespace, agent/pool kind, run/hive
lifecycle, read-only status and optional fork parent. Seats bind identities,
read through bounded reaches, choose recall moments, character budgets, and
remembered entry kinds. Contents never enter the package. The language mirrors
tinymemory namespace syntax without depending on its engine. A fork declares
a child namespace; the host must implement copy-on-write and pinned reads.
Sharing and reassignment change bindings without changing seat identities.

No binding or reach may access protected evaluation/orchestrator namespaces,
widen to root, or mutate identities pinned read-only by the constitution.
`inherit: true` reaches tinymemory’s global root, so this version rejects it.
Declare individual ancestor reads with `inherit: false` inside the package
root to share curated memory without broadening the namespace boundary.
Memory learning deltas are separate host operations with stable entry IDs;
canary-containing content is rejected. Hive-lifecycle lineage records pin
identity IDs and host event watermarks. Cross-run storage is an opt-in host
extension, superseding the memory spec's run-only non-goal for these packages;
the existing HiveMemory default remains run-scoped.

## Edits and archive

Patch classes are `prompt`, `context`, `memory`, `roster`, `topology`, `policy`,
and `goal`. Typed manifest operations and stable Markdown item deltas carry
preconditions, so stale proposals fail atomically. JSON Pointer operations
must remain within their class. Guarding checks the complete candidate, not
just operation names. Constitution, evaluation, acceptance and telemetry are
outside editor write authority. The host must guard every application.

A version record contains a cryptographic content identity, parent, patch,
class, rationale, transcript sequence references, finite objective scores,
verdict, and persistent memory watermarks. Both accepted and rejected designs
remain available. Pure folds select accepted heads and a nondominated Pareto
frontier; a rejected candidate never becomes the active head. The example
executes propose, guard, evaluate and record without a network or store.

## Acceptance evidence

Deterministic tests pin wire shapes, package parse/lower, diff/apply,
reference failures, every guard/error variant, memory sharing/reassignment/
forking, read-only/protected/root reaches and contamination. Fixture packages
represent the lead/implementer/tester/reviewer DeepSWE room and PE1006 room.
The self-edit example records an evaluated candidate with its lineage.
