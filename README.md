<h1 align="center">tinyhivemind</h1>

<p align="center"><strong>Group chats for agents.</strong></p>

<p align="center">
A shared transcript several agents read and write, and the mechanism that gets
the right one to answer. Written in Rust, owns no storage, serves no HTTP,
picks no runtime.
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
> This is one of my best works so far and one of the most important libraries that I have worked on: tinyhivemind is a very crucial component, taking inspiration and learnings from my experience building harnesses, coordinating with agents, and building agents that solve a large, complex amount of problems.
>
> The reason this repository was built was because such a crucial component, had to be well-defined, researched, coded, tested, and simulated thoroughly before it got shipped into any software module. I'm excited to share this repo as an open-source GNU Rust library, and I hope this contributes towards hivemind agents.
>
> If you like this work, give me a follow over at https://github.com/senamakel/

## One agent answering is easy. A room is not.

Put five agents in one channel and the obvious problems show up immediately.
They read each other's replies as their own words. They miss what a peer said
between their own two turns. A single `@everyone` wakes all of them at once.
Nobody can tell whether the room agreed on anything, or just stopped.

tinyhivemind is the layer that fixes those. It answers four questions and holds
no state doing it.

**Who is here?** A roster of agents, and the people signed in alongside them.

**What is a desk, and who is on it?** A declared group chat merged with the
operator's runtime additions, retirements and ordering.

**Who does `@this` mean?** A mention grammar, resolved against the live roster
and desks, where only a direct agent mention can start a turn.

**What does one participant see?** An attributed projection of a multi-speaker
transcript into one viewer's history, so agent B never reads agent A's words as
its own.

## And then it lets the room decide

`tinyhivemind-hive` is the opt-in part. Agents leave typed markers in the
transcript, support accumulates, a grounded objection silences an advocate, and
the episode ends as converged, deadlocked, exhausted, or idle. Always for a
reason you can name afterwards.

```text
!propose #stage Stage the rollout across three regions.
!support #stage ^1 Staging bounds the blast radius if the migration is wrong.
!object  >3      The regions are not independent, so this bounds nothing.
!commit  #stage
```

Deliberation with an audit trail, in about 2.3 microseconds of library time per
step.

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
type in this library that can carry two authorized speakers.

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
| [Hive episodes](https://github.com/tinyhumansai/tinyhivemind/wiki/Hive-episodes) | traces, salience, quorum, and the attention market |
| [Benchmarks](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks) | the full report, including what it does not show |
| [Host integration](https://github.com/tinyhumansai/tinyhivemind/wiki/Host-integration) | the three ports, and what your application owes the library |
| [Development](https://github.com/tinyhumansai/tinyhivemind/wiki/Development) | the build contract, testing, and how to contribute |
| [ROADMAP.md](ROADMAP.md) | the phase plan, and the two defects this work exists to fix |

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
