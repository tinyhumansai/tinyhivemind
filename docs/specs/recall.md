# Recall: selection, search, pinning, and a stated message budget

**Status:** Implemented — P14
**Owner:** tinyhivemind maintainers

## Problem

An agent's turn reads a bounded window of a shared desk — `SESSION_WINDOW`, 30
messages by default — and the log behind it is unbounded. Everything outside
the window is, from the turn's point of view, gone. The failure this produces
is not "the agent forgot"; it is worse than that:

- The decision the room settled two hundred messages ago is invisible, so it is
  re-litigated, and the second answer may contradict the first.
- The one message that mattered — a constraint, a number, a refutation — is
  indistinguishable from the two hundred that did not, because nothing marked
  it.
- A busy participant pushes everyone else's context out. A six-paragraph
  message costs five other messages of shared window, including the one that
  answered the question.

The tempting answer — enlarge the window — does not work: it moves the cliff
without removing it, and it costs every participant on every turn. The answer
here is the one a person uses on a large corpus: stop trying to hold it, and
make it *queryable*, with a small pinned working set that rides along
regardless. That is the same move a recursive language model makes over a long
context — the context becomes an environment to interrogate rather than a
prefix to swallow — applied to a shared transcript rather than a single prompt.

## Goals

1. One ranking, used by every picker in the workspace, so an agent search and a
   desk search cannot disagree about what "better match" means.
2. Pickers over the snapshots a turn already holds: agents, people, desks.
3. Search over the transcript itself — a desk, a thread, or the whole log —
   with literal queries and, optionally, regular expressions.
4. A pinboard: a bounded set of messages that appear in every turn's context
   for a conversation, folded out of the transcript rather than stored beside
   it.
5. A stated per-message budget, so an agent knows what it is spending.

## Non-goals

- **A second journal.** A pin is a fold over the log, exactly like a trace or a
  thread index. Nothing here is stored, so nothing here can drift from the
  transcript it describes.
- **An index.** No inverted index, no embedding, no background job. A search is
  a bounded backward walk over the same `SessionLog` port everything else uses,
  and it is honest about being bounded.
- **Ranking quality as a research result.** The ordering is a fixed-point
  integer fold chosen to be predictable and testable, not a retrieval model.
- **Enforcing brevity.** The budget is reported, never applied. This crate does
  not edit an authored message; a transcript that disagrees with what was said
  is worse than a long one.

## Proposed behavior

### Selection — `tinyhivemind_core::select`

`score(query, text) -> Option<TextMatch>` folds a query and a piece of text into
a tier and a fixed-point score. Tiers, worst to best: `Subsequence` (200),
`Substring` (400), `WordPrefix` (600), `Prefix` (800), `Exact` (1000). The score
is the tier base plus a density term, `100 * query_chars / text_chars`, so the
shorter of two candidates matching the same way wins. Tiers are 200 apart and
density contributes at most 100: density never promotes a weaker tier past a
stronger one.

`rank(query, candidates, limit) -> Vec<Hit>` scores each `Candidate`'s `label`
and `id` at full weight and its `detail` at half, reports the strongest field,
and orders by score, then earlier match, then shorter label, then candidate
order. Blank query, zero limit, or no match yields an empty list; an empty
picker box is a request to list, and listing is the caller's own snapshot to
iterate.

`Pattern` chooses what candidates are matched against: `Text` or, under the
`regex` feature, a borrowed compiled `Regex`. A regular-expression hit is read
onto the same tiers from the span the engine matched — whole text is `Exact`,
offset zero is `Prefix`, a word boundary is `WordPrefix`, anything else is
`Substring`, and a zero-width match is the weakest `Substring` there is — so
literal and expression hits rank in one comparable list. `regex_source("/…/")`
is the pure spelling that lets one input box carry either intent.

### Pickers — `tinyhivemind_core::find`

`agents`, `people` and `desks` rank the roster and desk snapshots the caller
already holds. Retired agents are never offered. A desk's description is
`detail`, so a desk named for the query always outranks one that merely
mentions it. Each has a `*_matching` form taking a `Pattern`.

### Transcript search — `tinyhivemind::search`

`search_messages(log, query) -> Result<Vec<MessageHit>>` walks the log
newest-first through `SessionLog`, bounded by `SEARCH_SCAN` (2048) raw rows,
and returns at most `SearchQuery::limit` hits ordered by score then newest
first. `SearchQuery` carries a `SearchPattern` (`Text` or `Regex`, a wire form
— compilation happens inside the search), an optional `scope`, an optional
`author_id`, an exclusive `before`, and a limit.

