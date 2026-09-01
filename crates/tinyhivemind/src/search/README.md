# `search`

Bounded search over the shared transcript: messages, and the threads holding
them.

## Why it exists

A turn reads about thirty messages of a desk whose log is unbounded, so the
useful question is not "how do we fit more in" but "how does a turn reach the
rest". Search is that reach: the transcript becomes something a turn queries,
and only what matched comes back. [`../pins`](../pins) is the other half —
what must arrive without anybody asking.

## Public surface

| Item | What it is |
| --- | --- |
| `SearchPattern` | wire form of a pattern: `Text { query }` or `Regex { source }` |
| `SearchPattern::parse` | one input box: `/…/` is an expression, anything else is text |
| `SearchQuery` | pattern, optional scope, optional author, `before`, limit |
| `MessageHit` | address, author, parent, excerpt, score, tier |
| `ThreadHit` | a `ThreadLine` with the score that found it |
| `search_messages(log, query)` | the bounded backward walk |
| `search_threads(log, conversation, pattern, limit)` | thread openings, ranked |
| `SEARCH_LIMIT` / `SEARCH_SCAN` / `EXCERPT_CHARS` | 10 / 2048 / 96 |

Ranking is [`tinyhivemind_core::select`], one ordering shared with the agent
and desk pickers, so a literal query and an expression land in one comparable
list.

[`tinyhivemind_core::select`]: ../../../tinyhivemind-core/src/select

## Constraints worth knowing

- **A desk-scoped search reads the desk's interior.** Thread replies included,
  unlike the projection. The projection is narrow so a turn stays readable; the
  search exists precisely to reach the reply buried three deep in an old
  thread. A thread-scoped search stays inside its thread.
- **The scan bound is not an error.** Reaching `SEARCH_SCAN` is a successful
  partial search over the newest rows — the same contract `project_session`
  has. A host that needs older rows pages with `before`.
- **A regular expression never silently degrades.** Without the `regex`
  feature a `Regex` pattern is `Error::RegexUnsupported`; one that does not
  compile is `Error::InvalidPattern`. Falling back to a literal search for the
  expression source would return confidently wrong results.
- **Offsets are over the collapsed, lowercased line.** Excerpting saturates on
  every cut, so an unusual lowercasing lands the window a character or two off
  and can never panic or split a character.
- **An empty query reads nothing.** No pattern, no read: an empty picker box is
  not a request for the whole log.

## Where the result goes

Back to the host, or to an agent as a tool result. Nothing here writes to the
log, and nothing here is stored.
