# diagnostic

The located finding every rule reports. A `Diagnostic` names a `file`, an
optional JSON `pointer` into it, a `rule` family and a lowercase `message`,
and displays compiler style: `hive.json:/seats/0/role: reference: unresolved
role reference ghost`. `Rule` has one snake_case variant per family; the first
thirteen mirror `crate::Error`, the rest are the edit rules `check` adds.

Inside the crate, validation and parsing push `(Error, file, pointer)`
findings into a `Findings` collector. `validate` and `parse` return its first
finding as the error they always returned; `diagnose`, `diagnose_documents`
and `check` return all of them, without repeats.

| File | Purpose |
| --- | --- |
| `mod.rs` | The collector, canonical file names, and pointer escaping |
| `types.rs` | `Diagnostic`, `Rule`, and the `Error`-to-`Rule` mapping |
| `test.rs` | The pinned wire, every rule name, the error mapping, and the collector |