Scope semantics differ from projection on purpose: a desk-scoped search reads
the desk's **whole interior**, thread replies included. The projection is
narrow so a turn stays readable; the search exists precisely to reach the reply
buried three deep. A thread-scoped search reads that thread's root and direct
replies.

A hit carries the row's address, author, thread parent, and a
whitespace-collapsed `excerpt` of `EXCERPT_CHARS` (96) around the match, so a
caller can decide whether to go read the row without reading it first.

`search_threads(log, conversation, pattern, limit)` ranks a desk's threads by
their opening words, bounded by `THREAD_INDEX_SCAN` for the reason the thread
index is: which threads are live is a recency question.

Reaching a scan bound is a successful partial search over the newest rows, the
same contract `project_session` has. A `Regex` pattern without the `regex`
feature is `Error::RegexUnsupported`, and one that does not compile is
`Error::InvalidPattern` — never a silent literal fallback, which would return
confidently wrong results.

### Pinning — `tinyhivemind::pins`

A marker is recognised at the start of a line, ignoring leading whitespace, and
only outside a fenced code block — the hive trace grammar's rule, for the same
reason.

```text
!pin [^N] [#label] [free text]
!unpin ^N
```

`!pin` with no target pins its own carrier, which is the common case: an agent
marking the insight it just wrote. `!unpin` without a target yields no
directive at all — fail-closed, like an incomplete trace marker.

`fold_pins(rows, limit)` folds a chronological slice into a board. Later
markers win: pinning an already-pinned message updates it rather than
duplicating it, and an unpin removes it whoever pinned it. The board is
returned most recently pinned first and truncated to `limit` (`PIN_LIMIT`, 12),
dropping the least recently pinned — a full board means something has to come
off, and the oldest pin is the one the room stopped arguing about. Each pin
carries an `excerpt` when the fold saw the pinned row, and `None` when it fell
outside the scan; the sequence is still there, so a host can read that one row.

`read_pinboard(log, conversation, limit)` is that fold plus its bounded read.
A desk-scoped read folds the desk's whole interior, thread replies included: a
pin exists to lift a message out of the depth it is buried at.

`SessionContext` carries `pins`, and `initialize_session_with_context` reads
them, so a pinned message is in every turn's context for that conversation
whether or not anybody searched.

### The budget — `BrevityPolicy`

`TeamBriefing` carries a `BrevityPolicy { message_chars, window }`, default
600 characters against `SESSION_WINDOW`, and states it in `system_text()`
alongside the pin and search spellings. `overrun(content)` reports the
characters by which a message exceeds the budget. A host may nudge, may ask for
a shorter message, or may do nothing.

## Invariants

- Every score is fixed-point integer arithmetic; every payload derives `Eq` and
  every fold is reproducible across machines.
- Ranking is total and stable: the same snapshot and query always produce the
  same list, in the same order.
- No function here opens storage, a socket, or an index. Search and the
  pinboard read through the existing `SessionLog` port and nothing else.
- No new port, and no host type. The `regex` feature adds a pure dependency and
  is off by default.
- Every bound is a named constant: `SELECT_LIMIT`, `SEARCH_LIMIT`,
  `SEARCH_SCAN`, `EXCERPT_CHARS`, `PIN_LIMIT`, `PIN_SCAN`,
  `PIN_EXCERPT_CHARS`, `PIN_MARKER_CAP`.
- Wire forms are pinned by unit tests: `SearchQuery`, `MessageHit`, `Pin`,
  `PinDirective`, `TextMatch`, and the two new `SessionContext` and
  `TeamBriefing` fields, both `#[serde(default)]` so an older stored record
  still decodes.

## Acceptance criteria

- A query that names a candidate exactly ranks it above one that merely
  contains it, and above one that only mentions it in a description.
- A message pinned two thousand rows back appears in a turn's context with no
  search performed.
- An unpinned message does not, however it was pinned or by whom.
- A desk-scoped search finds a reply inside a thread; a thread-scoped search
  does not leave its thread.
- A regular-expression search and a literal search over the same rows return
  hits ordered by one comparable score.
- The four contract commands pass with and without `--all-features`, and
  `.github/scripts/assert-pure.sh` stays clean.

## Open questions

- **Who may unpin.** Today anyone may unpin anything; a host that wants
  narrower rules filters rows before folding. Whether the library should carry
  an authorship rule is unresolved, and deliberately left until a host needs it.
- **Ranking recency.** A search orders by score alone and lets the newest of
  two equal scores win. Whether an explicit recency term earns its complexity
  is unmeasured, and unmeasured knobs are how P9 ended up off by default.
