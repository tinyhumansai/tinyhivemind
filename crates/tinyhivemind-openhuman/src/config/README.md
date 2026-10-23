# Deployment plan

The crate-internal plan `deploy` builds one `OpenHuman` runtime from. It is
**not an input format**: the public `HiveConfig` JSON and Markdown manifests,
`load_dir` and the YAML profile frontmatter are retired
([ADR 0034](../../../../docs/adr/0034-the-hive-language-defines-the-deployment.md)).
A hive is defined once, as a `tinyhivemind-lang` package; `language::plan`
lowers the checked packages into these types, so there is one schema to edit,
check and version. Nothing here opens a file or resolves a credential.

`validate` checks what a single package cannot, because it needs the whole plan:
references and identities, limits, permission narrowing and MCP consistency.
`ValidatedHiveConfig` exposes immutable data through `config()`; consuming it
with `into_config()` returns editable data without retaining validation proof.
Parse failures are `ConfigError` values, exported from the crate root, and
none carries input contents.

| File | Responsibility |
| --- | --- |
| `mod.rs` | Module doc and internal exports |
| `types.rs` | The plan's serde types and the immutable validated wrapper |
| `parse.rs` | Inline-secret refusal over a plan value; the JSON entry points exist only for tests |
| `validate.rs` | References, identities, limits, narrowing and MCP consistency |
| `test/` | Wire and validation regression tests; see [`test/README.md`](test/README.md) |

Named `SecretRef` values (the language's own type) encode as `{"env":"NAME"}`
or `{"store":"KEY"}`. MCP environment and header values require references.
Seat overrides have no provider credential field. Runtime-only ports and
resolved secrets never enter the plan.

Permission layers compose as an intersection; omitted optional allowlists and
approval/rule fields inherit. Explicit allowlists are subsets. Denylists must
retain parent denials. Conservative rule equivalence requires identical explicit
argument-sensitive rules and approval policies, except explicit disabling of
approval authority. Deployment must enforce every layer, including inherited
ones. Runtime, profile and seat turn limits can only decrease.

`normalized_mcp` combines runtime/profile/seat declarations, sorts tool names,
and rejects conflicting named connections. Membership MCP declarations assert
the complete same set; use two seats to change connections.

Adapter hive defaults use episode `round_width=1`, `revealed_width=1`. Each
width is independently bounded by runtime concurrency; blind rounds may be
wider than revealed rounds. Core defaults remain unchanged. Other policies
retain core and coordinator defaults.
