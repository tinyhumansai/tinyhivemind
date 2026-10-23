# Language examples

| File | Purpose |
| --- | --- |
| `hive_check/` | The compiler-style check an orchestrator runs after editing a package's files: every finding as `file:pointer: rule: message`, non-zero exit on any |
| `hive_view/` | Render a package (text, mermaid, JSON) and run it through core's `hive::step`, one floor per desk linked by referral, offline or `--live` on OpenRouter; NDJSON frames for the viewer; the math `--compare` |
| `hives/` | The example package gallery: eight complete packages, from a three-seat review board to a fifty-seat math swarm |
| `live_self_edit/` | Provider-failure regression tests and their directory guide |
| `live_self_edit.rs` | Opt-in OpenRouter editor returning whole files, `check`ed by the host; exact-match held-out evaluation, negative control and telemetry/contamination refusals |
| `self_edit.rs` | An editor returns whole files, the host `check`s them, evaluates the candidate, and archives one lineage record per derived change |
| `self_edit/` | The self-edit example's offline test and its directory guide |

Run from the repository root:

```sh
cargo run -p tinyhivemind-lang --example self_edit
cargo test -p tinyhivemind-lang --example self_edit
```

## Checking an edit

```sh
cargo run -p tinyhivemind-lang --example hive_check -- <original-dir> <edited-dir> [--json]
cargo run -p tinyhivemind-lang --example hive_check -- <package-dir>
```

An orchestrator edits a copy of a package and runs this; exit status 1 means
at least one finding. See [`hive_check/README.md`](hive_check/README.md).

The self-edit example's editor returns whole files rather than a patch; one
edit that also raises `max_turns` in the constitution is refused with a
located diagnostic. The example uses a deterministic toy rubric: a prompt
containing `evidence` scores higher. This demonstrates host ownership of evaluation and acceptance;
it does not measure model quality or execute the benchmark harness. The archive
retains a rejected candidate while `accepted_head` selects the last accepted
candidate. Neither evaluation nor archive persistence opens a store or network.
A real host substitutes its held-out evaluation and persists candidate snapshots
with their records, including persistent memory watermarks when present.

## Viewing and running a package

```sh
cargo run -p tinyhivemind-lang --example hive_view -- \
  crates/tinyhivemind-lang/examples/hives/review-board --run
cargo test -p tinyhivemind-lang --example hive_view
```

[`hive_view/README.md`](hive_view/README.md) documents every mode, the wire
format the viewer reads, and the measured math `--compare` result.
[`hives/README.md`](hives/README.md) lists the gallery.

## Live model check

```sh
cargo run -p tinyhivemind-lang --example live_self_edit -- \
  --live target/live-self-edit-run-1
```

Set `OPENROUTER_API_KEY` in the environment. `OPENROUTER_MODEL` optionally
changes the default `openai/gpt-oss-120b:nitro`. The example host invokes curl;
the library retains its pure boundary. Credentials pass on curl stdin and are
excluded from artifacts. Every model request, response, candidate and lineage
record is saved below the chosen output directory, which must be different
for each run.

The live editor returns the whole new `seats/solver.md`, and the host runs
`check` on it. The incumbent deliberately contains a status-filter bug. The editor sees that
training feedback, but not the three held-out invoice inputs. The host checks
numeric answers exactly, requires all three to pass, and rejects a deliberately
broken prompt after evaluating it with the same live model. It also asks the model to return a `hive.json` with telemetry disabled, which `check` must refuse as `immutable`, and checks a contamination canary.
A provider refusal is retained explicitly; the host then tests the requested
mutation fixture itself and records its origin. This is a small
integration smoke test, not evidence of general self-improvement on SWE tasks.

Use the resulting `accepted-package.json` with the adapter's
[live language example](../../tinyhivemind-openhuman/examples/README.md) to
exercise native tool completion and registration from the lowered package.

Failed provider calls retain redacted response bodies in `response-N.error`
and include a bounded, redacted stderr excerpt in the returned error. Run the
offline regression with `cargo test -p tinyhivemind-lang --example live_self_edit`.
