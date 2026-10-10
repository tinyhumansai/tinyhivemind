# Editor proposals

The host proposes and calls `apply`; a private candidate is guarded and
validated before being returned. Host learning operations require a separate
atomic memory transaction checking the supplied watermark and previous entry.
The library opens no store and never embeds learned entries in the package.

| File | Purpose |
| --- | --- |
| `mod.rs` | Typed JSON Pointer edits, diff, atomic application and full candidate guard |
| `types.rs` | Patch classes, preconditioned operations, results and errors |
| `items.rs` | Stable `<!-- item:id -->` Markdown deltas and prompt body boundaries |
| `test.rs` | Deterministic wire, diff/apply and guard regressions |

Context item bodies end with a newline. Deltas preserve text outside item
boundaries. Prompt edits preserve JSON frontmatter. Constitution controls
permitted classes, canaries, pinned read-only memory and package invariants.
