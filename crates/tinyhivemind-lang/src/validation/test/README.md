# validation tests

| File | Purpose |
| --- | --- |
| `mod.rs` | Module doc and submodule wiring |
| `rules.rs` | Wire contracts and fail-fast validation and parsing regressions |
| `integration.rs` | The shipped `integrations` package checks clean; each integration rule reports its own finding at its own pointer |
| `harness.rs` | The `mixed-harnesses` package checks clean; every harness reference, identity, isolation, delegation and capability finding at its pointer; scoping to packages that name a harness; frontmatter restating a harness |
| `locations.rs` | Where `diagnose` locates each finding, and agreement with `validate` |
