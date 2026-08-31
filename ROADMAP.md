# Roadmap

Replace this file with the real plan for the crate generated from this
template, or delete it if the project does not need a public roadmap.

Keep it short and honest: what exists, what is next, and what is deliberately
out of scope. A roadmap that lists everything is a roadmap nobody trusts.

## Shipped

- module layout, crate-wide error type, and the public re-export surface
- lint configuration in `[lints]`, enforced identically locally and in CI
- CI: format, clippy, build, test, per-file coverage, rustdoc, MSRV, and
  supply-chain checks
- a manual release workflow that versions, tags, publishes to crates.io, and
  creates a GitHub release with crate and TinyBus runtime/module assets

## Next

- the first real feature area, replacing the placeholder `greeting` module
- module-level `README.md` and `docs/spec/` entries as modules grow

## Out Of Scope

- anything that cannot be tested deterministically
- convenience wrappers that hide the crate's error taxonomy from callers
