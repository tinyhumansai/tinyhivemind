# Shared context: the human half, and the landscape

Companion to [`biology.md`](biology.md). The insect mechanisms say how a room
converges; they are silent on what a trace should *contain* and how a reader
knows it was understood. That is the human literature. The second half surveys
what open-source multi-agent and multi-writer systems actually put in their
schemas.

## Transactive memory

Daniel Wegner, "Transactive memory: a contemporary analysis of the group mind",
in Mullen & Goethals (eds.), *Theories of Group Behavior*, Springer (1986),
185–208; and "A computer network model of human transactive memory", *Social
Cognition* 13(3):319–339 (1995), where Wegner himself reaches for the
directory-sharing metaphor.

A transactive memory system is the individual stores **plus** the processes that
link them, and it has three operations:

- **Directory updating** — maintaining a shared *who knows what* map. This is
  the actual shared object. The group's memory is the directory, not the
  contents.
- **Information allocation** — routing a new item to whoever the directory says
  owns that domain. Note that the encoder may then not retain the item at all.
- **Retrieval coordination** — knowing where to look, and in what order.

Lewis, *J. Applied Psychology* 88:587–604 (2003), validates the three-factor
scale: **specialization, credibility, coordination**. Credibility is the
load-bearing one here — a directory entry is useless unless the retriever trusts
the owner enough not to re-derive the answer. Liang, Moreland & Argote, *PSPB*
21:384–393 (1995), find that training teams *together* improves performance and
that the effect is mediated by directory quality. Ren & Argote, *Academy of
Management Annals* 5:189–229 (2011), is the review.

The failure modes are named: directory **staleness**, **shared-information
bias** (Stasser & Titus again — the group discusses what everyone already
knows), and the Google effect (Sparrow, Liu & Wegner, *Science* 333:776–778,
2011 — people who expect an external store to persist remember *where* rather
than *what*, which is the intended behaviour until the store is lossy).

> **What this workspace would have to represent.** `Roster` answers *who is
> here*. Nothing answers *who knows what*. The hidden-profile failure recorded
> in [the live experiment](../experiments/2026-09-01-live-hidden-profile.md) is
> precisely a transactive-memory failure: scout's refuting fact was in the room,
> at a citable sequence, and nothing could route the floor to the member holding
> it. `AgentThreshold.affinity` is the shape of a directory entry — an agent, a
> topic, a weight — supplied statically by the host rather than folded from the
> transcript. Folding it is a pure operation over traces the crate already
> reads, and specialization and credibility both have obvious estimators in it:
> who deposited grounded evidence on a topic, and whether their deposits were
> later cited or refuted.

## Distributed cognition

Edwin Hutchins, *Cognition in the Wild*, MIT Press (1995); "How a cockpit
remembers its speeds", *Cognitive Science* 19(3):265–288 (1995).

The formulation: cognition is "computation realized through the creation,
transformation and propagation of representational states across representational
media". The unit of analysis is the system — people plus instruments plus
procedures plus layout — not the individual.

In the ship's-bridge case a position fix is computed by a chain in which nobody
holds the whole computation: pelorus operators sight landmarks and report
bearings *by voice*, a recorder writes them into a durable timestamped
**bearing record log**, a plotter transfers them to the chart with a protractor
whose mechanical constraint performs part of the trigonometry. The chart is not
a display of the answer; it is where the computation happens. Hutchins's
decisive move is that the system's cognitive properties follow from the
**physical properties of the media** — a written log is reviewable, persistent
and inspectable by a bystander; a spoken bearing exists only at the moment of
utterance. When the gyrocompass failed, the crew reorganized the *propagation
pathways*, not anyone's knowledge.

The cockpit paper makes the memory point sharpest: V-speeds are remembered by
the cockpit as a system, partly in heads, partly in a speed-card booklet, and
critically in the **speed bugs** physically set on the airspeed dial. The bug
converts a remembered number into a perceptual judgement — is the needle past
the bug? — and that is the deepest available lesson for a transcript: *the best
shared memory changes the task from recall to perception*.

