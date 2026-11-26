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
| `harness.rs` | The `harnesses` section and `harness` fields: identities, references, isolation, and whether the capability table judges the package |
| `capability/` | The capability table judged per seat, default-deny; see [`capability/README.md`](capability/README.md) |
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

## Harness rules

`harness.rs` reports a blank or duplicate harness id (`identity`), a
`harness` on the runtime, a profile or a seat naming no declared harness
(`reference`), and `isolation: deployment` on a kind other than `openclaw`
(`integration`); `integration.rs` reports a harness on a delegated profile.
Once a package declares or names a harness, `capability.rs` judges every seat
against its kind's column of the capability table and reports each rejected
setting once, at the field that sets it. It is default-deny: the runtime, the
seat, its profile, model, permission sets, toolpacks and hooks, and `session`
and `jev`, are walked field by field, and any field set away from its default
asks for the capability `harness/fields.rs` gives it. By value it also
judges the resolved provider's `kind`, `endpoint` or `credential` (or the
`provider` field that should name one), a sandbox on the runtime or an
applying permission set, `tool_rules`, a toolpack group turned on, a
`sqlite` or host session store, an in-process or host memory engine, a
`stop` hook, and `limits.iterations` at the layer that set it (rejected but
on OpenHuman, and there above its cap). Seats
sharing one OpenClaw instance must resolve to one provider (reported at the
harness's `isolation`), and a tool a judged seat receives may not declare an
argument named `turn` or `call`. A package naming no harness is not judged:
the linked adapter deploys it under its own limits.
