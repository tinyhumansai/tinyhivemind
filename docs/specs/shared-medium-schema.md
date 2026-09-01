# The shared medium's schema

**Status:** Draft
**Owner:** tinyhivemind maintainers

## Problem

The charter says this repository holds "hive mind mechanics for agents: a
**shared session transcript** that several agents read and write". The projected
unit of that transcript is three fields:

```rust
pub struct SessionMessage { sequence: Sequence, author: SessionAuthor, content: String }
```

Everything else a reader might need is either dropped at projection or
re-derived by re-parsing prose on every fold. Five specific costs, each already
visible somewhere in the repository:

1. **`parent` is dropped.** `project_channel` needs a private `Candidate` struct
   to do a narrowing no caller can do, and
   [`thread-scoped-conversations.md`](thread-scoped-conversations.md) records
   that the channel-level rule "must happen inside the walk, where `LogMessage`
   still has it, and cannot be a fold a caller applies afterwards."
2. **Traces have nowhere to ride.** `trace::resolve(body, supplied, …)` accepts
   an authoritative supplied list and revalidates it against the body — the
   right shape, with no field on `SessionMessage` to carry it. So `trace::read`
   re-parses every message on every step.
3. **One scalar watermark per agent.** `SharingState` holds one `Conversation`
   and one `watermark`. The stated goal of the thread work is that "an agent can
   hold two conversations in one desk at once", which a scalar cursor cannot
   express — the retrofit Matrix had to do in MSC3771 once it added threads.
4. **Truncation is silent and citations do not survive it.** `SESSION_WINDOW`
   and `SCAN_LIMIT` drop the oldest rows, and a `^N` below the window resolves to
   nothing with no way for a reader to tell that from a fabricated citation.
5. **Contradiction has no representation.** Two `!evidence` traces that
   contradict each other both stand forever, modulo decay. A correction cannot
   retire a claim, so "what did the room believe at turn four" is not answerable
   even though every payload derives `Eq` and every fold replays.

## Goals

- Give a projected message the fields a caller needs to reproduce the
  projection's own decisions.
- Let per-participant read state be correct when a participant holds more than
  one conversation.
- Make truncation representable, so a citation into truncated history resolves
  to something rather than to nothing.
- Let a claim be retired by a later one without deleting either.
- Change no wire format without a stated compatibility plan, because
  `crates/*/tests/public_api.rs` and the serde unit tests pin every payload and
  a host may pin any commit on `main`.

## Non-goals

- **A second journal.** Every item here is a field on a record the host already
  writes, or a fold over records it already holds. Nothing introduces a store.
- **Host types.** A digest names a sequence range; it does not name a card, a
  run, or a board.
- **Mutable messages.** Revisability is denied deliberately — see below.
- **A directory as storage.** Who-knows-what is a fold, not a table.

## Proposed behavior

Five changes, independently landable. Each names the phase it lands in.

### `SessionMessage` carries `parent` — P11

```rust
pub struct SessionMessage {
    pub sequence: Sequence,
    pub parent: Option<Sequence>,
    pub author: SessionAuthor,
    pub content: String,
}
```

`project_channel`'s `narrow_to_roots_and_first_replies` becomes expressible by a
caller, the private `Candidate` struct is retired, and
[`thread-scoped-conversations.md`](thread-scoped-conversations.md)'s one
outstanding constraint is removed.

Compatibility: additive on the wire and breaking for struct-literal
construction. `parent` uses `deserialize_required_option`, matching
`Trace.topic` and `DispatchConversation.thread_root`, so an old payload without
the key is rejected loudly rather than defaulting to `None` and silently
reintroducing thread collapse.

### A structured sidecar — P11

An optional, host-supplied, always-revalidated payload beside the prose —
MetaGPT's `instruct_content`, Semantic Kernel's `Items[]`, A2A's `Part`. The
rule that makes it safe is the one `mention::resolve` and `trace::resolve`
already implement: **the body is authoritative, the sidecar may only select**.
A supplied entry survives only if it still matches the body verbatim at its
offset; every other field it claims is discarded in favour of the body.

