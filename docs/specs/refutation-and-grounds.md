# Refutation and grounds

**Status:** Implemented, and **off by default**
**Owner:** tinyhivemind maintainers

> **Outcome.** The acceptance criterion below says the benchmark arm must be
> able to lose. It lost, on every configuration tried, and both knobs are
> therefore `false`/`None` in `QuorumPolicy::DEFAULT` and are not taught in the
> live protocol prompt. See
> [`../experiments/2026-09-01-refutation-and-grounds.md`](../experiments/2026-09-01-refutation-and-grounds.md)
> for the numbers and for what the benchmark does not test.

## Problem

A room can register that a member disagrees with another member. It cannot
register that a *fact* kills a *hypothesis*.

`!object >N ^M` names a message and removes its author from the supporter set of
every topic that message advocated. Killing a hypothesis therefore costs one
turn per advocate, and each new supporter costs one more turn. In the
[live hidden-profile run](../experiments/2026-09-01-live-hidden-profile.md) a
five-member room at a budget of fifteen ran out of budget before it ran out of
advocates: scout's refutation — the retry path shipped disabled, zero retries
fired — sat at a citable sequence in every failed room and changed nothing.

Separately, `Trace::grounded()` is `!cites.is_empty()`, so a `!support` citing
another `!support` counts exactly as much as one citing a measurement. That is
the information-cascade condition with a citation on it: agents observe actions,
not the evidence behind them, and pointing at the action does not change that.

Both are properties of the shared medium, not of any model. They are the same
class of defect as the two P4 and P5 fixed — the fold is lossy in a way no
reader can detect.

## Goals

- Let one grounded move remove a hypothesis from contention for the whole room,
  in one turn, without deleting anything or silencing anyone.
- Distinguish a support whose grounds reach a stated fact from one whose grounds
  reach only another opinion, and let policy require the first.
- Make an objection cost something to misuse.
- Keep every addition a pure, order-independent, idempotent fold over traces the
  crate already reads, with fixed-point arithmetic and `Eq` payloads.
- Score the result against the matched-budget vote, and report a negative result
  as one.

## Non-goals

- **Judging a citation.** The library counts distinct grounded refuters and
  resolves a citation chain's *kind*. It does not read the cited text and decide
  whether it supports the claim. A room can refute a true hypothesis.
- **Replacing `!object`.** Cross-inhibition stays exactly as specified in
  [`hive-mind.md`](hive-mind.md); the two moves do different work.
- **Weighing support by intensity.** A waggle dance carries the scout's estimate
  in its length; a `!support` carries `importance(Support)` whether its author is
  certain or indifferent. That is a real gap, and it is not this document's.
- **A port, a journal, or a model call.** As with all of P8.

## Proposed behavior

### The grammar

```text
!refute #topic ^N [^M ...] [free text]
```

`TraceKind::Refute` joins `Propose`, `Support`, `Object`, `Evidence`,
`Question`, `Commit`. It is recognized only at line start outside fenced code,
counts toward `TRACE_CAP`, and preserves its UTF-8 byte offset, exactly as every
existing marker does.

It **requires both a topic and at least one citation**. A line with a marker but
no `#topic`, or with a topic but no `^N`, yields no trace — the same outcome as
any other unparsable marker, and the same fail-closed rule the mention grammar
already uses. Consequently `Trace::grounded()` is true for every `Refute` that
parses, and a refutation cannot be made without pointing at something.

`importance(Refute)` is `850`, between `Object` at `800` and `Propose` at `900`.

### Standings

```rust
pub struct QuorumPolicy {
    pub threshold: u32,
    pub window: u32,
    pub require_grounded: bool,
    pub refutation_cap: u32,
    pub require_evidential: bool,
}

pub struct TopicStanding {
    pub topic: TopicId,
    pub supporters: Vec<String>,
    pub silenced: Vec<String>,
    pub refuted_by: Vec<String>,
    pub support: i64,
}
```

`refuted_by` holds the distinct agent ids of members who deposited a `Refute`
naming this topic within the window, sorted, deduplicated, and folded on the
same `(sequence, offset)` key as everything else so a late-joining participant
folds to the same value as one that watched live. A member cannot refute a topic
it also supports in window; the refutation stands and the support is dropped,
because the later, more specific move is the one the author meant.

`TopicStanding::carried` returns `false` when
`refuted_by.len() >= refutation_cap as usize`, whatever the support says.
`refutation_cap` is `Option<u32>`. `None` records refutations in `refuted_by`
and caps nothing, and is the default; `Some(0)` is `Error::ZeroRefutationCap`.
A room that wants the mechanism sets `Some(2)` — one member's assertion should
not kill a hypothesis, and two distinct grounded refuters should.

