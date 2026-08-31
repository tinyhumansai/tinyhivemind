# Hive mind

**Status:** Accepted
**Owner:** tinyteams maintainers

## Problem

A message on a shared desk selects exactly one responder, that agent replies,
and the interaction ends. A room of agents therefore cannot deliberate: there is
no way to put two proposals side by side, accumulate support for them, register
a grounded objection, decide that the room has settled, or notice that it has
deadlocked. Hosts that want group deliberation today have to build the whole
protocol themselves, and the observed failure modes of doing so are well
documented — unbounded fan-out, restating a settled point forever, and never
recognising a termination condition.

## Goals

- Decide, from a transcript the caller already holds, which single participant
  should take the next turn in a bounded deliberation episode.
- Accumulate support for competing proposals, and let a grounded objection
  silence an advocate so a tie between two good options can break.
- Terminate for an explicit, auditable reason: converged, deadlocked, exhausted,
  or nobody wanted the floor.
- Restore first-round independence without concurrency.
- Add no port, no journal, no async, and no host obligation beyond P4–P7.

## Non-goals

- **Fan-out.** An episode is a sequence of single turns. See
  [`../adr/0002-hive-episodes-are-sequential.md`](../adr/0002-hive-episodes-are-sequential.md).
- **Making answers better.** Nothing here is shown to improve accuracy, and
  almost every positive multi-agent result in the literature is confounded by
  compute. This is a protocol for bounded deliberation, not a quality claim.
- **Persistence.** `EpisodeState` is returned, never stored; the caller commits
  it under its own serialization, as with `sharing::prepare_delta`.
- **Model calls.** Deciding who speaks is pure. Whether a host consults its
  `Selector` port first is the host's business.

## Proposed behavior

### Traces — the shared medium

Coordination is stigmergic: a message deposits a typed trace, and the traces are
the stimulus for the next turn. No agent addresses another.

```rust
pub enum TraceKind { Propose, Support, Object, Evidence, Question, Commit }
pub struct TopicId(String);

pub struct Trace {
    pub sequence: Sequence,
    pub author: SessionAuthor,
    pub kind: TraceKind,
    pub topic: Option<TopicId>,
    pub target: Option<Sequence>,
    pub cites: Vec<Sequence>,
    pub text: String,
    pub offset: usize,
}
```

`Trace::grounded()` is `!cites.is_empty()`. Grounds are the cited sequences
rather than a flag so that a decision can be audited back to the messages that
carried it, the way a reflection in a memory stream cites its source nodes.

`resolve(body, supplied, author, sequence)` mirrors `mention::resolve` exactly:
either extract traces from authored text, or revalidate an authoritative
supplied list. Authored spans and UTF-8 byte offsets are preserved, inline and
fenced code spans are masked, and the grammar is line-leading markers —
`!propose #id`, `!support #id`, `!object`, `!evidence`, `!question`, `!commit` —
with an optional trailing `^123` citing a sequence. A body with no marker yields
no trace; ordinary conversation is not silently coerced into a vote.

`read(messages)` folds a projected transcript into traces in sequence order.

### Salience

```rust
pub struct SalienceWeights { pub recency: u16, pub importance: u16, pub relevance: u16, pub half_life: u32 }
pub struct Salience(pub i64);
pub fn salience(trace: &Trace, at: Sequence, weights: &SalienceWeights, relevance: u8) -> Salience;
```

Recency decays by rank in sequence, not by elapsed time — the transcript has no
clock, and rank is what the validated implementation of this score actually
used. `SalienceWeights::DEFAULT` is `recency: 5, importance: 30, relevance: 20`
in tenths, with `half_life: 20`, matching the shipped weights of the design this
is taken from rather than its paper. Decay is mandatory: without it the first
participant to speak keeps the floor forever.

All arithmetic is fixed-point integer, so results are `Eq` and reproducible.

### Quorum and cross-inhibition

```rust
pub struct QuorumPolicy { pub threshold: u32, pub window: u32, pub require_grounded: bool }
pub struct TopicStanding { pub topic: TopicId, pub supporters: Vec<String>, pub silenced: Vec<String>, pub support: i64 }
pub enum ConsensusState { Deliberating, Quorum { topic: TopicId }, Deadlocked { topics: Vec<TopicId> } }
```

Quorum is a **local, decaying count of distinct participants** — `threshold`
distinct supporters within the last `window` sequences — not a global majority.
It is order-independent and idempotent, so a participant that catches up late
folds to the same standing as one that watched live.

Cross-inhibition **targets the advocate, not the option**. An `Object` at
sequence *s* naming a target message removes that message's author from the
supporter set of the topic they were advocating, and records them in `silenced`.
Subtracting from a score cannot break a tie between two equally supported
options; silencing an advocate can, which is the entire reason the mechanism is
shaped this way.

When `require_grounded` is set, a `Support` with no `cites` contributes to
neither `supporters` nor `support`. A conclusion offered without grounds is what
lets an information cascade form, so it is second-class by construction.

`consensus` reports `Quorum` when exactly one topic is at or above threshold,
`Deadlocked` when two or more are, and `Deliberating` otherwise.

### Attention

```rust
pub struct AgentThreshold { pub agent_id: String, pub threshold: i64, pub affinity: Vec<(TopicId, i64)> }
pub enum BidReason { Addressed, Salience, Dissent, Quiet }
pub struct Bid { pub agent_id: String, pub urge: i64, pub reason: BidReason }
```

