# tinyhivemind-lang

Portable hive definitions and guarded edits for a host that owns agents,
transcripts, evaluation and memory. The crate accepts an in-memory document
map and performs no file, socket or store access. See the
[repository overview](../../README.md) and
[language specification](../../docs/specs/hive-language.md).

A `hive.json` spells out only what has no safe default: `$schema`,
`version`, `id`, `name` and `seats`, and per seat its `id`, `role`, `prompt`
and `template`. Every other section, seat field, policy block and policy
field takes a documented default; see
[optional fields and defaults](../../docs/specs/hive-language.md#optional-fields-and-defaults).

A package also says what its agents run with: `runtime`, `providers`, `models`,
`mcp`, `skills`, `tools`, `toolpacks`, `permissions`, `profiles`, `hooks`,
`session`, `jev`, `features` and `workflows`, and may ship executable tool and
hook sources under `tools/` and skill bundles under `skills/`. Each setting
lowers to something OpenHuman can run or fails `check`; secrets are only ever
references (`{"env": "NAME"}`). See the
[integrations specification](../../docs/specs/hive-language-integrations.md) and
[executable tools](../../docs/specs/executable-tools.md).

Call `parse` for canonical JSON plus Markdown side documents (and the files under `tools/` and `skills/`), `validate` before
activation, and `lower` to obtain existing core desks and routing candidates
with declared roles and, per seat, its resolved integration (runtime defaults,
then profile, then seat). Lowered seat configuration preserves memory constraints;
the host must enforce those when registering a runner.

Editors change a hive by editing its files. Call `check` with the original
and edited files: like a compiler, it reports every violation of the language
and the immutable constitution as a located diagnostic, or returns the edit as
one patch per edit class for lineage. Eight classes exist; `integration`
owns the sections above, the integration fields of a seat and the files under
`tools/` and `skills/`, so a constitution decides whether a hive may change
what it runs with. The `hive_check` example runs the same
check from the command line. See the
[checking specification](../../docs/specs/hive-language-checking.md).

The immutable constitution pins evaluator settings and hard caps. The host
must check every edit, or guard every patch it applies, before activation.
Accepted and rejected evaluated candidates remain host-owned lineage records.

| Directory | Purpose |
| --- | --- |
| src | Typed wire definitions, validation, lowering, edits and lineage. |
| tests | Checked-in package fixtures and public API integration evidence. |
| examples | Self-edit demonstrations, the `hive_check` command, the `hives/` package gallery (including `integrations/`), the `broken/` packages, and the `hive_view` viewer. |
