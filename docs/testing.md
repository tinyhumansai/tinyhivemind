# Testing agent coordination

The production surface spans desks, rosters and mentions, attributed session
projection and briefing, continuous-sharing watermarks, responder selection,
and bounded mention dispatch (P2 through P7 in [`ROADMAP.md`](../ROADMAP.md)).
The core remains pure, while the runtime exposes executor-neutral ports for
host-owned logs, selection, and atomic child-turn enqueueing.

## Deterministic suite

Run the mock and feature tests with the normal workspace suite:

```sh
cargo test --all-features
```

Two deterministic fuzz-style integration suites also run as part of that
command. `fuzz_invariants.rs` in the core and hive crates generate hundreds of
Unicode- and punctuation-heavy inputs and assert parser bounds, byte offsets,
determinism, trace ordering, and quorum idempotence. They are deliberately
seeded so a failure is reproducible in CI; coverage-guided fuzzing can replay
the same invariants with its own generated corpus.

The core and runtime module suites are the authoritative P7 verification. They
cover the pure one-target decision, finite hop policy, ignored non-agent
mentions, and the atomic queue boundary, including forced concurrent duplicate
attempts that leave exactly one durable key and child turn.

`coordination_harness.rs` is a legacy test harness that drives planner, critic,
and synthesizer agents over a host-owned attributed transcript. It manually
invokes each agent; it does not exercise or prove mention-triggered dispatch.
Its model failures, desk isolation, alias, journal-ordering, and exhausted-mock
checks remain useful host-harness coverage.

## Live host verification

The legacy OpenRouter test is compiled by the normal suite but returns before
making a request unless explicitly enabled. It makes three short model calls,
so it can incur OpenRouter usage charges. Like the deterministic legacy
harness, it manually invokes agents and does not prove P7 mention dispatch.

```sh
TINYHIVEMIND_LIVE_OPENROUTER=1 \
OPENROUTER_API_KEY='<key>' \
OPENROUTER_MODEL='<provider/model>' \
cargo test -p tinyhivemind-core --features e2e --test openrouter_live -- --nocapture
```

The API key is read only by the test process and is never logged. Both the key
and model must be supplied when live execution is enabled; there is no hidden
default model or accidental network path in CI. Live execution requires
`curl`; its request configuration is sent over stdin so the API key is not
placed in the process argument list.

## Hive deliberation suite

`crates/tinyhivemind-hive` carries its own in-memory host under
`tests/support/hive_harness.rs`: it owns the journal, runs the one turn each
step authorizes, appends it, and commits the returned state. `hive_episode.rs`
drives that host through convergence, budget exhaustion, a blind opening round,
a tie that only cross-inhibition breaks, and the watermark that stops an episode
inheriting the votes of the conversation before it.

The bundled example prints a readable episode trace and needs no model:

```sh
cargo run -p tinyhivemind-hive --example hive
```

The benchmark's statistics live in an example file, so `cargo test` never runs
a `#[test]` placed among them. `--stats-check` is that coverage's stand-in: it
puts known cases through `wilson`, `paired_bootstrap` and `spearman_milli` and
exits `0` or `1`. CI runs it, along with two short benchmark shapes that
exercise the expertise and delegation paths the ordinary `--episodes 25` line
never reaches:

```sh
cargo run -p tinyhivemind-hive --example bench -- --specialists 2 --cost-tiers --episodes 25
cargo run -p tinyhivemind-hive --example bench -- --hidden-profile --episodes 25
cargo run -p tinyhivemind-hive --example bench -- --stats-check
```

Those three are smoke tests rather than assertions about accuracy: a benchmark
number is a measurement, and pinning one in CI would turn a finding into a
regression test for the weather. What they assert is that every arm runs, every
table formats, and the debug-only bookkeeping invariants in `run.rs` hold.

The live episode mirrors the OpenRouter suite above and is gated the same way:

```sh
TINYHIVEMIND_LIVE_OPENROUTER=1 \
OPENROUTER_API_KEY='<key>' \
OPENROUTER_MODEL='<provider/model>' \
cargo test -p tinyhivemind-hive --features e2e --test openrouter_hive_live -- --nocapture
```

It asserts **structure, not quality**: that real models emit parseable traces,
that exactly one agent speaks per turn, that the episode terminates inside its
budget by one of its four terminal steps, and that attribution survives. It does
not assert that the room reached a good answer, and it could not — a matched
token-budget baseline would be needed to make any such claim.

The `bench` example (`crates/tinyhivemind-hive/examples/bench/`) carries a
second, unasserted live path of its own: `--agent-cmd` drives a real scenario
through an agent CLI, and `--api-base` drives the same seats directly over
HTTP against an OpenAI- or Anthropic-shaped chat endpoint, both through
`curl` with the whole request — including the API key — sent over its stdin
rather than as a process argument. Nothing here runs in CI; it is documented
in `crates/tinyhivemind-hive/examples/bench/LIVE.md`, alongside the harness
command lines for the CLI and HTTP paths and the environment variables each
needs.

One backend cannot use the HTTP path at all. `codex exec` speaks the streaming
Responses API, which the router in front of these runs will not relay, so a
`codex` seat is driven through `--agent-cmd` against OpenRouter directly rather
than through `--api-base`. That is a property of the route rather than of the
harness, and it is why the delegation matrix carries one CLI row for a model
every other row reaches over HTTP.

End-to-end live verification belongs in the future OpenCompany host adapter,
where a committed agent reply can pass through the real atomic enqueue port and
trigger one attributed child turn through a live provider. That host work is
deliberately outside this crate's P7 library verification.

## Coverage

CI requires at least 90% line coverage in every production source file:

```sh
.github/scripts/check-file-coverage.sh 90 coverage.json
```

Test-support code lives under `tests/` and does not dilute the production
source coverage calculation.