> **What this workspace would have to represent.** The attributed transcript is
> a bearing log and it is already the right shape. What it lacks is the second
> medium. Every intermediate representation currently has exactly one form:
> prose in a `SessionMessage`, re-parsed on every read. A folded standing —
> which topic carries, who supports it, how far it is from quorum — is computed
> and thrown away each step. The bench harness already found this: the fix for
> models coining `#rollout` and `#rollout-strategy` was to *show the room its
> own standings* in the prompt. That is a speed bug, and it lives in an example
> rather than in the library.

## Grounding

Herbert Clark & Susan Brennan, "Grounding in communication", in Resnick, Levine
& Teasley (eds.), *Perspectives on Socially Shared Cognition*, APA (1991),
127–149.

Common ground is what participants *mutually believe* they share, and each
contribution has a presentation phase and an **acceptance** phase in which the
addressee supplies evidence of understanding. The **grounding criterion** is
that both parties mutually believe understanding was reached "to a criterion
sufficient for current purposes" — task-relative and negotiable, not absolute,
which is a far cheaper contract than "shared state must be identical" and the
right one for agents too. The **principle of least collaborative effort** says
participants minimize *joint* effort, not their own.

The eight media constraints are the part that reads as a schema checklist:
copresence, visibility, audibility, cotemporality, simultaneity, sequentiality,
reviewability, revisability. A shared append-only transcript grants
**reviewability** — the whole point — and **simultaneity**, and denies
copresence, visibility and audibility, which means every cheap back-channel
signal of uptake is unavailable and **acknowledgement must be made explicit and
paid for**. It grants **sequentiality** only where a total order is enforced.
**Revisability** is a genuine hazard rather than a feature: if a message can
change after it is read, an agent citing it is citing something that no longer
says what it said.

> **What this workspace would have to represent.** Sequentiality is bought
> already — `Sequence` is a total order and `same_conversation` scopes it — and
> revisability is denied by construction, which is the correct choice and worth
> stating as one. Acknowledgement is the gap: `repetition_cap` counts how many
> "distinct peers have acknowledged a point", but acknowledgement is *inferred*
> from a trace being deposited, never stated. `Trace.cites` is the nearest thing
> to positive evidence of uptake and is used for grounding rather than for
> grounding-in-the-Clark-sense. A trace kind meaning "I have read this and it
> changes nothing" is cheap, and is the only way a room can distinguish silence
> from assent.

## Boundary objects and awareness

**Boundary objects.** Star & Griesemer, *Social Studies of Science* 19(3):387–420
(1989): objects "plastic enough to adapt to local needs … yet robust enough to
maintain a common identity across sites", weakly structured in common use and
strongly structured in local use. The point is **coordination without
consensus**. Four types: repositories, ideal types (useful *because* imprecise),
coincident boundaries, and standardized forms — the last being the one most
prone to becoming a rigid, information-losing artifact. Star's own corrective,
*Science, Technology & Human Values* 35(5):601–617 (2010), insists the
interpretive flexibility is not optional.

**Workspace awareness.** Gutwin & Greenberg, *CSCW* 11(3–4):411–446 (2002).
Awareness is a two-tense structure — who/what/where now, plus how/when/who-did-
that historically — and it is gathered three ways: **consequential
communication** (information given off unintentionally by the act of working),
**feedthrough** (observing the effects of someone's actions on shared
artifacts), and **intentional communication** (explicit telling, the expensive
fallback). Feedthrough is sematectonic stigmergy under a different name, and
the design rule is to maximize the first two so as to spend as little as
possible on the third.

**Situation awareness.** Endsley, *Human Factors* 37(1):32–64 (1995): perception,
comprehension, projection. A raw event feed delivers only perception. Team
situation awareness is explicitly *not* the union of individual awareness —
over-broadcasting the non-shared parts degrades it.

