# Hive definition language

Status: Accepted for the initial host-neutral format. Implements issue #112.
Implementation: [plan](../plans/2026-10-10-hive-language.md).
Boundary: [ADR 0032](../adr/0032-hive-language-owns-the-wire-format.md).
Editing: [checking direct edits](hive-language-checking.md), [ADR 0033](../adr/0033-editors-write-files-and-the-crate-checks-them.md).
Integrations: [what the agents run with](hive-language-integrations.md) and
[executable tools](executable-tools.md), [ADR 0034](../adr/0034-the-hive-language-defines-the-deployment.md).

## Purpose and ownership

A hive package describes one configuration as data. The host supplies an
in-memory map of relative paths to strings; this library opens no files,
connects no clients, creates no agents, and persists nothing. The host owns
credentials, evaluation, activation, memory contents, and lineage storage.
Canonical manifests use JSON. Markdown carries prompts and role charters.
Frontmatter is optional. When present it is a JSON object between an opening
`---\n` (the document's first four bytes) and the first `\n---\n`. A seat
prompt or role charter that opens with `---\n` and never closes it, or whose
frontmatter is not a JSON object, is rejected. YAML, TOML and other
frontmatter syntaxes are outside this version.

## Package and wire contract

`hive.json` has `$schema`, `version`, identity, roles, seats, desks, goals,
missions, topology/referrals, context references, and policies. All references
resolve within the package. `constitution.json` pins hard constraints, turn
and width budgets, permitted edit classes, read-only memory identities,
protected namespace prefixes, contamination canaries, and evaluation,
acceptance and telemetry settings. It is supplied by the host and immutable
to every editor operation. Its `hard_constraints`, `evaluation`, `acceptance`
and `telemetry` are opaque to this crate: it carries them, guards them against
edits, and leaves their meaning and enforcement to the host. Unknown fields
and unsupported schema/version values fail rather than disappearing during a
round trip. The `$schema` value `https://tinyhumans.ai/schemas/hive/v1`
identifies this wire version; parsing does not fetch that URI.

### Optional fields and defaults

Only what has no safe default is required. Omitting any other field is the
same as writing its default, and serializing a package always writes every
field, so a stored package is complete and its content identity does not
depend on how tersely it was written.

| File | Required | Omitted field → default |
| --- | --- | --- |
| `hive.json` | `$schema`, `version`, `id`, `name`, `seats` | `roles`, `desks`, `goals`, `missions`, `topology`, `referrals`, `context` → `[]`; `policies` → `Policies::default()`; `coordinator` → `CoordinatorSettings::default()` |
| seat | `id`, `role`, `prompt`, `template` | `label` → the seat's `id`; `config` → `{}`; `model` → `null`; `tools` → `{"allow": [], "deny": []}` (either list may be omitted); `mcp`, `skills` → `[]` |
| desk | `id`, `name`, `members` | `description` → `null`; `responder_mode` → `lead` |
| role, mission, link | every field | none |
| goal | `id`, `description` | `acceptance`, `subgoals` → `[]` |
| `policies` | nothing | each of the eleven blocks, and each field inside a block, takes its `Policies::default()` value |
| `coordinator` | nothing | `round_width` → `1`; `conduct_policy` → `{"child_turn_wall": 6, "turn_wall": 60}`; `broadcast_budget` → `null`; each `retention` bound → `null` |
| `constitution.json` | nothing | `max_turns` → `60`; `max_width` → `8`; the five lists → `[]`; `evaluation`, `acceptance`, `telemetry` → `null` |
| `memory.json` | `root` | `identities`, `bindings` → `[]` |
| memory identity | `id`, `namespace`, `kind`, `lifecycle` | `read_only` → `false`; `fork_parent` → `null` |
| seat binding | `seat`, `identity`, `budget_chars` | `reads`, `recall_at`, `remember` → `[]` |
| read reach | `at` | `inherit`, `descendants` → `false` |

`policies` is an overlay: a JSON object merges key by key, recursively, into
`Policies::default()`, and an array, scalar or `null` replaces the default
value outright. So `{"episode": {"turn_budget": 20}}` changes one field and
keeps the other episode settings, and `"rules": [...]` replaces the approval
rules rather than appending to them. `Policies::default()` is the sequential
configuration: episode `turn_budget` 12, `round_width` and `revealed_width` 1,
routing thresholds 0 with `round_width` 1 and `choice_option_limit` 8,
approval disabled with a deny default and no approver, aside, referral and
exchange disabled, division `round_width` 4, and conduct walls 6 and 60; the
remaining blocks take their core defaults.

`memory.json`'s `root` stays required because a shared default namespace
would let two hives' memories collide in one engine. Core policy values and
core desks keep their complete wire form when decoded on their own; only
their place in a manifest is lenient. A defaulted value is validated exactly
like a written one: the defaults fit the default constitution, but a
constitution with `max_turns` below 60 or `max_width` below 4 rejects the
default conduct walls or division width until the manifest sets them.

A minimal package that parses, validates and lowers:

```json
{
  "$schema": "https://tinyhumans.ai/schemas/hive/v1",
  "version": 1,
  "id": "mini",
  "name": "Mini",
  "roles": [{ "id": "solver", "charter": "roles/solver.md" }],
  "seats": [
    { "id": "solver", "role": "solver", "prompt": "seats/solver.md", "template": "default" }
  ],
  "desks": [{ "id": "room", "name": "Room", "members": ["solver"] }]
}
```

with `constitution.json` of `{}`, `memory.json` of `{"root": "team:mini"}`,
and the two Markdown files `roles/solver.md` and `seats/solver.md`.

`seats/<id>.md` contains a prompt body, optionally preceded by frontmatter.
Roles have independent charters in `roles/<id>.md`. Shared `context/*.md`
documents use stable `<!-- item:id -->` identities, so deltas add, replace or
remove one item instead of rewriting all accumulated context. Seat and role
metadata is canonical in the manifest; frontmatter may restate it but must
agree with it. Every key in a seat's frontmatter must be a field of that
manifest seat (`id`, `label`, `role`, `prompt`, `template`, `config`, `model`,
`tools`, `mcp`, `skills`) or one of its memory binding settings (`reads`,
`recall_at`, `budget_chars`, `remember`, and `memory` for the bound identity),
with an equal value. A role's frontmatter may hold only `id` and `charter`.
Declared seat prompts, role charters and context references must use their
respective package directories, so document purpose cannot bypass edit-class
authority.

Policies reuse core policy structs. Division and conduct gain serde. The
coordinator's options gain serde in the coordinator crate; the pure language
owns a host-neutral options representation that converts in the adapter.
No dependency from the language to hives or OpenHuman is permitted.

### Parsing a document map

`parse` takes a `BTreeMap<String, String>` of relative path to contents:

- Every key must be a safe relative path (no leading `/`, `\`, control
  character, empty, `.` or `..` segment, or `:`), including keys that are then
  dropped.
- `hive.json`, `constitution.json` and `memory.json` are required and decoded
  into their typed sections.
- Every other key ending in `.md` (any case) is kept in `documents`; every
  other file, such as `notes.txt`, is dropped.
- A kept Markdown document no manifest entry references (a `README.md`, say)
  is accepted and carried along. It is part of the package, so it changes the
  content identity. A patch cannot create, remove or edit it unless it sits
  under `seats/`, `roles/` or `context/`.
- Beyond serde's own `deny_unknown_fields`, a second pass compares the
  decoded value with its input and rejects unknown keys nested inside reused
  core policy values. The one exception is `"responder_mode": "lead"` on a
  desk: core omits the lead mode when serializing, so an explicit `lead` is
  accepted rather than reported as unknown. Omitted fields pass this check,
  since it only looks for input keys missing from the output.
- A JSON array where the language expects an object is rejected at its
  pointer. serde would otherwise read a struct from an array of its fields in
  order, and with defaults an empty array (`"tools": []`, or a
  `constitution.json` of `[]`) would quietly decode as the defaults.

The parsed package is then validated.

### The whole-package form

`Package` also serializes as one JSON object with four fields: `manifest`,
`constitution`, `memory`, and `documents` (path to contents). A host may store
or ship a package this way. `crates/tinyhivemind-openhuman/examples/live_language.rs`
reads one (the `accepted-package.json` that `live_self_edit` writes) with
`serde_json` directly. That path skips `parse`'s Markdown filter and nested
unknown-field pass, so the host must call `validate` (or `lower`, which
validates first) before it uses the package; validation re-checks every
document path.

## Validation and lowering

Reject duplicate/empty identities, unresolved roles, seats, desks, goals,
missions, context and memory references, unsafe relative paths, malformed
namespaces, and invalid policy bounds. Topology is a graph and may contain
cycles; goal decomposition is acyclic. Widths and budgets must be nonzero.
The language requires `round_width <= revealed_width`, as requested in #112.
This is stricter than core, whose default is blind width 4 and revealed width
1, so the language's own default episode uses width 1 for both; a package that
widens the blind round must also widen `revealed_width`. Core defaults are
unchanged. Constitution caps apply regardless of edit class.

Routing confidences and thresholds (`minimum_confidence`,
`high_impact_minimum_confidence`, `clarification_threshold`,
`high_impact_threshold`) are integer parts per million: `1000000` is
certainty and `250000` is one in four. A value above `1000000` fails to
decode.

Lowering returns existing `Desk`, role-populated `RouteCandidate`, and core
policies, plus host-neutral seat creation/configuration, coordinator settings
and memory binding data. Goals, missions, graph links and declared shared
context are retained for the host to install alongside those runtime values.
Seat prompts and the role charters used as candidate descriptions are handed
over without their frontmatter.

Each seat with a memory binding lowers to a `LoweredMemoryBinding` derived
from its identity's namespace. `agent_id` is the text after the last `:`
and `root` is the text before the last `/`, falling back to the spec's `root`
when there is no `/`. So `team:hive/agent:critic` yields agent `critic` under
root `team:hive`. The OpenHuman adapter accepts only agent identities whose
namespace is exactly `<root>/agent:<agent_id>`.
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

Editors edit a package's files directly; [checking direct
edits](hive-language-checking.md) specifies how `check` validates them and
derives the patches below, which are the recorded form of an edit
([ADR 0033](../adr/0033-editors-write-files-and-the-crate-checks-them.md)).

Patch classes are `prompt`, `context`, `memory`, `roster`, `topology`, `policy`,
and `goal`. Typed manifest operations and stable Markdown item deltas carry
preconditions, so stale proposals fail atomically. JSON Pointer operations
must remain within their class. Guarding checks the complete candidate, not
just operation names. Constitution, evaluation, acceptance and telemetry are
outside editor write authority. So is any `evaluation`, `telemetry`, `logging`
or `constitution` key in the manifest or in frontmatter, and any `acceptance`
key outside a goal. The host must guard every application.

`diff` computes the patch between two packages for one class at coarse
granularity. For `manifest` and `memory` it emits one JSON Pointer operation
per differing *top-level* field (`/manifest/seats`, `/memory/bindings`),
carrying the whole before and after values rather than a minimal path. A
created or removed document becomes one document operation. A changed
context document becomes one operation per changed item. Any other changed
document becomes one prompt-body operation, and a changed frontmatter is
rejected as immutable. The result is guarded before it is returned, so a diff
spanning two classes fails rather than producing a patch the class cannot
apply.

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
