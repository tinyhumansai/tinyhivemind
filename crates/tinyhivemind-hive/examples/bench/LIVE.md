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

## A corpus rather than one problem

One scenario is one genre. Every scenario this harness shipped until recently
was an SRE incident, which made the corpus a test of incident triage as much as
of the protocol — so a room that held the grammar on `checkout-503` had been
shown to hold it on an incident and nothing more.

`--scenario-dir` runs every `.txt` in a directory, in name order, announcing
each before it spends minutes on it:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --scenario-dir crates/tinyhivemind-hive/examples/bench/scenarios \
  --api-base http://127.0.0.1:6969 --model flash --thinking off
```

The shipped corpus is now seven scenarios across four domains — incident triage
(`checkout-503`, `index-lock-*`), logistics (`port-congestion`), payments fraud
(`chargeback-spike`) and laboratory measurement (`assay-drift`). Every one is
the same structure: a decoy the brief itself plants, and a truth reachable only
by a conjunction of facts no single member holds. They differ in what the
decisive move *is* — a contractual prohibition, a volume figure, an inference
from the shape of an error rather than a lookup.

A scenario that fails to parse stops the run rather than being skipped, and
`scenario/test.rs` parses all seven under `cargo test`, so a typo in a fixture
is found before a run spends money on it.

## The two backends

`--agent-cmd` shells out to a CLI, one process per turn. `--api-base` instead
posts each turn straight to an HTTP endpoint through the `curl` binary — never
an HTTP crate, so the pure-crate boundary this repository enforces is never in
question — with the whole request, headers and API key included, sent over
`curl`'s own stdin rather than as a process argument. The two share the exact
same prompt assembly in `live/mod.rs`, so an HTTP seat and a CLI seat parse
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

With `--thinking off` against the local router, `checkout-503` converges on the
right answer in seven turns and ten seconds for 5729 tokens across five seats,
while the matched-budget poll ties three ways and returns no answer. That is
one round and therefore an anecdote, not a result; what it establishes is that
the cheap regime holds the grammar, which the reasoning regime was needed for
at a 6000-token ceiling.

A CLI seat's per-turn timeout (`--timeout`, default 180s) runs through
`timeout`/`gtimeout` when one is on `PATH` — macOS ships neither by default;
`gtimeout` comes from Homebrew's `coreutils` — and falls back to no deadline,
with one printed warning, when neither is found. A seat retries once on a
non-zero exit, a curl failure or non-2xx status, or an empty answer.

## A federation of live desks, running at once

`--swarm` with a live backend drives several desks of real agents. Each desk
runs its own episode with its own journal, its own budget and its own
authorized speaker, so a federation of `N` desks is `N` model calls that need
not wait on each other. `--jobs` decides how many of them are in flight:

```sh
LADDER_API_KEY='<key>' \
cargo run --release -p tinyhivemind-hive --example bench -- --swarm \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503-federated.txt \
  --api-base http://127.0.0.1:6969 --model flash --thinking off --jobs 16
```

Two things about that are worth stating rather than discovering.

**`--jobs > 1` selects a different scheduler**, and it decides differently. The
sequential scheduler finishes each desk before starting the next, so a question
routed from desk 3 to desk 7 is answered by desk 7 in the same pass; the
concurrent one plans every desk, runs every model call at once, and lands the
rows afterwards, so that question is answered one pass later. One pass of
latency moves the sequence number a row lands at, and the quorum window and
salience decay read raw sequence distance. On the recorded three-desk
federation that is the difference between `swarm 78.0` and `swarm 67.0`.
Neither is wrong and both are schedules a host could write, but **the
sequential one is the reference every recorded number was taken against**, and
it is what `--jobs 1` selects. `swarm/schedule.rs` carries the full argument.

One message, one turn is untouched either way: each desk still runs exactly one
turn per pass, authorized by its own episode. What overlaps is the waiting,
across episodes that were already independent — which is the arrangement
[ADR 0002](../../../../docs/adr/0002-hive-episodes-are-sequential.md) points a
host at. Rows land in desk order however the calls return, so a run is
reproducible at a given `--jobs`.

**Asking has its own budget.** `--ask-cap` (default 2) bounds how many other
channels one desk may ask, off the floor. Set it to `0` and asking goes back on
the floor, where a member spends its authorized turn on it — the behaviour every
recorded live federation used, and the one that collapses above roughly eight
desks. See
[the scale write-up](../../../../docs/experiments/2026-09-10-hive-at-scale.md).

**`--digest` has no live implementation.** The simulated federated comparison
gains a `swarm◦` arm in which every desk publishes its reading to every
channel, which is the only bounded mechanism that survives a federation whose
desks share a blind spot — see
[`SCALE.md`](SCALE.md#correlated-desks-and---digest). Nothing corresponding runs
live: `SwarmMember::publish` returns `None` by default and every live seat takes
that default, because publishing a reading is a prompt of its own and no live
arm has been written for it. `--digest` on a live run is therefore inert, and
the `digests` column reports the zero calls it actually made rather than
pretending otherwise.

## Driving it with a lightweight OpenHuman

Nothing here is specific to a particular endpoint: `--api-base` wants something
that speaks `/v1/chat/completions`, and a slim [OpenHuman][oh] build serves
exactly that. Its JSON-RPC server nests an OpenAI-compatible router at `/v1`, so
a headless core is a drop-in backend for every command above.

```sh
# In the openhuman checkout. The default feature set carries the desktop
# shell's dependency tree; this is the headless subset that still serves /v1.
cargo build --release --no-default-features --features http-server --bin openhuman-core
./target/release/openhuman-core run --headless-api --port 6969
```

Then point the harness at it:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --swarm \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503-federated.txt \
  --api-base http://127.0.0.1:6969 --model <model> --jobs 16
```

**No dependency is added to this repository by any of that** — the seam is a
URL, and the harness still talks to it through `curl` over stdin. That matters
for more than tidiness: `tinyhivemind-hive` is a pure crate, OpenHuman is
GPL-3.0 and vendors a large tree, and a dev-dependency on it would put both
facts inside this workspace for no gain.

The real ceiling on `--jobs` is not the machine. A hundred concurrent seats is a
hundred concurrent `curl` processes, which any developer machine will run; what
decides the useful value is how many requests the endpoint behind OpenHuman
will serve at once, and what they cost. Start small — four desks — confirm the
wire and the marker grammar, then raise it.

[oh]: https://github.com/tinyhumansai/openhuman
