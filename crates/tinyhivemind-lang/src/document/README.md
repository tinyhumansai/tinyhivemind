# document

The package wire format and the parser that reads it. A host collects a
package's files into a `BTreeMap<String, String>` and calls `parse`; this
module never opens a file.

`parse` checks that every key is a safe relative path, decodes `hive.json`,
`constitution.json` and `memory.json` (each required), keeps every other key
ending in `.md`, and every key under `tools/` (executable sources) or `skills/`
(skill bundles), as a document, and drops the rest, then runs `validate`. Every
wire struct denies unknown fields, and a second pass rejects unknown fields
nested inside core policy values, which serde alone would accept. The one
exception is `"responder_mode": "lead"` on a desk: core omits the lead mode
when it serializes, so an explicit `lead` is accepted.

Every field with a safe default may be omitted; `defaults.rs` holds the
three cases plain `#[serde(default)]` cannot express. `policies` is decoded
as an overlay on `Policies::default()` (objects merge key by key, anything
else replaces), a desk's `description` defaults to `None` without relaxing
core's own `Desk` wire form, and a seat's `label` defaults to its `id`.
Serialization still writes every field. The integration sections default to
empty (or to the defaults listed in [`integration/`](integration/README.md)),
so a package that predates them is unchanged. Because serde reads a struct from an
array of its fields, and an empty array as all defaults, a JSON array where
an object is expected is rejected at its pointer. The full table of
defaults is in the
[language specification](../../../../docs/specs/hive-language.md#optional-fields-and-defaults).

`diagnose_documents` runs the same steps without stopping: every unsafe
path, every canonical file that is missing or fails to decode (with serde's
line and column), an unknown nested field at its JSON Pointer, and every
validation finding once the three files decode. `parse` returns its first
finding.

| File | Purpose |
| --- | --- |
| `mod.rs` | `parse`, `diagnose_documents`, JSON decoding, and the nested unknown-field check shared with `patch` |
| `types.rs` | `Package`, `Manifest` and its sections, `Policies`, coordinator settings, and the seat's integration fields |
| `integration/` | Wire types for the integration sections and executable tools; see [`integration/README.md`](integration/README.md) |
| `defaults.rs` | Wire defaults for policies, desks and seats that `#[serde(default)]` alone cannot give |
| `test.rs` | Collected parse findings and their agreement with `parse` |

Parsing regressions also run alongside validation in
[`../validation/test/rules.rs`](../validation/test/rules.rs).
