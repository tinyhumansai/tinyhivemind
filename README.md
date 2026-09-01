<img src="https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/hero.png?raw=true" />

<h1 align="center">tinyhivemind</h1>

<p align="center"><strong>Hive mind mechanics for agents. A step closer towards AGI</strong></p>

<p align="center">
Quorum sensing, cross-inhibition, stigmergy, pheromone decay and response
thresholds, implemented as integer folds over a transcript your application
already owns. Written in Rust. No storage, no HTTP, no runtime.
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
     └───── one message, one turn ◀── Speak { turn } ┤
                                                     │
      Converged · Deadlocked · Exhausted · Idle ◀────┘
```

Nothing in the box on the right opens a file, a socket or a database. It reads
what you hand it and returns what should happen next.

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

The floor under the bars is the trace's standing importance, which is why a
proposal nobody has touched for eighty messages still outranks a fresh
question. Recency is the term that moves.

**[Quorum sensing](https://en.wikipedia.org/wiki/Quorum_sensing).** An option
carries when some number of distinct participants have grounded support for it
inside a window. Not a majority of anything, not a score to beat. The count is
local, order independent and idempotent, so an agent that catches up late folds
to exactly the same standing as one that watched live. This is how
[honeybee swarms](https://en.wikipedia.org/wiki/Swarming_%28honey_bee%29)
settle a nest site.

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

**Who does `@this` mean?** A mention grammar resolved against the live roster
and desks, where only a direct agent mention can start a turn.

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

## An episode ends for a reason you can name

Every step walks the same ladder in the same order, and the first rung that
answers is the answer:

```text
  step(state, transcript, roster, desks, policy)
    │
    ├─ budget spent? ─────────────────────────▶ Exhausted { spent }
    ├─ quorum, and phase = Commit? ───────────▶ Converged { topic, .. }
    ├─ quorum, and phase = Deliberate? ───────▶ Speak { the commit turn }
    │                                           and the phase flips, once
    ├─ two topics carry, nobody to break it ──▶ Deadlocked { topics }
    │
    └─ highest bid clears its threshold? ─────▶ Speak { turn }
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

## It measurably beats one agent answering alone

Five agents choosing between four options, 5000 seeded rooms, on one core:

| arm | what it is | turns | correct |
| --- | --- | --- | --- |
| `ladder` | one responder answers alone, which is how most systems work today | 1.00 | 57.6% |
| `vote` | independent answers, plurality, matched budget | 15.00 | 78.5% |
| `hive+` | a tuned deliberation episode | 6.75 | **82.1%** |