Every active desk member bids; `floor_holder` takes the argmax, and ties break
by roster order. Taking the argmax rather than everything above threshold is
what enforces one message, one turn.

Two adjustments fold into the bid:

- **Dominance.** Speaker share is measured over the window in *grounded,
  supported* contributions, never raw message count, because raw count is a
  proxy an agent can trivially inflate. A member holding more than
  `dominance_cap` percent of that share is damped; the member with the smallest
  share is lifted, and bids with `BidReason::Quiet`.
- **Repetition.** Once `repetition_cap` distinct peers have acknowledged a
  point, restating it scores zero.

A member whose urge does not clear its threshold does not bid. Speaking raises
the speaker's threshold; a silent round lowers it.

### The episode

```rust
pub enum Phase { Deliberate, Commit }
pub enum Visibility { Blind, Full }

pub struct EpisodePolicy {
    pub turn_budget: u32,
    pub blind_round: bool,
    pub dominance_cap: u32,
    pub repetition_cap: u32,
    pub quorum: QuorumPolicy,
    pub weights: SalienceWeights,
}

pub struct EpisodeState {
    pub conversation: Conversation,
    pub spent: u32,
    pub phase: Phase,
    pub thresholds: Vec<AgentThreshold>,
    pub watermark: Sequence,
}

pub struct HiveTurn {
    pub agent_id: String,
    pub phase: Phase,
    pub visibility: Visibility,
    pub reason: BidReason,
    pub next_state: EpisodeState,
}

pub enum HiveStep {
    Speak { turn: HiveTurn },
    Converged { topic: TopicId, standing: TopicStanding },
    Deadlocked { topics: Vec<TopicId> },
    Exhausted { spent: u32 },
    Idle,
}

pub fn step(state, transcript, roster, desks, policy) -> Result<HiveStep>;
pub fn project_for<'a>(turn: &HiveTurn, messages: &'a [SessionMessage]) -> Vec<&'a SessionMessage>;
```

`step` evaluates in a fixed order: validate the roster and desks; return
`Exhausted` if the budget is spent; fold traces and standings; return
`Converged` if consensus is `Quorum`, the phase is already `Commit`, **and a
`Commit` trace names that topic**; flip `Deliberate` to `Commit` and emit one
commit turn if consensus is `Quorum` and the phase is `Deliberate`; return
`Deadlocked` if consensus is `Deadlocked` and **every member already supports
one of the tied topics**; otherwise take bids and either `Speak` or return
`Idle`.

Two details of that order are load-bearing:

- **Convergence requires the decision to have been recorded**, not merely to
  have been reachable. A room that reaches quorum takes one commit turn, and
  only a `!commit` naming the carried topic ends the episode. If the committing
  participant never records it, the episode runs on and terminates at its
  budget instead — bounded either way, and never silently converged on a
  decision nobody wrote down.
- **A free member is identified from the standings, not from a bid's reason.**
  Bid precedence classifies a member that has also been cited or objected to as
  `Addressed` ahead of `Dissent`, so reading the reason would mask a real
  dissenter and end a breakable deadlock early.

Only messages authored by a **current, active member of the episode's desk**
are folded into traces. A retired agent, or one belonging to another desk,
whose message lands above the watermark is context: visible, but unable to
manufacture a quorum nobody eligible actually holds.

The `Deliberate` to `Commit` transition is one-way. Deliberation and commitment
are different classes of turn, and a room that has settled does not reopen
because a late trace arrives.

`Visibility::Blind` applies while `blind_round` is set and no participant has
spoken twice. `project_for` then hides messages authored by other agents
**within the episode** — that is, above the watermark — keeping operator,
person and system messages, the participant's own work, and the whole
conversation that led into the episode. The round withholds the positions peers
have taken since the room opened, not the context the room was opened about.

`next_state` is returned rather than applied. The caller commits it only after
its turn is durably appended, the discipline `prepare_delta` already
establishes.

## Invariants and constraints

- `HiveStep::Speak` carries exactly one turn. There is no representation of two.
- `standings` is commutative and idempotent over its trace input.
- No floating point anywhere in the crate.
- The crate names no host type, defines no trait a host implements, and performs
  no IO. `.github/scripts/assert-pure.sh` asserts it.
- An episode terminates: `spent` strictly increases on every `Speak`, and
  `turn_budget` is finite.
- A trace never resolves against a retired or inactive member.

## Acceptance criteria

- A three-agent episode reaches quorum, flips once to `Commit`, and reports
  `Converged` with the standing that carried it.
- Two equally supported topics report `Deadlocked`; the same input with one
  grounded objection reports `Converged`. Both directions are asserted, so the
  mechanism is shown to be load-bearing rather than incidental.
- An ungrounded `Support` moves neither `supporters` nor `support` under
  `require_grounded`.
- A spent budget returns `Exhausted` and no turn.
- A dominant speaker is damped and the quietest member bids `Quiet`.
- `project_for` hides peer messages under `Blind` and reveals them under `Full`.
- A point restated past `repetition_cap` scores zero.
- `standings` over a shuffled trace list equals `standings` over the ordered one.
- A quorum with no recorded `Commit` trace does not converge; it runs to its
  budget instead.
- A trace authored by a retired agent, or by one who is not a member of the
  episode's desk, moves no standing.
- Wire forms are pinned for every payload type, as elsewhere in the workspace.
- A gated live run shows real models emitting parseable traces and an episode
  terminating inside its budget. It asserts structure and attribution, never
  answer quality.

## Open questions

None. Roster composition — whose failure modes actually differ — is outside this
algebra and remains the host's responsibility.
