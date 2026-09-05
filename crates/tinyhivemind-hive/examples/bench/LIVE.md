# Live rooms

Driving [the deliberation benchmark](README.md) through real agents: one
process per turn, or one HTTP request per turn, against a problem with a
recorded answer.

## One process per turn

`--agent-cmd` swaps the simulated participants for a real agent CLI — one
process per turn, any command that takes a prompt as its final argument and
prints an answer:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest" --agents 5
```

```sh
cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "claude -p"
cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "codex exec"
```

The library still authorizes exactly one speaker per step, so the number of
processes an episode can start is bounded by its turn budget and by nothing
else. Coloured output and a banner are stripped before the marker line is read.

The prompt is not a static block of protocol text, because live rooms fail in
ways the simulation cannot reach. It names the options already on the floor
with their standings, folded through the library's own `standings`, so support
does not split across two names for one idea; it shows a participant its own
last line, because models restate it verbatim; it offers only the moves that
count in the turn's phase, because a `!commit` written during deliberation adds
no supporter; and it calls out the grammar's `#` and `^` sigils, because models
drop them. Each of those is a host obligation rather than something the library
can impose, and each was found by running the thing.

## A real problem

Without a scenario the live room deliberates a brief with no answer, which
measures whether a model can hold the grammar and nothing else. `--scenario`
gives it a problem that has one:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/openai/gpt-5-mini" \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --repeat 5
```

A scenario file is a shared brief, the options under the ids the room should
use, a private brief per member, and the recorded answer:

```text
task: what the room must decide
truth: the option id that is genuinely right

[option rollback]
One sentence describing it.

[agent planner]
role: release manager, who owns what can and cannot be shipped
knows: a fact this member holds and nobody else does
```

The private briefs are deliberately not appended to the shared journal. A fact
every member can already read is not private information, and a room whose
members all start from the same facts has nothing to pool.

A scenario can also name specialists, so a run can be scored not just on
whether the room reached the truth but on whether it leaned on the member who
actually held the decisive fact:

```text
truth_expert: dba

[option reindex]
expert: dba
Drop the partial index and rebuild it against a narrower predicate.

[agent dba]
role: database engineer, who owns the datastore
expert_on: indexes
tier: reasoning
knows: a fact this member holds and nobody else does
```

- `expert:` inside `[option ...]` names the agent id it is honest to defer to
  on that option. It is optional, and set only where the room has a real
  reason to trust one member's judgment on that specific call.
- `expert_on:` inside `[agent ...]` is repeatable and names the *areas* a
  member is the room's specialist in (`locks`, `indexes`), never an option id
  — so knowing who the specialist is does not itself give away which option is
  right. It is folded into that member's private brief as an explicit caveat:
  being leaned on for an area is not the same as being correct about it.
- `tier: cheap` or `tier: reasoning` inside `[agent ...]` records which model
  tier a seat is meant to stand in for, so a run can compare a room that spends
  its one expensive seat on the member who needs it against a room of uniform
  seats.
- `truth_expert:` at the top level names the agent id who holds the fact that
  actually decides the truth. That member must hold at least one `knows:`
  line, or the scenario fails to parse.

`crates/tinyhivemind-hive/examples/bench/scenarios/index-lock-expert.txt` and
its `index-lock-tiers.txt` twin use all four keys: a write-latency incident
where the seductive move is rolling back last night's migration and the
correct one is rebuilding a partial index that has quietly stopped being
selective. One member holds the decisive number; every other member holds a
fact that rules out exactly one wrong move and nothing more, and no member's
private brief read alone reaches the truth. The `-tiers` file is the same
incident with `tier:` set on every seat — four `cheap`, and the single
`reasoning` seat is the same member `truth_expert` names.

Each round runs both arms against the same real agents: one deliberation
episode, then an independent poll of the same members answering alone, decided
by plurality and scored as no answer on a tie. `--repeat` runs the pair N
times, because a live room is sampled rather than computed and one episode is
an anecdote.

The scenario that ships here is a hidden profile — the shared brief plants the
wrong answer and the right one is reachable only by pooling facts across four
members. That shape is what lets the poll lose; a scenario whose answer
survives deleting every private brief measures nothing. Its header comment
records the two designs that failed that test before this one passed it.

Live mode asserts nothing and is not part of CI; it is for watching real agents
hold — or fail to hold — the trace grammar, and for watching whether a room
pools what its members separately know.
`crates/tinyhivemind-hive/tests/openrouter_hive_live.rs` is the asserting
version, behind the `e2e` feature.

## The two backends

`--agent-cmd` shells out to a CLI, one process per turn. `--api-base` instead
posts each turn straight to an HTTP endpoint through the `curl` binary — never
an HTTP crate, so the pure-crate boundary this repository enforces is never in
question — with the whole request, headers and API key included, sent over
`curl`'s own stdin rather than as a process argument. The two share the exact
same prompt assembly in `live.rs`, so an HTTP seat and a CLI seat parse
identically, and a room may even mix the two through `--seat-model` and
`--seat-cmd`.

Against a local ladder router serving both an OpenAI-shaped
`/v1/chat/completions` and an Anthropic-shaped `/v1/messages`:

```sh
LADDER_API_KEY='<key>' \
cargo run --release -p tinyhivemind-hive --example bench -- \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --api-base http://127.0.0.1:6969 --model flash --repeat 1