The middle row is [self-consistency](https://arxiv.org/abs/2203.11171), the
control most multi-agent claims are missing. A room that could not beat an
independent vote at the same budget would not be worth its budget. This one
does, at every desk size from three to eight, while spending about half the
turns.

The
[benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the rest: the two bounds on the quorum threshold, why the turn budget has
to scale with the desk, what happens to accuracy without a blind opening round,
what five live models did to the grammar when nobody was watching, and an
honest section on what none of it shows.

## Not an agent council

A council is a conversation with roles: a manager or a round-robin picks the
next speaker, and it stops on a round cap or when the manager says so.

| | agent council | tinyhivemind episode |
| --- | --- | --- |
| who speaks next | a manager model, or round-robin | argmax over per-member bids |
| what agreement is | inferred from the replies | an explicit supporter set |
| what disagreement is | a message saying "I disagree" | an objection that removes an advocate |
| how it ends | round cap, or the manager stops | quorum, deadlock, exhaustion or idle |
| cost per round | one turn per member | one turn, total |
| replay | re-run and hope | byte-identical from the same transcript |

Councils are better at open-ended writing, at work that genuinely decomposes,
and at running on any model with no grammar to learn. Take one when the
deliverable is prose. Take this when the deliverable is a decision somebody
will ask you to justify later.
[The full comparison](https://github.com/tinyhumansai/tinyhivemind/wiki/Agent-councils)
is honest about both sides.

## Three rules it will not break

**The host owns storage.** No database, no file, no socket. Your log stays
yours and is lent through one port.

**No host types, ever.** Nothing here names a type from your application.

**One message, one turn.** `@everyone` is a list, not a broadcast, and no type
here can carry two authorized speakers.

They are enforced by the shape of the crates, not by discipline:

```text
   your application
   ┌──────────────────────────────────────────────────────────┐
   │  the session log      model calls        the turn queue   │
   └────────┬───────────────────┬───────────────────┬─────────┘
        SessionLog          Selector        MentionTurnQueue    ports you
   ┌────────▼───────────────────▼───────────────────▼─────────┐  implement
   │  tinyhivemind        the paging walk, the responder      │
   │                      ladder, the mention-dispatch edge   │
   ├──────────────────────────────────────────────────────────┤
   │  tinyhivemind-core   desks · roster · mention grammar ·  │
   │                      projection — arguments in, value    │
   │                      out, no async, no host types        │
   └──────────────────────────────────────────────────────────┘

   ┌──────────────────────────────────────────────────────────┐
   │  tinyhivemind-hive   traces · salience · quorum ·        │
   │  opt-in, and pure    cross-inhibition · attention        │
   │  enough to define    market · the episode machine        │
   │  no port of its own  — it waits through the ports above  │
   └──────────────────────────────────────────────────────────┘
```

Anything answerable from its arguments lives in a pure crate; anything that has
to wait lives behind one of the three ports. CI asserts the split rather than
trusting it — the pure crates cannot take on a runtime, a transport, an HTTP
client, a web framework or a database driver without failing the build.

## Frequently asked questions

### Does tinyhivemind create a new language or literally share agents' minds?

No. It is a Rust library, not a programming language or a model. Agents still
write normal natural language. The optional hive crate recognizes a small,
line-leading marker grammar inside those messages — for example `!propose`,
`!support`, `!object`, and `!commit` — so it can audit a decision from the
transcript. The agents do not share hidden thoughts, memory, or a model
context; the host gives each turn an attributed view of the same message log.

### Who decides which agent speaks next?

No manager model does. In a hive episode, every active desk member gets a
deterministic bid and the highest bid wins; a tie breaks by desk order. A bid
is the sum of each trace's salience for that member, minus the member's current
speaking threshold. A trace is more salient when it is recent, important, and
relevant to that member's configured topic affinity.

The bid also gives fixed bonuses when an agent was addressed, can break a
deadlock, or has been least heard, and applies a penalty to a member dominating
grounded contributions. Speaking raises that agent's threshold; silence lowers
the others'. That makes a recent speaker less likely to monopolize the floor.
Only one bid can win, so one step can authorize only one turn.

### Does a newly joined agent receive the entire transcript?

No. The host asks for a bounded projection. The default session window is 30
qualifying messages, and the paging walk inspects at most 2,048 raw log rows.
For a desk channel it keeps recent roots and each root's first reply; for a
thread it keeps that thread's root and direct replies. Every returned message
preserves its original author, so a peer's reply is never presented as the
viewer's own prior response.

The initialization also returns a separate team briefing and can include an
index of live threads plus host-supplied notes. It does not automatically
summarize a 100,000-token history. A host that needs a durable summary or
retrieval of older material owns that policy and data, then supplies it as
context or exposes it through its own tools.

### How does an agent see messages added after it starts?

The host records the last accepted sequence number as a watermark. Before a
later turn, `prepare_delta` reads only qualifying messages between that
watermark and the new trigger, preserves their authorship, and returns them in
chronological order. If the gap cannot be read safely inside the scan bound,
the library asks the host to reinitialize instead of silently skipping history.

### Does tinyhivemind assign work or run agents?

No. The host owns the agent lifecycle, model calls, queueing, storage, and
authorization. The normal runtime can resolve a direct mention and produce at
most one turn request; the optional hive crate can select one next speaker for
a bounded deliberation. The host decides whether to run that turn, what model
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
cargo run --release -p tinyhivemind-hive --example bench -- --trace
```

That prints one deliberation episode turn by turn, which is the fastest way to
see what the thing actually does.

## Read more

| | |
| --- | --- |
| [Quick start](https://github.com/tinyhumansai/tinyhivemind/wiki/Quick-start) | pin it, resolve a mention, read a deliberation |
| [Architecture](https://github.com/tinyhumansai/tinyhivemind/wiki/Architecture) | the three crates and why they are split that way |
| [Threads](https://github.com/tinyhumansai/tinyhivemind/wiki/Threads) | thread-scoped projection, and finding your way back into a busy desk |
| [Hive episodes](https://github.com/tinyhumansai/tinyhivemind/wiki/Hive-episodes) | salience, quorum, cross-inhibition, and the attention market |
| [Trace grammar](https://github.com/tinyhumansai/tinyhivemind/wiki/Trace-grammar) | what a marker deposits, and what real models get wrong |
| [Episode policy](https://github.com/tinyhumansai/tinyhivemind/wiki/Episode-policy) | every setting, and how to tune it to the size of a desk |
| [Benchmarks](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks) | the full report, including what it does not show |
| [Host integration](https://github.com/tinyhumansai/tinyhivemind/wiki/Host-integration) | the three ports, and what your application owes the library |
| [Agent councils](https://github.com/tinyhumansai/tinyhivemind/wiki/Agent-councils) | how this differs from a council or crew, and what each does better |
| [Development](https://github.com/tinyhumansai/tinyhivemind/wiki/Development) | the build contract, testing, and how to contribute |
| [Glossary](https://github.com/tinyhumansai/tinyhivemind/wiki/Glossary) | every term, what it means here, and where it came from |
| [Further reading](https://github.com/tinyhumansai/tinyhivemind/wiki/Further-reading) | the swarm biology, the group-decision literature, the papers |
| [ROADMAP.md](ROADMAP.md) | the phase plan, and the two defects this work exists to fix |

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
