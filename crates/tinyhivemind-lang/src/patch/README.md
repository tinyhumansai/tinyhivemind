# Editor proposals

A `Patch` is the recorded form of an edit. Editors usually write files and
the host runs `crate::check`, which derives one patch per touched class with
the helpers in `derive.rs`. A host may still propose a patch directly and
call `apply`, and must for memory learning operations, which are not files.

The host proposes and calls `apply`; a private candidate is guarded and
validated before being returned. Host learning operations require a separate
atomic memory transaction checking the supplied watermark and previous entry.
The library opens no store and never embeds learned entries in the package.

| File | Purpose |
| --- | --- |
| `mod.rs` | Typed JSON Pointer edits, diff, atomic application and full candidate guard |
| `types.rs` | Patch classes, preconditioned operations, results and errors |
| `derive.rs` | Operations between two package states and the class owning each, shared by `diff` and `check` |
| `items.rs` | Stable `<!-- item:id -->` Markdown deltas and prompt body boundaries |
| `test.rs` | Deterministic wire, diff/apply and guard regressions |

Context item bodies end with a newline. Deltas preserve text outside item
boundaries. Prompt edits preserve JSON frontmatter. The `integration` class owns the
integration sections of the manifest, a seat's integration fields, and every
file under `tools/` and `skills/`; those files are edited whole, so the
leading-`---` frontmatter rule for prompts does not apply to them. Constitution
controls permitted classes, canaries, pinned read-only memory and package invariants.
