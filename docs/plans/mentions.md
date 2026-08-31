# Implement roster and mention resolution

Linked specification: [`../specs/mentions.md`](../specs/mentions.md)

## Goal

Add the pure roster, mention grammar, normalization, responder selection, and
context expansion needed by later runtime phases. Do not add async behavior,
storage, dispatch, or host types.

## Test-first tasks

1. **Roster structure and wire form**
   - Add failing validation and serde tests in
     `crates/tinyhivemind-core/src/roster/test.rs`.
   - Implement `RosterMember`, `Person`, and borrowed `Roster` in the roster
     module, with typed errors in `src/error/mod.rs`.
2. **Mention payloads and extraction**
   - Add failing wire, grammar, Unicode-offset, ambiguity, desk-only, and code
     masking tests in `src/mention/test.rs`.
   - Implement payload types, alias construction, span masking, and extraction.
3. **Supplied mention revalidation and normalization**
   - Add failing tests for authoritative empty input, malformed spans, stale
     targets, self mentions, repeated targets, and the 50-ping cap.
   - Implement the supplied path and shared reading-order normalization.
4. **Pure routing decisions**
   - Add failing tests for agent-only responder choice and expansion of agent,
     desk, and everyone context targets.
   - Implement `direct_responder` and `mentioned_members`; neither may dispatch.
5. **Public surface and documentation**
   - Export the modules from `src/lib.rs`, add a runnable example, link this
     plan/spec from their indexes, and mark P3 done/P4 next in `ROADMAP.md`.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy -p tinyhivemind-core --all-targets --all-features -- -D warnings
cargo test -p tinyhivemind-core --all-features
.github/scripts/assert-pure.sh
```

## Completion checklist

- [x] Specification accepted before implementation.
- [x] Roster validation and exact wire forms tested.
- [x] Grammar, masking, supplied, and normalization paths tested.
- [x] Responder and mentioned-member behavior tested.
- [x] Public docs, indexes, and roadmap updated.
- [x] Focused verification passes.
