# Language examples

| File | Purpose |
| --- | --- |
| `self_edit.rs` | Propose a prompt delta, guard the candidate, supply a host evaluation, and archive accepted and rejected designs |

Run from the repository root:

```sh
cargo run -p tinyhivemind-lang --example self_edit
```

The example uses a deterministic toy rubric: a prompt containing `evidence`
scores higher. This demonstrates host ownership of evaluation and acceptance;
it does not measure model quality or execute the benchmark harness. The archive
retains a rejected candidate while `accepted_head` selects the last accepted
candidate. Neither evaluation nor archive persistence opens a store or network.
A real host substitutes its held-out evaluation and persists candidate snapshots
with their records, including persistent memory watermarks when present.
