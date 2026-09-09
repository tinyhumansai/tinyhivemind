# `digest` — one bounded account of a channel, behind a live tail

## The problem

A projection is bounded; a channel is not. `project_session` hands a turn the
newest N rows it may read, which is right for answering the room and wrong for
knowing it: a seat that joins on turn forty gets thirty rows and no idea what
the first four hundred settled. Every host reaction to that is worse than the
disease — a bigger window (which the long-context literature is least kind to),
a shared scratch file that is everyone's and therefore nobody's, or a seat that
re-reads the workspace before it can think.

## The shape

A channel carries one **account**: bounded text standing for every row older
than the live tail, rewritten as the room moves, always covering strictly more
than it did before. A turn is then

```text
account (≤ budget_chars)  +  the rows the account does not cover
```

Three rules keep it from becoming a second journal.

1. **The account is not a row.** It supersedes itself, and an append-only log
   that never condenses cannot hold a superseding record without growing
   without bound or being asked to forget. It is host state, exactly as
   `SharingState` is.
2. **The account folds only what every member may read.** `collect_digest_input`
   skips anything outside `Audience::Desk`, so one account serves every member
   — including one that has never spoken — and no fold can launder a private
   row into a shared summary.
3. **The rows survive.** Folding changes what a turn is *shown*, never what the
   log holds. A folded row keeps its sequence and stays reachable through
   `search_messages`, which is the escape hatch a lossy summary needs.

### Two triggers, and why the second is in characters

`fold_after` says the room has *moved*. `fold_after_chars` says its scrollback
has grown *expensive*, and on a desk writing derivations the second arrives long
before the first: one run solved Project Euler 1006 in 23 rows and 604k tokens,
which is a fold the row count would never have planned. Whichever binds first
wins, and `0` disables the size one.

The unit is characters, not tokens, and `DigestPolicy::from_token_budget` is how
a host writes its budget in the unit it thinks in. A tokenizer is a per-model
table, so a trigger that used one would fold the same channel at different
points on two machines and the plan would stop being a fold over its arguments —
and this crate may not take the dependency anyway. Four characters to the token
puts a converted budget within roughly 15% of its intent, which is the right
precision for deciding *when to spend one summarization call*.

The count itself is the host's to keep, and arrives as `ChannelHead`. Only the
host sees a row's size as it appends it; reading the log back on every turn to
decide whether to compact it would cost more than the compaction saves. Private
rows must not be counted — an account may not contain one, so one must not be
able to trigger a fold either.

### A pinned message does not scroll away

A pin is the room saying one message must stay reachable. The fold is the only
thing standing between a pinned row and the moment it leaves the window, so
`refold` takes the board and passes the pinned sequences at or below `through`
in `DigestRequest::pinned`. A digester is expected to carry what those rows
established into the new account in full.

Only sequences cross, never content: a pin to a private row could not leak one
even if a host offered it, and the rows themselves have already been filtered to
`Audience::Desk` before the digester sees them.

## The public surface

| item | kind | what it does |
| --- | --- | --- |
| `DigestPolicy` | value | `keep_live`, `fold_after`, `fold_after_chars`, `input_limit`, `budget_chars`, and `from_token_budget` |
| `ChannelHead` | value | where the channel stands: newest `sequence`, and `unfolded_chars` the account does not cover |
| `ChannelDigest` | value | the account: conversation, `through`, `covered`, `generation`, `text` |
| `plan_digest` | pure fold | what the next fold should cover, or `Current` |
| `collect_digest_input` | async | the desk-visible rows of one step, chronological |
| `Digester` | **port** | request in, text out — no host handle, no callback |
| `accept_digest` | pure fold | validate an answer against the account it replaces |
| `refold` | async | plan → collect → digest → accept, one call at most |
| `apply_digest` | pure fold | compose a turn's history from account + live rows |

## Operational constraints

- **A fold advances by at most `input_limit` per call.** A first fold over a
  long history is a run of bounded calls that catch up, not one enormous one,
  and no row is stepped over.
- **The row trigger is measured in sequence distance, not rows of this channel.**
  On a log carrying several channels that overestimates, so a busy neighbour
  makes a fold happen *earlier* than strictly necessary — one extra call, never
  a missed one.
- **A missing or failing digester is not an error.** `refold` returns
  `Unavailable`, the held account stands, and the live tail still carries every
  row the fold would have covered. Compaction is an optimization over a
  projection that is already correct without it, and it is treated like the
  `Selector` port for the same reason.
- **A rejected answer is not an error either.** Empty, over budget, or covering
  no more than what it replaces are `DigestRejection` values, so a host can log
  a bad digester without a failed turn.
- **A step with nothing desk-visible in it still advances the account.**
  Otherwise the same empty step is planned forever. No model call is made for
  it.
- **`DigestGap` is an error.** If the bounded scan cannot reach the range, the
  fold would have a hole in the middle, and a fold with a hole is worse than
  none.
