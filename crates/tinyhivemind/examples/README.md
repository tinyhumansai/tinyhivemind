# Examples

Two host implementations, written out in full, that seat real agents on a
desk and exercise this crate's ports against something other than a fixture.
Neither is a test — both need a live model backend — so both are examples,
and each says in its own README what it establishes and what it does not.

| Example | Demonstrates | Run |
| --- | --- | --- |
| [`crosstalk`](crosstalk/README.md) | The two routing edges — `choose_responder` picking who answers an unaddressed instruction, and `dispatch_mention` handing exactly one child turn to the peer an agent names — against real models, with a printed report of what each turn saw and where the chain stopped. Also shows thread scoping (`--thread`) and private asides (`--aside`). | `cargo run -p tinyhivemind --example crosstalk -- --api-base http://127.0.0.1:6969 --model flash` |
| [`desk`](desk/README.md) | A full multi-round deliberation: a JSONL-backed `SessionLog`, a `MentionTurnQueue` with real idempotency, an `opencode run` agent per turn, CortexDB-backed memory, and the chair nudging a quiet room via `choose_responder`. Written against a real, hours-long task rather than a synthetic question. | `cargo run --release -p tinyhivemind --example desk -- --desk crates/tinyhivemind/examples/desk/desks/pe1006.txt --task ./TASK.md --workspace ./ws --rounds 8` |

Both need a live endpoint or an agent CLI (`--api-base`/`--agent-cmd`); see
each example's own README for flags, backend options, and the host
obligations each run surfaced.

## Dependencies these examples take, and the library does not

`desk` takes three crates as `[dev-dependencies]` of `crates/tinyhivemind`:
`tinyinference` for the provider layer behind the wrap-up channel and the
`Digester` port, `tinytools` for rendering the room's tool surface, and
`anyhow` because `tinytools::Tool` returns it.

None of the three may be a dependency of any crate under `crates/*`, and none
is: `assert-pure.sh` reads `cargo tree -e normal,build`, which excludes
dev-dependencies, and a consumer takes the crates as path dependencies and
never builds an example. See
[ADR 0013](../../../docs/adr/0013-a-vendored-crate-is-an-example-dependency.md).
