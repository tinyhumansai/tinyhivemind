# Implement the hive mind

Linked specification: [`../specs/hive-mind.md`](../specs/hive-mind.md)

## Goal

Add bounded group deliberation as a third, opt-in crate — traces, salience,
quorum with cross-inhibition, and the attention market — without adding a port,
a journal, an async dependency, or fan-out.

## Test-first tasks

1. Accept the specification and record
   [`../adr/0002-hive-episodes-are-sequential.md`](../adr/0002-hive-episodes-are-sequential.md),
   which fixes the sequential-episode decision and the no-new-port claim before
   any code is written.
2. Create `crates/tinyhivemind-hive/` with its manifest, `lib.rs` and
   `error/mod.rs`; wire it into `[workspace.dependencies]` and add it to
   `pure_crates` in `.github/scripts/assert-pure.sh`. Close that script's
   own long-standing gap by giving `tinyhivemind` the narrower exempt list its
   comment already calls for.
3. Add failing wire and grammar tests, then implement `trace/`, mirroring
   `tinyhivemind-core`'s `mention/` in its two input modes and preserved spans.
4. Add failing decay tests, then implement `salience/` in fixed-point integer
   arithmetic, pinning the shipped weights rather than the published ones.
5. Add failing standing tests — including the deadlock that only cross-inhibition
   breaks, and the commutativity and idempotence of the fold — then implement
   `quorum/`.
6. Add failing bid tests, then implement `attention/`, with the threshold
   subtracted from the bid rather than merely gating it.
7. Add failing state-machine tests, then implement `episode/` and its README.
8. Add the in-memory host harness under `tests/support/` and the end-to-end
   episode suite.
9. Add `examples/hive.rs` and run it from CI.
10. Add the `e2e`-gated live OpenRouter episode, asserting structure and
    attribution only.
11. Update `ROADMAP.md`, `docs/testing.md`, and the specification and plan
    indexes.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
cargo run -p tinyhivemind-hive --example hive
.github/scripts/assert-pure.sh
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo test --doc
```

## Completion checklist

- [x] Specification accepted and the decision recorded before implementation.
- [x] The crate is pure, defines no port, and is asserted as such in CI.
- [x] Trace grammar, wire forms and both input modes tested.
- [x] Salience decay pinned, including saturation at extreme distance.
- [x] Cross-inhibition tested in both directions, so it is shown to be
      load-bearing rather than incidental.
- [x] The quorum fold proven commutative and idempotent.
- [x] One-turn, budget, blind-round and phase invariants tested.
- [x] End-to-end host harness and a runnable example.
- [x] Gated live episode asserting structure, not answer quality.
- [x] Full verification passes.
