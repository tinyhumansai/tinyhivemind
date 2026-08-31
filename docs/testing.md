# Testing agent coordination

The current production surface is conversation identity (P1 in
[`ROADMAP.md`](../ROADMAP.md)). The coordination tests therefore use a small
host-owned harness around that real public API. The harness owns its journal
and model engines, just as a consuming host must; it does not add storage or a
model client to `tinyteams-core`.

## Deterministic suite

Run the mock and feature tests with the normal workspace suite:

```sh
cargo test --all-features
```

`coordination_harness.rs` drives planner, critic, and synthesizer agents over
one attributed transcript. It also covers one-message/one-turn dispatch,
failed model calls, named-desk isolation, General-desk aliases, desk-id case
sensitivity, journal ordering, and exhausted mock responses.

## OpenRouter live suite

The live test is compiled by the normal suite but returns before making a
request unless explicitly enabled. It makes three short model calls, so it can
incur OpenRouter usage charges.

```sh
TINYTEAMS_LIVE_OPENROUTER=1 \
OPENROUTER_API_KEY='<key>' \
OPENROUTER_MODEL='<provider/model>' \
cargo test -p tinyteams-core --features e2e --test openrouter_live -- --nocapture
```

The API key is read only by the test process and is never logged. Both the key
and model must be supplied when live execution is enabled; there is no hidden
default model or accidental network path in CI. Live execution requires
`curl`; its request configuration is sent over stdin so the API key is not
placed in the process argument list.

## Hive deliberation suite

`crates/tinyteams-hive` carries its own in-memory host under
`tests/support/hive_harness.rs`: it owns the journal, runs the one turn each
step authorizes, appends it, and commits the returned state. `hive_episode.rs`
drives that host through convergence, budget exhaustion, a blind opening round,
a tie that only cross-inhibition breaks, and the watermark that stops an episode
inheriting the votes of the conversation before it.

The bundled example prints a readable episode trace and needs no model:

```sh
cargo run -p tinyteams-hive --example hive
```

The live episode mirrors the OpenRouter suite above and is gated the same way:

```sh
TINYTEAMS_LIVE_OPENROUTER=1 \
OPENROUTER_API_KEY='<key>' \
OPENROUTER_MODEL='<provider/model>' \
cargo test -p tinyteams-hive --features e2e --test openrouter_hive_live -- --nocapture
```

It asserts **structure, not quality**: that real models emit parseable traces,
that exactly one agent speaks per turn, that the episode terminates inside its
budget by one of its four terminal steps, and that attribution survives. It does
not assert that the room reached a good answer, and it could not — a matched
token-budget baseline would be needed to make any such claim.

## Coverage

CI requires at least 90% line coverage in every production source file:

```sh
.github/scripts/check-file-coverage.sh 90 coverage.json
```

Test-support code lives under `tests/` and does not dilute the production
source coverage calculation.
