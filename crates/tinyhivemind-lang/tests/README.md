# tests

This directory owns tests for the portable hive language.

| File | Purpose |
| --- | --- |
| example_packages.rs | Behavior regression tests. |
| gallery_packages.rs | Every package under `examples/hives/` parses and lowers, with its desks, seats, links, memory bindings and constitution pinned; no seat reaches the web except `web_search` on `euler-hive`'s research desk. |
| content_ids.rs | Every shipped package without a harness hashes to the literal content id it had before the harness fields existed. |
| direct_edits.rs | Direct edits to the gallery's `self-improving` package checked by `check`: accepted, refused with located diagnostics, and reproduced through derived patches. |
| euler_hive_package.rs | The `euler-hive` package parses and lowers from disk: five linked desks with referral enabled, web search only on research seats, `run_python` only on compute and verify, distinct personas, an agent memory binding per seat that reads the verified pool, and a placeholder problem. |
| minimal_packages.rs | A `hive.json` that omits every defaulted section parses, validates and lowers; partial policy blocks keep their other defaults; defaults still answer to the constitution; unknown fields and arrays in place of objects are still rejected; and every shipped package re-serializes to exactly its contents. |