Nothing is removed. A capped topic keeps its supporters, its `support` sum and
its place in `standings`, so `consensus` still sees it, the transcript records
that the room considered it, and a reader can audit the refutation back to the
message it cites.

### Evidential grounding

A `Support`'s citation chain is resolved transitively through the traces in
window: follow each cited sequence to the trace at that sequence and recur,
stopping at a trace with no citations, at a `TraceKind::Evidence`, or on
revisiting a sequence already seen. The support is **evidentially grounded** if
any chain reaches an `Evidence` trace, and **socially grounded** otherwise —
including when its chain leaves the window, which is not followed.

Under `require_evidential`, a socially grounded support contributes to neither
`supporters` nor `support`, exactly as an ungrounded one does under
`require_grounded`. `require_evidential` implies `require_grounded`.

`require_evidential` defaults to `false`. It is strictly narrowing, it can
starve a room that has deposited no evidence, and whether it pays is empirical.

### Grounded objections

Under `require_evidential`, an `Object` silences an advocate only if its author
has deposited an `Evidence` trace within the window. The bee stop signal is
delivered by a scout who inspected the rival site; the one live `!object`
observed in the experiment fired against the *correct* option on an argument its
author had put no evidence behind.

This rides on `require_evidential` rather than on its own flag because it is the
same claim — grounds are weighed — applied to the negative move, and a policy
that weighed support but not objection would be inconsistent in an
exploitable direction.

## Invariants

- `standings` stays commutative and idempotent on `(sequence, offset)`.
  Evaluation order within one fold is: support, then cross-inhibition, then
  refutation. Refutation last, because a silenced supporter must not change
  whether a topic is capped.
- Chain resolution terminates on any input. The visited set is over sequences,
  so a cycle is bounded by the number of traces in window.
- Chain resolution never reads outside `window`. A member's standing must not
  depend on how far back it happened to page.
- Every new payload field derives `Eq` and pins its serde form in a unit test.
- All arithmetic stays fixed-point integer.

## Acceptance criteria

1. `!refute` with a topic and a citation deposits a trace; without either, none.
2. `refutation_cap` distinct refuters make `carried` false at any support level,
   and `refutation_cap == 0` is a typed error.
3. A permutation of the trace list folds to an equal `Vec<TopicStanding>`.
4. Under `require_evidential`, a support chain reaching `Evidence` counts and one
   reaching only `Support`/`Propose` does not; a cyclic chain terminates; a chain
   leaving the window reads as social.
5. Under `require_evidential`, an `Object` from an author with no in-window
   `Evidence` silences nobody.
6. Every new error variant has a test that produces it.
7. **The benchmark arm can lose.** The mechanism is scored against `vote` at a
   matched budget on the simulation, and against the sequential-`!object` arm on
   the same seeds. If it does not beat the arm without it, the result is written
   up in [`../experiments/`](../experiments) as a negative result and the
   mechanism does not go on to the live protocol prompt. **This is what
   happened.** `hive+ref` scores 75.0% against `hive+`'s 82.1% and the vote's
   78.5%; `hive+ev` scores 55.9%; and no policy with either knob on reaches the
   top twelve of an 864-point grid search.

## Open questions

- ~~Should `refutation_cap` scale with desk size?~~ Answered: the sweep tried
  `Some(2)` and `Some(3)` across every other dimension and neither reached the
  top twelve. Scaling a knob that loses at both settings is not the question.
- Should a `Refute` require the refuter to have deposited the evidence it
  cites? The experiment's diagnosis is that a refutation's blast radius is the
  whole room, so bounding who may fire one is the obvious next thing to try. It
  is the same shape as the grounded-objection rule already specified here.
- Should `require_evidential` *widen* rather than narrow — weight an
  evidentially grounded support above a socially grounded one, instead of
  discarding the latter? Weighting cannot starve a room, and starvation is
  where 202 of the arm's 500 episodes went.
- The simulated task has no hidden profile, which is the structure the whole
  mechanism was designed for. A simulated hidden-profile arm is the open item
  that would make this a real test rather than a partial one.

## Related

- [ADR 0003](../adr/0003-refutation-links-evidence-to-a-topic.md),
  [ADR 0004](../adr/0004-grounds-are-weighed-by-evidential-depth.md).
- [`hive-mind.md`](hive-mind.md), which this extends.
- [`../research/biology.md`](../research/biology.md).