> **What this workspace would have to represent.** `TeamBriefing` and
> `SessionContext` are boundary objects and are correctly kept separate from
> history. `ThreadLine` is the beginning of a repository index. What does not
> exist is feedthrough: an agent's tool calls and artifact edits are invisible
> unless it narrates them, so a room's only awareness channel is the expensive
> one. That is the host's to emit, but the *record kind* it would emit into is
> this library's to define — and today every record is a chat message.

## Global workspace

Baars, *A Cognitive Theory of Consciousness*, CUP (1988); Mashour, Roelfsema,
Changeux & Dehaene, *Neuron* 105(5):776–798 (2020). Massively parallel
specialized processors compete for a capacity-limited workspace; the winner's
content is **broadcast** back to all of them. **Ignition** is the nonlinearity:
"the sudden, coherent, and exclusive activation of a subset of workspace neurons
… with the remainder inhibited" — all-or-none, self-sustaining after the
stimulus ends, and exclusive by lateral inhibition.

This is worth recording because the architecture is already that shape. `bids`
is the competition, `floor_holder` is the argmax, and appending to the shared
transcript is the broadcast. The one-message-one-turn rule is not a concession
to safety with a performance cost; it is the same design a brain uses, and the
lateral inhibition is `!object`. Self-sustaining ignition is the one part
missing: a decision that carries has no minimum dwell, so a late trace can move
standings the step after they settled — which is what `commit_boundary` and the
one-way `Deliberate → Commit` transition exist to bound.

## The open-source landscape

Read for schema, not for marketing. Full notes with URLs are in the pull request
that added this file; what follows is the transferable part.

| System | The lesson |
| --- | --- |
| **Zulip** | Conversation scope is a **mandatory, total, mutable** string on every record, addressed as `(channel, topic)`. Mutability is the feature: agents mislabel, and re-filing is cheaper than fragmentation. Per-participant state is one row per `(reader, message)` with flags, so per-topic unread counts fall out and no threaded-receipt mechanism was ever needed. |
| **Slack** | The counterexample. Threading is one nullable `thread_parent_id`, added years after channels, optional and secondary — which produced non-uniform adoption, a second unread model, and finally "also send to channel" as an escape hatch. Optional structure is worse than none. |
| **Matrix** | `prev_events` (what the sender had seen) and `auth_events` (what makes the write legitimate) are **different edge sets over the same node**. Event id is a content hash, so dedup, integrity and citability are one field. Threads via `m.thread` are server-side aggregatable, so a client gets `{latest, count, participated}` without downloading history. And MSC3771 had to make read receipts **per-thread**, because "read receipts and read markers assume a single chronological timeline". |
| **Kafka** | Immutable log plus per-reader offset is the entire architecture of several readers at different positions. Compaction is **offset-stable** — retained records keep their original offsets, so compaction leaves gaps and never renumbers, and every held cursor and citation stays valid. Tombstone lifetime is bounded by the slowest reader, not by a fixed TTL. |
| **MetaGPT** | A shared message pool with two orthogonal addressing axes: `send_to` (who it is for) and `cause_by` (what it is about), with roles subscribing on the second. Also `instruct_content`: a structured, machine-checkable payload carried beside the prose. |
| **AutoGen / AG2** | `ChatMessage` versus `AgentEvent` — two tiers over one stream, separating what enters another agent's prompt from what is only telemetry. `OnContextCondition` gives a deterministic routing path beside the LLM one, so selection does not always cost a model call. |
| **LangGraph** | A **reducer per channel**: different fields want different merge policies in one session. `add_messages` is an ID-keyed upsert, so edit and redact are ordinary appends. Long-term memory is namespaced by a hierarchical tuple with prefix search. |
| **Letta / MemGPT** | Memory blocks can be attached to several agents at once — genuine shared mutable state — and the docs rank the mutation primitives by concurrency safety: append is safe, replace is a compare-and-swap that fails loudly, wholesale rewrite loses updates and needs an owner. Say which is which in the API. |
| **Zep / Graphiti** | Bi-temporality: `created_at`/`expired_at` (when the system learned it) is separate from `valid_at`/`invalid_at` (when it was true). Nothing is deleted; a contradiction **supersedes**. Point-in-time replay needs both axes. |
| **A2A** | Message (conversational, ordered) versus Artifact (durable, named output) — two record kinds in one log. Conflating them forces readers to re-derive results by scanning prose. |
| **MCP** | Update notifications carry only a URI, never a payload: **invalidate, do not push**, so there is no ordering or merge problem. `annotations.audience` and `annotations.priority` are a ready-made vocabulary for per-reader projection and context-budget eviction. |
| **Claude Code / OpenCode transcripts** | The session is a **tree**, not a list: `parentUuid` linkage makes a rewind a branch, and a summary record carries `leafUuid` pointing at the tip it replaces. Compaction is an *added node*, never a deletion. `isSidechain` keeps subagent traffic out of the derived view while staying in the same store. |
| **Blackboard / HEARSAY-II** | Levels of abstraction as first-class partitions of the shared board, hypotheses carrying **credibility ratings** integrated across levels, and knowledge sources triggered by *changes* to the board rather than by a pipeline. The classic critique is that the control problem then dominates — which is what "auto speaker selection costs a full transcript read per turn" is a restatement of. |
| **Linda tuple spaces** | `rd` (observe) and `in` (claim, destructively, with a uniqueness guarantee) are different operations. Generative communication decouples producer and consumer in time, space and identity — nobody addresses anybody, which is stigmergy from the coordination-language side. |
| **Mem0** | The negative result. An LLM arbitrates `ADD`/`UPDATE`/`DELETE` over shared state, which makes the write path non-deterministic and unauditable. Keep the model in the content and deterministic code in the merge. |
| **Cognition versus Anthropic** | Cognition: share *full* traces, because a summarized message loses the reasoning and parallel writers then make conflicting implicit decisions. Anthropic: repeated condensation plays telephone with citations, and token usage explains most of the performance variance. Both are right, and the only shape that satisfies both is one durable full-fidelity log plus cheap per-reader **projections** — which is already this library's shape and is the strongest thing to say about it. |

