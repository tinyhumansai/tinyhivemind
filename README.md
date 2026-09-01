<h1 align="center">tinyhivemind</h1>

<p align="center"><strong>Hive mind mechanics for agents.</strong></p>

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
> This is one of my best works so far and one of the most important libraries that I have worked on: tinyhivemind takes inspiration and learnings from my experience building harnesses, coordinating with agents, and building agents that can solve large, complex problems.
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

## The mechanics

**Stigmergy.** Work leaves a trace in a shared medium, and the trace is the
stimulus for the next piece of work. No agent addresses another and nothing
dispatches anything. The transcript is the medium, and a marker line is a
deposit in it.

```text
!propose #stage Stage the rollout across three regions.
!support #stage ^1 Staging bounds the blast radius if the migration is wrong.
!object  >3      The regions are not independent, so this bounds nothing.
!commit  #stage
```

**Pheromone decay.** A trace's pull on the room's attention decays
exponentially with distance in the transcript. Without it, whoever spoke first
holds the floor forever, which is the failure ant trails avoid only because
pheromone evaporates.

**Quorum sensing.** An option carries when some number of distinct participants
have grounded support for it inside a window. Not a majority of anything, not a
score to beat. The count is local, order independent and idempotent, so an
agent that catches up late folds to exactly the same standing as one that
watched live. This is how honeybee swarms settle a nest site.

**Cross-inhibition.** An objection names a *message*, and removes that
message's author from the supporter set of whatever they were advocating. It
does not debit the option. Subtracting from a score cannot break a tie between
two equally supported options; silencing an advocate can, and that asymmetry is
the entire reason it is shaped this way. Honeybees do this too, with stop
signals.

**Response thresholds.** Every member computes an urge from the salience field
and its own affinity, and whoever bids highest takes the floor. A member whose
urge never clears its threshold does not bid at all. This is the
response-threshold model of division of labour, and it is also Pandemonium's
decision demon, which is the same idea arrived at from the AI side.

Every one of those is fixed-point integer arithmetic. Every payload derives
`Eq`, and every episode replays byte for byte from the same transcript.

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

## An episode ends for a reason you can name

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

The middle row is the control most multi-agent claims are missing. A room that
could not beat an independent vote at the same budget would not be worth its
budget. This one does, at every desk size from three to eight, while spending
about half the turns.

The [benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the rest: the two bounds on the quorum threshold, why the turn budget has
to scale with the desk, what happens to accuracy without a blind opening round,
what five live models did to the grammar when nobody was watching, and an
honest section on what none of it shows.

## Three rules it will not break

**The host owns storage.** No database, no file, no socket, no second journal.
Your log stays yours and gets lent through one port.

**No host types, ever.** Nothing here names a type from a consuming
application, and no callback ever crosses back into it.

**One message, one turn.** `@everyone` is a list, not a broadcast. There is no
type in this library that can carry two authorized speakers. The one thing
fan-out genuinely buys is independence, and that is bought here as a visibility
filter on the projection, for the cost of a flag rather than a scheduler.

## Use it

```sh
git submodule add https://github.com/tinyhumansai/tinyhivemind.git vendor/tinyhivemind
```

```toml
[dependencies]
tinyhivemind = { path = "vendor/tinyhivemind/crates/tinyhivemind" }
```

Nothing is published to crates.io and there are no releases. The pinned commit
is the version.

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
| [Development](https://github.com/tinyhumansai/tinyhivemind/wiki/Development) | the build contract, testing, and how to contribute |
| [ROADMAP.md](ROADMAP.md) | the phase plan, and the two defects this work exists to fix |

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
