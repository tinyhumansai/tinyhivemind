# Documentation

This directory holds the working record: what was specified, in what order it
was built, and which decisions are closed. The narrative documentation, the
kind a reader wants before touching anything, lives in the
[wiki](https://github.com/tinyhumansai/tinyhivemind/wiki), which is checked out
as a submodule at `wiki/`. API reference lives in doc comments next to the
code, where it cannot drift.

## Layout

```text
docs/
├── README.md      # this index
├── testing.md     # the deterministic harness, opt-in live tests, coverage
├── specs/         # behavior and architecture specifications
├── plans/         # implementation plans derived from approved specs
└── adr/           # architecture decision records, numbered and immutable
```

- [`testing.md`](testing.md) covers the deterministic coordination harness, the
  opt-in OpenRouter tests, and the coverage commands.
- [`specs/`](specs/README.md) holds one file per feature, module, or subsystem,
  describing its behavior, public surface, invariants, and acceptance criteria.
- [`plans/`](plans/README.md) holds implementation-ordered, test-first steps for
  delivering an approved specification. Plans name exact files and verification
  commands, and are updated as the work progresses.
- `adr/` holds a dated record per significant decision. Use
  [`adr/0001-record-architecture-decisions.md`](adr/0001-record-architecture-decisions.md)
  as the template. An accepted ADR is not edited; it is superseded by a later
  one.

Complex modules also carry a module-level `README.md` inside `src/<module>/`
covering their design, public surface, and important constraints.

## What lives in the wiki instead

Anything a reader wants before they touch the code:

| page | what it covers |
| --- | --- |
| [Architecture](https://github.com/tinyhumansai/tinyhivemind/wiki/Architecture) | the three crates and why they are split that way |
| [Quick start](https://github.com/tinyhumansai/tinyhivemind/wiki/Quick-start) | pinning it, resolving a mention, reading a deliberation |
| [Desks and rosters](https://github.com/tinyhumansai/tinyhivemind/wiki/Desks-and-rosters) | membership, responder mode, conversation identity |
| [Mentions](https://github.com/tinyhumansai/tinyhivemind/wiki/Mentions) | the grammar, resolution, and expansion |
| [Transcript projection](https://github.com/tinyhumansai/tinyhivemind/wiki/Transcript-projection) | attribution, the paging walk, continuous sharing |
| [Threads](https://github.com/tinyhumansai/tinyhivemind/wiki/Threads) | thread-scoped projection and the desk's thread index |
| [Responder ladder](https://github.com/tinyhumansai/tinyhivemind/wiki/Responder-ladder) | the rungs, the selector, and mention dispatch |
| [Hive episodes](https://github.com/tinyhumansai/tinyhivemind/wiki/Hive-episodes) | stigmergy, salience decay, quorum, cross-inhibition, the attention market |
| [Episode policy](https://github.com/tinyhumansai/tinyhivemind/wiki/Episode-policy) | every setting, and how to tune it to a desk |
| [Benchmarks](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks) | the full report, including what it does not show |
| [Host integration](https://github.com/tinyhumansai/tinyhivemind/wiki/Host-integration) | the three ports and what a host owes the library |
| [Development](https://github.com/tinyhumansai/tinyhivemind/wiki/Development) | the build contract, testing, and contributing |
| [Glossary](https://github.com/tinyhumansai/tinyhivemind/wiki/Glossary) | every term, what it means here, and where it came from |
| [Further reading](https://github.com/tinyhumansai/tinyhivemind/wiki/Further-reading) | the swarm biology, the group-decision literature, the papers |
| [FAQ](https://github.com/tinyhumansai/tinyhivemind/wiki/FAQ) | the questions that keep coming up |

Edit those pages in `wiki/` and push the submodule, then commit the pointer
bump here. The wiki is a separate git repository
(`tinyhumansai/tinyhivemind.wiki`), so a change to it is not part of a pull
request against this one.

## Conventions

- Keep every Markdown file at 500 lines or fewer. When a topic outgrows that,
  split it into focused files and link them from the nearest `README.md`.
- Update documentation in the same commit as the behavior it describes.
- Prefer a concrete example over an abstract description.
- Link between documents rather than duplicating content; one fact lives in one
  place.
- Write a specification before a plan: the spec defines the outcome and
  constraints, while the plan defines the implementation sequence.