## What the landscape says this workspace already gets right

Worth recording, because it is the argument for the design rather than against
it.

- **One log, many projections.** Every framework above either floods context
  (broadcast to all) or fragments it (pairwise isolation, per-agent retrieval).
  `project_session`, `project_for` and `prepare_delta` narrow *presentation*
  without narrowing *availability*, which is the shape Cognition and Anthropic
  jointly argue for and neither implements.
- **Deterministic merge.** Every fold here is pure and replayable. Mem0's
  LLM-arbitrated writes are the counterexample, and CrewAI's implicit retrieval
  assembly means you cannot reproduce what an agent saw.
- **Attribution is never collapsed.** `SessionAuthor` survives projection. This
  is P4, and it is the one thing every chat-shaped framework gets right and
  every memory-shaped one loses.
- **No second journal.** The charter's refusal is the same discipline as Kafka's
  single log and event sourcing's single source of truth.

## What it says is missing

In the order the [work plan](../../ROADMAP.md) takes them:

1. Evidence cannot argue against a topic, only against an advocate — the `α(v)`
   gap from [`biology.md`](biology.md), and
   [ADR 0003](../adr/0003-refutation-links-evidence-to-a-topic.md).
2. Grounds are counted, not weighed, so a citation of a citation of an opinion
   is worth a citation of a fact — the cascade condition, and
   [ADR 0004](../adr/0004-grounds-are-weighed-by-evidential-depth.md).
3. No who-knows-what directory, so a room cannot route to the member holding the
   fact.
4. `SessionMessage` drops `parent` and carries no structured payload, so
   thread narrowing cannot be a caller's fold and traces are re-parsed on every
   read.
5. One scalar watermark per agent, which is only correct on a single timeline —
   the retrofit Matrix had to do.
6. No compaction that keeps citations resolvable, and no supersession.