This turns per-step re-parsing into a cache with a revalidation step, and it is
the only place the two-tier idea (AutoGen's `ChatMessage` versus `AgentEvent`)
can land without a second log: a record whose content is a feedthrough event
rather than a chat turn is a record with a sidecar and no prose.

### Per-conversation read state — P12

`SharingState` becomes keyed. The `PRESENT_SET_LIMIT` discipline applies per
key, the hand-written `Deserialize` that enforces it gains a bound on the number
of keys, and `prepare_delta` reads the entry for the desired conversation
instead of comparing one stored conversation against it.

`ReinitializeReason::ConversationChanged` stops being the answer to "the agent
moved thread", which is the case it currently over-triggers on, and stays the
answer to "the desk is not the one this state belongs to".

Compatibility: this is the largest wire change of the five. The state is
caller-owned and caller-serialized, so the migration is the host's; the spec
must say whether a single-entry legacy payload deserializes into a one-key map
or is rejected. **This is the open question that blocks acceptance.**

### Digests — P13

A digest is an ordinary appended record that names the sequence **range** it
stands for. Projection substitutes it for the range, and a citation into the
range resolves to the digest rather than to nothing.

Three properties, taken from Kafka's compaction and Claude Code's `leafUuid`:
compaction is an **added record, never a deletion**; sequence numbers are
**never renumbered**, so gaps are normal and every held cursor and citation
stays valid; and a digest is addressable, so "this was summarized" is
distinguishable from "this never existed".

### Supersession — P13

A trace may retire an earlier trace: `!supersede ^N`, folded as an edge, so a
correction removes the earlier claim from standings without removing it from the
transcript. This is Graphiti's bi-temporality reduced to the single axis this
crate has — there is no clock, so `valid_at` is meaningless, but *superseded at
sequence N* is exactly expressible.

The alternative — editing a message — is refused. Clark & Brennan's revisability
is a hazard here rather than a feature: a message that changes after it is read
makes every citation of it a citation of something that no longer says what it
said, and this crate's whole audit story is that a decision traces back to the
messages that carried it.

### A transactive-memory directory — P10

Not a schema change; a fold, listed here because it is the same subject.
`AgentThreshold.affinity` is a static, host-supplied who-knows-what entry — an
agent, a topic, a weight. Derive it instead: a member who deposited grounded
`Evidence` on a topic knows about that topic, and one whose deposits were later
cited knows about it credibly. Feed it into `bids` as `BidReason::Knows`,
between `Dissent` and `Quiet` in precedence, so the floor goes to the member
whose deposits cluster on the contested topic and who has not yet spoken on it.

This is the mechanism that would have surfaced scout's refutation in the failed
rooms of the [live run](../experiments/2026-09-01-live-hidden-profile.md), and
it is Wegner's directory with the estimators the transcript already carries.

## Invariants

- Every fold stays pure, order-independent on `(sequence, offset)`, and
  fixed-point.
- No record is ever deleted or renumbered. Digests add; supersession annotates.
- A sidecar can only narrow what the body already says. It is never trusted.
- The host owns storage. Nothing here opens anything.

## Acceptance criteria

Per change, and each change is separately acceptable:

1. `parent` survives projection, `Candidate` is gone, and the channel-level rule
   is exercised through the public API by a test that could not have been
   written before.
2. A sidecar whose offsets no longer match the body is discarded, and the body's
   own extraction is used.
3. A participant holding two threads in one desk receives correct deltas for
   both across interleaved ticks, with no `Reinitialize`.
4. A citation into a digested range resolves to the digest; sequence numbers in
   the surviving transcript are unchanged.
5. A superseded claim is absent from `standings` and present in the transcript.
6. `BidReason::Knows` gives the floor to the holder of an uncited relevant fact
   in a constructed hidden-profile transcript.

## Open questions

1. **The `SharingState` migration.** Does a legacy single-conversation payload
   deserialize into a one-key map, or is it rejected? This blocks acceptance of
   the P12 half.
2. Does the sidecar belong on `SessionMessage`, or on a wrapper the hive crate
   owns? Putting it on `SessionMessage` makes every host pay for a field only
   deliberation uses.
3. Does a digest need an author? `SessionAuthor::System { kind, label }` can
   carry it, which argues no.
4. Should `require_evidential`'s chain resolution follow a citation *into* a
   digest, and if so what kind does the digest report?

## Related

- [ADR 0003](../adr/0003-refutation-links-evidence-to-a-topic.md),
  [ADR 0004](../adr/0004-grounds-are-weighed-by-evidential-depth.md),
  [ADR 0005](../adr/0005-a-blind-round-may-be-concurrent.md).
- [`thread-scoped-conversations.md`](thread-scoped-conversations.md), whose one
  outstanding constraint the `parent` change removes.
- [`../research/shared-context.md`](../research/shared-context.md), for the
  landscape each item is taken from.