LADDER_API_KEY='<key>' \
cargo run --release -p tinyhivemind-hive --example bench -- \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --api-base http://127.0.0.1:6969 --model flash --wire anthropic --repeat 1
```

Against the same router through the `claude` and `opencode` CLIs — pointing
each CLI's own provider configuration at the router rather than the harness
talking to it directly:

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:6969 ANTHROPIC_AUTH_TOKEN=$LADDER_API_KEY ANTHROPIC_API_KEY= \
cargo run --release -p tinyhivemind-hive --example bench -- \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --agent-cmd "claude -p --model flash" --repeat 1

OPENCODE_CONFIG_CONTENT='{"provider":{"ladder":{"name":"Ladder","npm":"@ai-sdk/openai-compatible","options":{"baseURL":"http://127.0.0.1:6969/v1","apiKey":"'"$LADDER_API_KEY"'"},"models":{"flash":{"name":"flash"}}}}}' \
cargo run --release -p tinyhivemind-hive --example bench -- \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --agent-cmd "opencode run -m ladder/flash" --repeat 1
```

Every HTTP seat's tokens, calls, and cost (at `--model-cost`, default 1 unit
per 1000 tokens) print after the episode, totalled again by the `tier:` the
scenario put each seat on, and the independent poll runs through whichever
backend the seats themselves ran under — the control has to match the arm it
is scored against or the comparison means nothing. Each round also reports
whether the scenario's `truth_expert` spoke *before* the commit, whether the
winning commit's citation chain reaches anything that member said, and how
many turns went on `!defer`; a federated run adds the channel crossings.

The reply budget is 16000 tokens. `flash` spends its budget reasoning before
it writes the marker line, and 6000 was not enough for the shipped
hidden-profile scenario — seats reasoned in circles past an 8000-token budget
and returned empty turns. At 16000 every seat finishes and emits its line at
roughly 300 completion tokens a turn, so the budget is a ceiling the run does
not approach. `--thinking off` asks for the cheap regime instead — about 38
completion tokens on the same prompt — by sending `"thinking": {"type":
"disabled"}` on the `OpenAI` wire; on the `Anthropic` wire, which only thinks
when asked, it sends no thinking block, which is what that wire already did.
An HTTP seat's answer goes through the same `marker_line` a CLI seat's stdout
does, so the two parse identically.

A CLI seat's per-turn timeout (`--timeout`, default 180s) runs through
`timeout`/`gtimeout` when one is on `PATH` — macOS ships neither by default;
`gtimeout` comes from Homebrew's `coreutils` — and falls back to no deadline,
with one printed warning, when neither is found. A seat retries once on a
non-zero exit, a curl failure or non-2xx status, or an empty answer.
