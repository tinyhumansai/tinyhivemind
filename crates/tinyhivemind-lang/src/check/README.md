# check

The compiler for direct edits. An orchestrator edits a package's files;
the host passes the original and edited document maps to `check` (or two
decoded packages to `check_packages`) and gets back either every violation as
a located `Diagnostic` or a `Review`.

## Design

1. The original must be valid; its findings return with rule `original`.
2. The edited files are diagnosed in full (`diagnose_documents`).
3. Every changed manifest or memory top-level field and every changed
   Markdown document becomes one derived operation, owned by the edit class
   whose `patch` boundary admits it. A field's diagnostic points at its first
   differing leaf.
4. Every rule collects rather than stops: a changed constitution, protected
   key or frontmatter (`immutable`); a class the constitution does not permit
   (`forbidden_class`); a change no class owns or a file outside the package
   (`unowned`); a canary (`contamination`); a pinned memory or binding
   (`read_only`); a context edit item deltas cannot express (`context`).
   A tool source or a file of a skill bundle (anything under `tools/` or
   `skills/`) is edited whole: the integration class owns it, and a leading
   `---` block in it is its own content, not frontmatter restating the
   manifest. The integration sections of `hive.json` and a seat's
   integration fields are owned by the same class. Without `integration` in
   `permitted_edits` the constitution refuses all of them (`forbidden_class`).
5. With no finding, operations group into one `Patch` per class in class
   order. If a step would pass through an invalid package, the first valid
   order is used; with none, one `sequence` diagnostic is returned.

Diagnostics are deduplicated by file, pointer and rule and sorted by file
then pointer. Applying `Review::changes` in order with `patch::apply`
reproduces `Review::candidate` exactly. A host records one lineage record per
change. Memory contents are not files: learning operations stay on the
`patch::apply` path.

When the edit does not decode, only the raw files are read: the constitution
as JSON and every other file for canaries.

| File | Purpose |
| --- | --- |
| `mod.rs` | `check`, `check_packages`, the raw-file fallback, sorting, and the per-class ordering search |
| `rules.rs` | Each edit rule: constitution, protected keys, canaries, read-only memories, classification and context replay |
| `types.rs` | `Review`, the derived changes and the edited package |
| `test/` | Behaviour-grouped unit tests; see [`test/README.md`](test/README.md) |
