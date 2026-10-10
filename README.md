<img src="https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/hero.png?raw=true" />

<h1 align="center">tinyhivemind</h1>

<p align="center"><strong>Hive mind mechanics for agents</strong></p>

<p align="center">
Quorum sensing, cross-inhibition, stigmergy, salience decay and response
thresholds, implemented as integer folds over a transcript your application
already owns. Hosts expose episode tools through their own tool interface.
</p>

<p align="center">
<a href="https://github.com/tinyhumansai/tinyhivemind/wiki">Wiki</a> &nbsp;·&nbsp;
<a href="https://github.com/tinyhumansai/tinyhivemind/wiki/Quick-start">Quick start</a> &nbsp;·&nbsp;
<a href="https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks">Benchmarks</a> &nbsp;·&nbsp;
<a href="https://github.com/tinyhumansai/tinyhivemind/wiki/Architecture">Architecture</a>
</p>

---

> [!NOTE]
>
> A note from [@senamakel](https://github.com/senamakel/).
>
> This is one of my best works so far and one of the most important libraries that I have worked on: tinyhivemind takes inspiration and learnings from real life biology and my experience building harnesses, coordinating with agents, and building agents that can solve large, complex problems.
> 
> This concept was initially built inside of OpenCompany but had to be later on moved into it's own standalone repo as it was too important to be left inside of OpenCompany and it had to be well-defined, researched, tested, and simulated thoroughly.
>
> I'm excited to share this with you all as an open-source contribution and if you like my work, give me a follow over at https://github.com/senamakel/ 🙌

## Most hive minds are just fan-out

Publish a task, wake N agents, collect the replies, average them somehow. That
is a thread pool with a prompt attached. It has no notion of who is convinced,
no way to register a grounded objection, no reason to stop other than running
out of members, and no answer when somebody asks afterwards why the group chose
what it chose.

Actual collective decisions do not work that way, and the mechanisms that make
them work have been studied for decades in colonies that have no leader, no
shared memory, and far less bandwidth than five language models sharing a
channel. tinyhivemind implements those mechanisms.

The shape of it is a loop, and your application holds both ends:

```text
  your application                                  tinyhivemind
  ┌───────────────────────────────┐
  │ session log (you own it)      │
  │  1 planner  !propose #stage … │ ─── transcript ──┐
  │  2 scout    !propose #ship  … │     roster       │
  │  3 critic   !support #stage … │     desks        ▼
  │                               │     ┌─────────────────────────┐
  │                               │     │ step(state, …)          │
  │  4 planner  !object  >3     … │     │   -> HiveStep           │
  │  ▲                            │     │ a pure fold. no IO.     │
  └──┼────────────────────────────┘     └────────────┬────────────┘
     │                                               │
     └──── one message, one round ◀── Speak { turns } ┤
                                                     │
      Converged · Deadlocked · Exhausted · Idle ◀────┘
```

`Speak { turns }` above is shorthand for `Speak { turns, next_state }`, kept
off the diagram to stay narrow, not because it stopped mattering. The `step`
fold reads what you hand it and returns what should happen next. The host owns
the transcript and exposes the tools seats call.

## The mechanics

**[Stigmergy](https://en.wikipedia.org/wiki/Stigmergy).** Work leaves a trace
in a shared medium, and the trace is the stimulus for the next piece of work.
No agent addresses another and nothing dispatches anything. The transcript is
the medium, and a marker line is a deposit in it.

```text
!propose #stage Stage the rollout across three regions.
!support #stage ^1 Staging bounds the blast radius if the migration is wrong.
!object  >3      The regions are not independent, so this bounds nothing.
!commit  #stage
```

**[Pheromone decay](https://en.wikipedia.org/wiki/Ant_colony_optimization_algorithms).**
A trace's pull on the room's attention decays
[exponentially](https://en.wikipedia.org/wiki/Exponential_decay) with distance
in the transcript. Without it, whoever spoke first holds the floor forever,
which is the failure ant trails avoid only because pheromone evaporates.

```text
one !support trace, rescored as the room talks past it

  distance    recency term                salience
      0       ████████████████  1000        3000
     10       ████████████       750        2875
     20       ████████           500        2750
     40       ████               250        2625
     80       █                   62        2531
```

The floor under the bars is standing importance: an old proposal can outrank a
fresh question, while recency is the term that moves.

**[Quorum sensing](https://en.wikipedia.org/wiki/Quorum_sensing).** An option
carries when some number of distinct participants have grounded support for it
inside a window. Not a majority of anything, not a score to beat. The count is
local, order independent and idempotent, so an agent that catches up late folds
to exactly the same standing as one that watched live. This is how
[honeybee swarms](https://en.wikipedia.org/wiki/Swarming_%28honey_bee%29)
settle a nest site.

With a typed model, `standings_with_evaluations` uses fixed-point Choice × Score after a Noul gate; see the [quorum module](crates/tinyhivemind-core/src/hive/quorum/README.md).

```text
1 planner  !propose #stage Stage the rollout.
2 scout    !propose #ship  Ship it all at once.
3 critic   !support #stage ^1 Staging bounds the blast radius.

  #stage  supporters ["planner", "critic"]   ->  carries at threshold 2
  #ship   supporters ["scout"]

4 auditor  !support #stage I agree.           <- no citation, counts for nothing
```

**[Cross-inhibition](https://en.wikipedia.org/wiki/Lateral_inhibition).** An
objection names a *message*, and removes that message's author from the
supporter set of whatever they were advocating. It does not debit the option.
Subtracting from a score cannot break a tie between two equally supported
options; silencing an advocate can, and that asymmetry is the entire reason it
is shaped this way. Honeybees do this too, with stop signals.

```text
both options carry, so the room is deadlocked
  #stage  supporters ["planner", "critic", "auditor"]
  #ship   supporters ["scout", "auditor"]

7 planner  !object >6 ^1 The regions are not independent.

  #stage  supporters ["planner", "critic", "auditor"]
  #ship   supporters ["scout"]  silenced ["auditor"]   ->  #stage carries
```

The objection travels through the message to the author, and only then to the
option — never straight at the option:

```text
   !object >6           authored by            advocate for
  planner ─────▶ msg 6 ────────────▶ auditor ──────────────▶ #ship
                                        │
                                        └── removed from #ship's supporters,
                                            still counted for #stage
```

**[Response thresholds](https://en.wikipedia.org/wiki/Task_allocation_and_partitioning_of_social_insects).**
Every member computes an urge from the salience field and its own affinity, and
whoever bids highest takes the floor. A member whose urge never clears its
threshold does not bid at all. This is the response-threshold model of division
of labour, and it is also
[Pandemonium](https://en.wikipedia.org/wiki/Pandemonium_architecture)'s
decision demon, which is the same idea arrived at from the AI side.

```text
planner  urge 10312  Addressed     <- somebody cited its message
scout    urge  8312  Salience
critic   urge  8312  Salience
auditor  --                        <- threshold never cleared, does not bid

floor = planner
```

Every one of those is
[fixed-point](https://en.wikipedia.org/wiki/Fixed-point_arithmetic) integer
arithmetic. Every payload derives `Eq`, and every episode replays byte for byte
from the same transcript.

## The floor is a substrate, not a broadcast

The mechanics need a room to run in, so tinyhivemind owns that too, and it is
the part most systems get wrong first. Five agents in one channel read each
other's replies as their own words, miss what a peer said between their own two
turns, and stampede on a single `@everyone`.

**Who is here?** A roster of agents, and the people signed in alongside them.

**What is a desk, and who is on it?** A declared room merged with the
operator's runtime additions, retirements and ordering.

**Who does a name address?** A mention grammar resolved against the live
roster and desks, where only a direct agent mention can start a turn.

**What does one participant see?** An attributed, thread-aware projection of a
multi-speaker transcript into one viewer's history, so agent B never reads
agent A's words as its own.

```text
  the shared transcript                 what agent B is handed
  ┌───────────────────────────┐         ┌────────────────────────────┐
  │ 1  ana      (person)      │         │ user       ana: …          │
  │ 2  agent A                │  ─────▶ │ user       agent A: …      │
  │ 3  agent B                │         │ assistant  …               │ ← its own
  │ 4  ana      (person)      │         │ user       ana: …          │
  └───────────────────────────┘         └────────────────────────────┘
                               ─────▶   what agent A is handed
                                        ┌────────────────────────────┐
                                        │ user       ana: …          │
                                        │ assistant  …               │ ← its own
                                        │ user       agent B: …      │
                                        │ user       ana: …          │
                                        └────────────────────────────┘
```

One log, one sequence numbering, two histories. Every line a viewer did not
write arrives as somebody else's, named.

## A desk is a correlation boundary, so a question can leave it

One room can pool what its members separately know. A *company* of rooms cannot,
and that is not an inconvenience — it is the thing that decides the answer.
Members of one desk read the same transcript, work the same part of the system,
and are wrong about the same things. Averaging correlated error does not remove
it, so no amount of deliberating inside a channel cancels a mistake every member
of it shares.

A **referral** is the one mechanism here that leaves a conversation: at most one
turn, which may run on another desk, and one answer carried back to the
conversation that asked. `@#platform` is not a broadcast — it resolves to
exactly one agent before the decision leaves the fold — and a round trip is two
hops, so it cannot ring.

Three desks, each confidently wrong about a different option, deciding one
question: deliberating inside the channels gets it right **0.2%** of the time,
and every desk converges anyway, on three incompatible answers. Putting all
twelve members in one room instead — removing the boundary rather than crossing
it — gets **10.5%**, because a bigger room with three factions never reaches
quorum. Crossing it gets **77.5%**, which matches what the same information is
worth when handed over for free.

It is off by default, and at desks with no blind spot of their own it changes
no answer and costs twice the turns. That is the honest case for leaving it off.

## Keep a short working view

That projection is bounded — about thirty messages — and the log behind it is
not. The obvious fix is to show more, and it is the wrong one:
[Lost in the Middle](https://arxiv.org/abs/2307.03172) finds a fact in the
middle of a long context is used less reliably than the same fact at the edge
of a short one, so a bigger window mostly relocates the problem into its own
middle. It also charges every participant on every turn.

**Pin what must not be lost.** `!pin` folds out of the transcript, not into a
second store, and the board rides into every turn.

**State the budget.** The briefing tells an agent what a message costs the room
it is written into. Reported, never enforced: nothing here rewrites what
somebody said.

## An episode ends for a reason you can name

Every step walks the same ladder in the same order, and the first rung that
answers is the answer:

```text
  step(state, transcript, roster, desks, policy)
    │
    ├─ budget spent? ─────────────────────────▶ Exhausted { spent, standings, visibility }
    ├─ quorum, and phase = Commit? ───────────▶ Converged { topic, .. }
    ├─ quorum, and phase = Deliberate? ───────▶ Speak { turns, next_state }
    │                                           one commit turn; phase flips
    ├─ two topics carry, nobody to break it ──▶ Deadlocked { topics }
    │
    └─ highest bid clears its threshold? ─────▶ Speak { turns, next_state }
                                    otherwise ▶ Idle
```

The phase only ever moves one way, and the room gets exactly one chance to say
out loud what it settled on:

```text
   ┌─────────────┐   quorum reached   ┌──────────┐   still holds
   │ Deliberate  │ ─────────────────▶ │  Commit  │ ───────────────▶ Converged
   └─────────────┘   emit !commit     └──────────┘
```

Converged, deadlocked, exhausted, or idle. A room that could not decide says
so, instead of emitting an answer nobody actually supported. The turn budget is
finite, so termination is guaranteed rather than hoped for, and the standing
that carried is returned alongside the outcome.

## One synthetic decision task

Five agents choosing between four options, 5000 seeded rooms, on one core:

| arm | what it is | turns | correct |
| --- | --- | --- | --- |
| `ladder` | one responder answers alone, which is how most systems work today | 1.00 | 57.6% |
| `vote` | independent answers, plurality, matched budget | 15.00 | 78.5% |
| `hive+` | a tuned deliberation episode | 6.75 | **82.1%** |

The middle row is an independent vote at a matched turn budget. On this seeded
task, tuned deliberation beats that control by 3.6 points while spending fewer
turns. The participants here are simulated, so the table measures this
protocol's behavior on the constructed task, not how much better a language
model will answer in an application.

The
[benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the rest: the two bounds on the quorum threshold, why the turn budget has
to scale with the desk, what happens to accuracy without a blind opening round,
what five live models did to the grammar when nobody was watching, and an
honest section on what none of it shows.

## One task, one agent. Two or more, one seat each

A room is not a longer agent — it is a **wider** one, and the benchmark is
specific about where that line falls. On a task that merely gets *longer*, a
single agent compacting by a superseding account wins at every horizon. On one
that gets *wider* — several sub-decisions at once — it flips:

| facets in one task | 1 | 2 | 4 | 8 |
| --- | --- | --- | --- | --- |
| one agent, compacting | 78.7% | 56.9% | 28.1% | 7.8% |
| one seat per facet | **78.7%** | **61.1%** | **35.9%** | **13.0%** |
| rows one participant holds | 20 | 40 → 20 | 80 → 20 | 160 → 32 |

At one facet they are identical and the room is the wrong tool. From two on, a
seat's accuracy stays flat in width where the soloist's decays — a seat never
holds facets it is not deciding — and eight facets cost two rounds, not eight.
The advantage is the context window, bought by dividing along a line where
competence differs; divide it anywhere else and it vanishes. [Benchmarks: delegation](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks-delegation) has the controls.

## Not an agent council

A council is a conversation with roles: a manager or a round-robin picks the
next speaker, and it stops on a round cap or when the manager says so.

| | agent council | tinyhivemind episode |
| --- | --- | --- |
| who speaks next | a manager model, or round-robin | argmax over per-member bids |
| what agreement is | inferred from the replies | an explicit supporter set |
| what disagreement is | a message saying "I disagree" | an objection that removes an advocate |
| how it ends | round cap, or the manager stops | quorum, deadlock, exhaustion or idle |
| cost per round | one turn per member | one bounded round of turns |
| replay | re-run and hope | byte-identical from the same transcript |

Councils are better at open-ended writing, at work that genuinely decomposes,
and at running on any model with no grammar to learn. Take one when the
deliverable is prose. Take this when the deliverable is a decision somebody
will ask you to justify later.
[Hive episodes](https://github.com/tinyhumansai/tinyhivemind/wiki/Hive-episodes)
explains the decision protocol.

## Boundaries

The host owns agent construction, sessions, credentials, and authorization.
`tinyhivemind-core` holds the grammar, pure folds, host-supplied session ports,
semantic routing, TypeSafe System One adapter, and completion driver in focused
modules. It opens no storage or transport.

A normal message selects one responder. A mention or referral can authorize at
most one child turn from a committed reply. A hive episode may authorize a
bounded round of concurrent turns: `round_width` limits blind rounds and
`revealed_width` limits rounds that can read previous peer work. The host
commits the round's state after all authorized turns are appended.

`tinyhivemind-lang` makes a hive’s definition editable data: roles, goals,
seats, memory bindings and policies, with guarded patches and an evaluated
lineage. Read the [language specification](docs/specs/hive-language.md).

`tinyhivemind-tools` renders native `tinytools::ToolSpec` definitions and
records episode calls. `tinyhivemind-hives` coordinates durable messages and
serialized turns across dynamic hives with memory, SQLite, or host storage.
`tinyhivemind-openhuman` attaches tools to already configured OpenHuman agents. The
[architecture guide](https://github.com/tinyhumansai/tinyhivemind/wiki/Architecture)
and [crate dependency map](docs/crate-dependencies.md) show their boundaries.

### Supplied OpenHuman agents

Construct all agents on one OpenHuman `Runtime`, with their MCPs, skills,
memory, providers, and prompts configured before registration. Pass the handles
to `OpenHumanHost::new(runtime_id, coordinator)?`, then `register_agent(agent)?`
or `register_agent_in_session(agent, session_id)?` for an existing conversation.
Each agent retains one continuing session across all joined hives. A hive and a
desk have one public `hive_id`.

Nine permanent tools stay in the system catalogue and provider schemas under
deferred discovery. Four additional management tools require an explicit host
factory and authorizer. The host can also create hives, register agents, and
change membership directly while scheduling runs. Direct replies can be read
through `hivemind_read` without scheduling automatic return turns.

Run the [offline coordinator examples](examples/hives/README.md)
for a reviewer handoff and a session shared across hives. The
[basic OpenHuman hive](examples/openhuman/basic-hive/README.md) runs two supplied
agents in and out of a hive, offline or through OpenRouter. The
[standalone host proof](examples/openhuman/README.md) covers four topologies
and native MCP/skill integration. The [OpenCompany migration guide](docs/opencompany-migration.md)
describes registration, recovery, and the required single-runtime boundary.

## Frequently asked questions

### Does tinyhivemind create a new language or literally share agents' minds?

No. It is a Rust library, not a programming language or a model. Agents still
write normal natural language. The `hive` module recognizes a small,
line-leading marker grammar inside those messages — for example `!propose`,
`!support`, `!object`, and `!commit` — so it can audit a decision from the
transcript. The agents do not share hidden thoughts, memory, or a model
context; the host gives each turn an attributed view of the same message log.

### Who decides which agent speaks next?

No manager model does. In a hive episode, every active desk member gets a
deterministic bid. The highest bids take up to the policy's round width, with
ties broken by desk order. A bid
is the sum of each trace's salience for that member, minus the member's current
speaking threshold. A trace is more salient when it is recent, important, and
relevant to that member's configured topic affinity.

The bid also gives fixed bonuses when an agent was addressed, can break a
deadlock, or has been least heard, and applies a penalty to a member dominating
grounded contributions. Speaking raises that agent's threshold; silence lowers
the others'. That makes a recent speaker less likely to monopolize the floor.
The episode selects up to the policy width from eligible bids. Blind seats can
run concurrently; the default revealed width is one.

### Does a newly joined agent receive the entire transcript?

No. The host asks for a bounded projection. The default session window is 30
qualifying messages, and the paging walk inspects at most 2,048 raw log rows.
For a desk channel it keeps recent roots and each root's first reply; for a
thread it keeps that thread's root and direct replies. Every returned message
preserves its original author, so a peer's reply is never presented as the
viewer's own prior response.

The initialization also returns a separate team briefing and can include an
index of live threads plus host-supplied notes. It does not automatically
summarize a long history. The optional digest module produces a bounded account
of older desk-visible rows through a host `Digester` port; the host stores that
account. Pins can bring older source rows back into a turn without rewriting
the journal. A host can also provide its own search.

### How does an agent see messages added after it starts?

The host records the last accepted sequence number as a watermark. Before a
later turn, `prepare_delta` reads only qualifying messages between that
watermark and the new trigger, preserves their authorship, and returns them in
chronological order. If the gap cannot be read safely inside the scan bound,
the library asks the host to reinitialize instead of silently skipping history.

### Does tinyhivemind assign work or run agents?

No. The host owns the agent lifecycle, model calls, queueing, storage, and
authorization. The core mention fold resolves a direct addressee; the host can
use that decision to schedule one turn. The optional embedding layer can
recommend a primary and bounded specialist set; the `hive` module can
select a round inside a
bounded deliberation. The host decides whether to run those turns, what models
to use, what long-term memory or search to provide, and how to persist the
result. A workspace such as Buzz could host these mechanics, but it is a
separate system with its own routing and context policy.

## Use it

```sh
git submodule add https://github.com/tinyhumansai/tinyhivemind.git vendor/tinyhivemind
```

```toml
[dependencies]
tinyhivemind = { path = "vendor/tinyhivemind/crates/tinyhivemind" }
```

```sh
cargo run --release -p tinyhivemind-core --example bench -- --trace
```

That prints one deliberation episode turn by turn—the fastest way to see the mechanics.

## Read more

| | |
| --- | --- |
| [Quick start](https://github.com/tinyhumansai/tinyhivemind/wiki/Quick-start) | pin it, resolve a mention, read a deliberation |
| [Architecture](https://github.com/tinyhumansai/tinyhivemind/wiki/Architecture) | the crate boundaries and why they are split that way |
| [Threads](https://github.com/tinyhumansai/tinyhivemind/wiki/Threads) | thread-scoped projection, and finding your way back into a busy desk |
| [Cross-desk referral](https://github.com/tinyhumansai/tinyhivemind/wiki/Cross-desk-referral) | asking another channel a question, and getting the answer back |
| [Hive episodes](https://github.com/tinyhumansai/tinyhivemind/wiki/Hive-episodes) | salience, quorum, cross-inhibition, and the attention market |
| [Trace grammar](https://github.com/tinyhumansai/tinyhivemind/wiki/Trace-grammar) | what a marker deposits, and what real models get wrong |
| [Episode policy](https://github.com/tinyhumansai/tinyhivemind/wiki/Episode-policy) | every setting, and how to tune it to the size of a desk |
| [Benchmarks](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks) | the full report, including what it does not show |
| [Host integration](https://github.com/tinyhumansai/tinyhivemind/wiki/Host-integration) | the ports, and what your application owes the library |
| [Integration crates](https://github.com/tinyhumansai/tinyhivemind/wiki/Integration-crates) | semantic routing, TypeSafe, and the OpenHuman adapter |
| [Development](https://github.com/tinyhumansai/tinyhivemind/wiki/Development) | the build contract, testing, and how to contribute |
| [Glossary](https://github.com/tinyhumansai/tinyhivemind/wiki/Glossary) | every term, what it means here, and where it came from |
| [Further reading](https://github.com/tinyhumansai/tinyhivemind/wiki/Further-reading) | the swarm biology, the group-decision literature, the papers |
| [ROADMAP.md](ROADMAP.md) | the phase plan, and the two defects this work exists to fix |

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
