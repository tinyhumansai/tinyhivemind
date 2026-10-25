# split

A hive's manifest may live in one `hive.json` or be split over it and three
optional files at the package root, each a JSON object holding some of the
manifest's top-level sections under their usual keys:

| File | May hold |
| --- | --- |
| `agents.json` | `roles`, `seats`, `profiles` |
| `tools.json` | `tools`, `toolpacks`, `mcp`, `skills`, `hooks` |
| `desks.json` | `desks`, `topology`, `referrals` |

`manifest` decodes each present split file on its own first, as a typed
struct that denies unknown fields, so a schema error or a section the file
does not own is reported in that file with serde's line and column. It then
inserts the split sections into `hive.json`'s object and decodes the whole
as one `Manifest`, so nothing downstream (validation, lowering, patches,
lineage, content identity) can tell a split package from a single-file one.
A section present in both `hive.json` and its split file is reported in the
split file and the manifest is not assembled.

Rules locate every manifest finding in `hive.json`. `home` names the split
file a finding belongs in instead: the one present in the document map that
holds the section the finding's pointer starts in. Parsing applies it to
every finding it collects, and `crate::check` to every diagnostic of an
edit, so `agents.json:/seats/0/role` is reported where the editor wrote it.
`crate::diagnose`, which sees a decoded package without its files, reports
in `hive.json`.

| File | Purpose |
| --- | --- |
| `mod.rs` | The split files, their sections, assembly, and `home` |
| `test.rs` | The typed files hold exactly their sections, and `home` |
