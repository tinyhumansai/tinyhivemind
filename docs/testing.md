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
