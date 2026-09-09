# Folding by size: an account a joining seat can always be given

**Status:** Draft — extends `tinyhivemind::digest`, which is implemented
**Owner:** tinyhivemind maintainers
**Reading:** [`thoughts-and-channels.md`](thoughts-and-channels.md)
**Evidence:** [`../experiments/2026-09-08-pe1006-tool-room.md`](../experiments/2026-09-08-pe1006-tool-room.md),
[`../experiments/2026-09-09-desk-lessons.md`](../experiments/2026-09-09-desk-lessons.md)

## Problem

The standing account is built and has never fired. `DigestPolicy` triggers a
fold on a **row count** — `head > keep_live + fold_after` — and run 28 closed at
23 rows against a threshold of 32, so criterion 3 of
[`thoughts-and-channels.md`](thoughts-and-channels.md) is still open and the
prompt a seat reads has never contained a `room account:` line in a live run.

The row count is also the wrong unit, which the spec already says as an open
question: *twenty rows of one-line acknowledgements and twenty rows of
derivations are the same trigger and very different costs.* On PE 1006 they are
not close. Run 28's seats posted derivations — 603,835 tokens across 12 turns —
and a desk of that shape blows a context budget long before it reaches 32 rows.
The thing a host actually wants to bound is how much of a seat's context window
the room's scrollback is allowed to occupy, and the policy cannot express that.

The consequence for a joining seat is the one that matters. A seat that has
never spoken is handed the live window and, today, an account only if enough
*rows* happened to accumulate. What it should be handed is an account whenever
the room is *large*, which is exactly when it needs one.

## Goals

1. A fold triggers on the size of what would otherwise be sent, not only on how
   many rows it is spread across.
2. A host states that size in the unit it budgets in, and the trigger stays a
   deterministic fold over data the caller holds.
3. A seat joining a large room is always handed an account of it.
4. Nothing about the existing row trigger changes for a host that does not set
   the new one.

## Non-goals

- **A tokenizer in the library.** `tinyhivemind` may not take a transport or a
  model dependency, and a tokenizer is a per-model table that would make the
  fold non-reproducible across model choices. See below for what is used
  instead.
- **Folding the live tail.** `keep_live` rows are delivered verbatim, always.
  A room whose live tail alone exceeds the budget is a `keep_live` set too
  large, and that is the host's error to make.
- **Folding a private row.** Unchanged: an account folds `Audience::Desk` and
  nothing else.
- **A second journal.** Unchanged: the account supersedes itself and is host
  state.

## Proposed behavior

### 1. A size trigger beside the row trigger

`DigestPolicy` gains one field:

```text
/// Characters of foldable, desk-visible content that trigger a fold on their
/// own, whichever threshold binds first. `0` disables the size trigger.
pub fold_after_chars: usize,
```

`plan_digest` returns `DigestPlan::Fold` when **either** threshold is crossed.
`input_limit` still bounds one call, so a first fold over a very large history
is a run of bounded calls rather than one enormous one, and no row is stepped
over.

### 2. Characters, and why not tokens

The unit is characters of message content, counted over the same desk-visible
rows the fold would cover. It is a proxy for tokens, deliberately:

- **It is reproducible.** Every payload in this workspace derives `Eq` and
  every fold is a fold over arguments. A tokenizer makes the trigger depend on
  which model the host happens to be pointed at, and a policy that answers
  differently on two machines is not a fold.
- **It costs no dependency.** A tokenizer is a table, a download, or a
  transport; `tinyhivemind` may take none of the three.
- **The ratio is stable enough for a threshold.** English prose and code run
  roughly 3.5–4.5 characters per token. A host budgeting 50,000 tokens sets
  `fold_after_chars: 200_000` and is within about ±15% of its intent, which is
  the right precision for *when to spend one summarization call*.

The documentation states the proxy plainly, and `DigestPolicy` carries a
`from_token_budget(tokens: usize)` constructor that applies the documented
ratio, so a host writes its budget in the unit it thinks in and the conversion
happens in one place.

### 3. A joining seat is always given the account

Once the size trigger exists, "a large room has an account" is true by
construction, and the remaining work is that the host actually delivers it. The
`desk` example already composes the account into every prompt regardless of
whether the seat has spoken before; this spec pins that as a requirement rather
than an example detail, and adds the case that has never been tested: a seat
whose first turn is on a room of hundreds of rows.

## Invariants and constraints

- Both triggers are thresholds on foldable content, never on the live tail.
- A fold still advances by at most `input_limit` rows and must cover strictly
  more than the account it replaces, or it is `DigestRejection::Regressed`.
- The account still folds only `Audience::Desk` rows.
- A digester that is missing or failing is `DigestOutcome::Unavailable`, not an
  error. Compaction is an optimization over a projection that is already
  correct without it.
- `fold_after_chars: 0` is the disabled state, and `DigestPolicy::DEFAULT`
  keeps today's behavior plus a size ceiling generous enough that no existing
  host changes shape without asking.

## Acceptance criteria

1. A channel of 24 rows whose content exceeds `fold_after_chars` folds, where
   today it does not, and the resulting prompt contains an account.
2. A channel of 40 short rows still folds on the row trigger with
   `fold_after_chars: 0`.
3. `from_token_budget(50_000)` yields a policy that folds a room of roughly
   50,000 tokens of desk-visible content and not one appreciably smaller.
4. A seat taking its first turn on a folded room is handed the account, proven
   by a host-level test rather than by inspection.
5. No account contains a non-desk row, at any size. Carried from
   [`thoughts-and-channels.md`](thoughts-and-channels.md) criterion 4.
6. A live run on PE 1006 folds at least once and the account appears in a
   prompt. This is the criterion that has never been met.
7. The four contract commands and `.github/scripts/assert-pure.sh` pass.

## Open questions

- **What does a wrong account cost?** Carried unresolved from
  [`thoughts-and-channels.md`](thoughts-and-channels.md): a lossy summary that
  drops the one fact the next turn needed is a defect with no error message.
  The mitigations are that the rows survive and `desk_read` reaches them, and
  nothing yet measures how often a seat goes back for them.
- **Should the trigger count the prompt or the transcript?** It counts message
  content. A prompt also carries the brief, the notebook and the house rules,
  and those are host-composed and not the library's to measure.
- **Does a smaller live window now beat a larger one?** Still unrun, and the
  size trigger is what makes the trade cheap to test.
