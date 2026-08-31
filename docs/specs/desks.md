# Desks: declared groups with host-owned overlays

- **Status:** Implemented
- **Owner:** Maintainers
- **Plan:** [`../plans/desks.md`](../plans/desks.md)

## Problem

A host needs one deterministic view of a desk after combining its declared
blueprint with operator-owned additions, retirements, and member ordering. If
each caller performs that merge itself, a mention can resolve a different
group from the one rendered in the UI, and a partial order can silently drop
or invent a participant.

The host owns all storage. `tinyhivemind-core` receives borrowed snapshots and
performs only validation and projection over those arguments.

## Goals

- Define string-ID data-transfer types compatible with host records.
- Merge declared desks before added desks, and founding members before member
  additions.
- Remove retired agents and deduplicate members while preserving first
  appearance.
- Apply an explicit order only when it is a stable permutation of the complete
  final member set.
- Resolve an exact desk id or exact display name without weakening the
  case-sensitive identity of named desks.
- Reject malformed overlays with typed, actionable errors.

## Non-goals

- Reading or writing host storage, validating an agent against a live roster,
  or retaining a merged desk snapshot.
- Unicode case folding or case-insensitive lookup for named desks.
- Mention parsing and roster/person types; those land in P3.
- A second representation of the General conversation identity established in
  P1.

## Public data and wire format

The three host-facing DTOs use owned strings and serde's stable snake-case
field names:

```rust
pub struct Desk {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<String>,
    pub responder_mode: ResponderMode,
}

pub struct DeskMember {
    pub desk_id: String,
    pub agent_id: String,
}

pub struct DeskOrder {
    pub desk_id: String,
    pub ordered: Vec<String>,
}
```

Their JSON object keys are `id`, `name`, `description`, `members`,
`responder_mode`, `desk_id`, `agent_id`, and `ordered` as applicable. P6 added
`ResponderMode::{Lead, Auto}`: `Lead` is the default and its field is skipped,
preserving P2's exact four-field desk wire form. `Auto` serializes as
`"responder_mode": "auto"`. All original P2 fields remain required.

## Borrowed desk set

`DeskSet<'a>` borrows five host snapshots: declared desks, added desks, member
additions, desk orders, and retired agent ids. Its fields are private. Its
constructor stores those views without doing I/O or allocating a persistent
merged model.

The public operations are:

- `resolve_id(identity)` returns the canonical borrowed desk id for an exact
  id or exact name. An exact id takes precedence over name aliases. No match is
  an unknown-desk error; multiple exact name matches are an ambiguous-resolution
  error. Duplicate exact ids are rejected by `validate` as `DuplicateDeskId`
  before identity resolution is used on a valid set.
- `contains(identity)` reports whether `resolve_id` succeeds. It does not
  replace `validate`; malformed input can therefore make an otherwise present
  identity report false.
- `members(identity)` returns the validated final member ids in order.
- `lead(identity)` returns the first final member, or `None` for an empty desk.
- `validate()` checks every desk and overlay, returning the first error in
  input order.

All fallible operations return the crate-wide `Result<T>` alias.

## Merge and validation semantics

Desks are considered in declared order and then addition order. Desk ids and
names must both be non-empty. Desk ids must be unique by exact, case-sensitive
comparison. Names need not be unique because ambiguity is reported when a name
is resolved.

The canonical default desk is the desk whose id and name are both exactly
`GENERAL_DESK` (`"General"`). Every other desk is non-default. A non-default
desk is rejected if either its id or name equals `GENERAL_DESK` or
`MAIN_THREAD_ID` under ASCII-case-insensitive comparison. This prevents an
apparently named desk from entering the four-spelling General equivalence while
leaving all other desk ids case-sensitive.

For a resolved desk, founding `Desk::members` are considered first, followed
by matching `DeskMember` rows in input order. Duplicate agent ids are retained
only at their first appearance. Retired agent ids are then removed by exact,
case-sensitive comparison.

Every `DeskMember::desk_id` and `DeskOrder::desk_id` must target an existing
desk id exactly; display names are not accepted in overlay records. At most one
order may target a desk. Each ordered agent id must occur in that desk's final
member set exactly once. A repeated id is a duplicate-order-member error, an id
outside the set is an unknown-order-member error, and omitting any final member
is an incomplete-order error. Consequently an accepted order is a whole-set
permutation: it may reorder members but cannot partially reorder, add, or drop
them.

## Errors

The crate-wide `Error` enum has specific variants for:

- empty desk id and empty desk name;
- duplicate desk id;
- a reserved General identity;
- ambiguous desk resolution;
- unknown desk resolution and unknown member/order desk targets;
- duplicate orders for one desk;
- duplicate and unknown members in an order;
- an incomplete order.

Error messages are lowercase and have no trailing punctuation. Variants carry
the offending ids needed by a host to report the problem without parsing text.

## Example

Given declared desk `engineering` with members `["alice", "bob"]`, additions
`bob` then `cara`, retirement `alice`, and order `["cara", "bob"]`, the final
members are `["cara", "bob"]` and the lead is `cara`. Order `["cara"]` is
rejected as incomplete; order `["cara", "cara"]` is rejected as duplicate;
order `["cara", "dana"]` is rejected as containing an unknown member.

## Invariants and constraints

- The algebra is pure, synchronous, deterministic, and free of host types.
- `DeskSet` owns none of its inputs and never opens a database, file, or socket.
- Named ids remain case-sensitive; exact names are aliases only for lookup.
- Retirements never mutate the borrowed DTOs.
- Serde and `thiserror` are the only direct third-party dependencies restored
  for P2.

## Acceptance criteria

- Unit tests cover every merge step and every error variant, including wire
  representation tests for all DTOs.
- Integration tests exercise the public namespaced API and crate-wide error
  surface.
- Every changed source file has at least 90% line coverage.
- Formatting, Clippy, the core test suite, and the purity guard pass.

## Open questions

None.
