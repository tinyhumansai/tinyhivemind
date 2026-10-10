# Language source

All operations consume caller-owned values. No module opens storage or starts
an agent. Public feature modules are exposed through `lib.rs`.

| Module | Purpose |
| --- | --- |
| document | Canonical manifest JSON and safe Markdown document maps. |
| memory | Portable identities, lifecycle, bounded reads and recall settings. |
| validation | References, paths, graph integrity and constitution caps. |
| lowering | Core runtime values and host-owned seat creation inputs. |
| error | Structured parsing and validation failures. |
| patch | Typed, classified edits with atomic guarded application. |
| lineage | Content identities, evaluated records and archive selection folds. |
