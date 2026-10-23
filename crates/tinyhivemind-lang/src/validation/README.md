# validation

Checks references, paths, schema version, bounded policies and constitution
memory restrictions before activation. Desk graphs may contain cycles; goal
decompositions may not.

Every rule pushes its finding with the file and JSON Pointer it concerns.
`validate` returns the first as an `Error`; `diagnose` returns every one as a
`Diagnostic`. Rules run in one fixed order, so the first finding is the error
fail-fast validation stopped at. A seat's role is reported at
`hive.json:/seats/<i>/role`, a missing prompt at the manifest field naming it,
frontmatter and context markers on the Markdown file, memory findings in
`memory.json`, and protected namespaces and read-only pins in
`constitution.json`.

| File | Purpose |
| --- | --- |
| `mod.rs` | `validate`, `diagnose`, `validate_path`, `validate_namespace`, and the manifest rules |
| `bounds.rs` | Policy and coordinator bounds against the constitution's caps |
| `memory.rs` | Memory identities, bindings and reaches |
| `integration.rs` | The integration sections: identities, references, shipped skill and tool files, bounds, and settings OpenHuman cannot run as written |
| `test/` | Fail-fast regressions and finding locations; see [`test/README.md`](test/README.md) |

## Integration rules

`integration.rs` reports, per the
[integrations specification](../../../../docs/specs/hive-language-integrations.md#validation):
blank or duplicate ids in every section (`identity`); references from seats,
profiles, toolpacks, hooks, workflows and the runtime to undeclared providers,
models, MCP servers, skills, tools, toolpacks, permission sets, profiles,
hooks, seats and context documents (`reference`); a skill without
`skills/<id>/SKILL.md` and any `tools/…` command element that is not a shipped
file (`missing_document`); unsafe paths (`path`); zero or non-finite bounds
and a cron expression without five fields (`bound`); and shapes the harness
cannot run (`integration`): `sqlite` sessions with no workspace, a tool in both
`allow_tools` and `deny_tools`, a delegated profile that sets what only an
agent can, a `matcher` on a `stop` hook, an `argv` placeholder naming no
argument, and a feature name outside `[a-z0-9_]+`.
