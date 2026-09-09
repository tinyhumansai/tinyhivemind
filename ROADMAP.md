# Roadmap

`tinyhivemind` is being built by moving the shared-conversation layer out of
[`opencompany`](https://github.com/tinyhumansai/opencompany) and fixing two
defects that layer has, in that order. Roughly 60% of the substrate already
exists there and moves; 40% is capability that does not exist yet.

Phases land one at a time. Each is a pair of pull requests — this repository
first, then the `vendor/tinyhivemind` pointer bump in `opencompany` — so the
dependency direction is enforced by construction.

| Phase | What lands | State |
| --- | --- | --- |
| P0 | Reshape the TinyBus module template into a plain library workspace | **done** |
| P1 | Chat identity: `MAIN_THREAD_ID`, `GENERAL_DESK`, `is_general_chat`, `same_conversation` | **done** |
| P2 | Desk types, then the membership algebra behind `DeskSet<'a>` | **done** |
| P3 | The `@` grammar, `Mention`/`MentionTarget`, and resolution over `Roster`/`Person` | **done** |
| P4 | `crates/tinyhivemind`: the `SessionLog` port, the paging walk, and the **attributed** transcript projection | **done** |
| P5 | Continuous sharing — re-seed on a watermark rather than only on a rebind | **done** |
| P6 | The responder ladder, with the model-backed rung behind a `Selector` port | **done** |
| P7 | The mention-dispatch edge, bounded by a host-supplied finite configurable `max_hops` (OpenCompany defaults to 2), with no library hard cap, and explicitly enabled by host policy | **done** |
| P8 | `crates/tinyhivemind-hive`: bounded group deliberation — the trace grammar, salience, quorum with cross-inhibition, the attention market, and the episode state machine | **done** |
| P9 | `!refute`, evidential grounding, grounded objections, and the benchmark arm that scored them | **done**, both knobs **off by default** — see below |
| P10 | A transactive-memory directory folded from traces, `BidReason::Knows`, and `!defer` | **done**, both knobs **off by default** — see below |
| P11 | `SessionMessage.parent` and the structured trace sidecar | planned |
| P12 | Per-conversation read state | planned |
| P13 | Digests and supersession | planned |
| P15 | Cross-desk referral: one child turn that may run on another channel, the answer that comes back, and the federated benchmark that scored it | **done**, every knob **off by default** |
| P14 | Recall: one selection ranking, roster and desk pickers, bounded transcript search with optional regular expressions, pinning as a fold, and a stated per-message budget | **done** |
| P16 | Approval: a pure gate for a side-effecting action — `approve` as a total fold, standing grants, and epoch-scoped consent, with the waiting behind one `ApprovalGate` port | planned, **not implemented** |
| P17 | Private asides: an audience on a stored row, a viewer on a query, the collapsed redaction stub and its settlement pointer, and the rule that an aside carries information rather than support | **done**, **off by default** — the benchmark arm says asides do not improve a decision, see below |
| P18 | The utterance surface: a seat speaks by calling a tool rather than emitting a fence — the tool descriptions, the validation and the utterance-to-row fold live in `tinyhivemind::speech`, and a refused aside reaches its author inside the turn | **done** — see [`docs/specs/the-utterance-surface.md`](docs/specs/the-utterance-surface.md) |
| P19 | Folding by size: the standing account triggers on the characters of foldable content as well as its row count, stated by a host as a token budget, and the fold is told which messages the room pinned so it cannot drop one | **done** — see [`docs/specs/folding-by-size.md`](docs/specs/folding-by-size.md) |
| P20 | A real provider layer for the `desk` example: `tinyinference` behind the `Digester` and wrap-up paths in place of a `curl` subprocess, with classified provider failures, and `tinytools` rendering the room's tool surface — both example-only dev-dependencies, no library crate touched | **done** — see [ADR 0013](docs/adr/0013-a-vendored-crate-is-an-example-dependency.md) |
| P21 | Concurrent rounds: a step authorizes a bounded *round* of turns rather than one, `next_state` moves onto the round, and a peer row written in the same round is invisible to it — depth becomes rounds rather than turns | **done** — see [`docs/specs/concurrent-rounds.md`](docs/specs/concurrent-rounds.md) and [ADR 0014](docs/adr/0014-a-round-authorizes-concurrent-turns.md) |
| P22 | A task with a horizon: `--stages` runs a chain of decisions on one accumulating window, against a soloist handed the whole brief that compacts by eviction or by a superseding account | **done** — see [`docs/specs/long-horizon-tasks.md`](docs/specs/long-horizon-tasks.md) and [the experiment](docs/experiments/2026-09-09-the-long-horizon.md) |

P15 is also out of order, and for a related reason: it is not a wire-format
change either, and it answers a pressure none of P11 through P13 address. Every
mechanism before it stops at the edge of one conversation, so a room of agents
can pool what its members know and a *company* of them cannot. A desk is a
correlation boundary — members of one desk are wrong about the same things —
and no amount of deliberating inside a channel cancels an error every member
shares. See [`docs/specs/cross-desk-referral.md`](docs/specs/cross-desk-referral.md),
[ADR 0006](docs/adr/0006-a-referral-crosses-one-channel-at-a-time.md) and
[the federated experiment](docs/experiments/2026-09-02-federated-hidden-profile.md).

P17 is also out of order, and unlike P14 and P15 it *is* a wire-format change —
so it owes the compatibility story P11 and P12 owe, and
[`docs/specs/private-asides.md`](docs/specs/private-asides.md) carries it. It
comes first because it answers a pressure none of P11 through P13 touch. Every
mechanism to date narrows what a turn *sees* by time or by conversation, never
by *reader*: two agents on one desk receive byte-identical projections, so a
room where everyone reads everything is the only room this library can express.
That is a group chat. The measured cost is already in the harness — ADR 0005
records a 24-point gap between a blind opening and full visibility — and P17
generalises that one crude knob from per-turn and time-based to per-message and
addressee-based. See [`docs/specs/private-asides.md`](docs/specs/private-asides.md),
[ADR 0010](docs/adr/0010-an-aside-carries-information-never-support.md) and
[the reading](docs/research/context-in-agent-teams.md).

The arm that measures it **lost**, and the loss is published:
[`docs/experiments/2026-09-07-do-asides-help.md`](docs/experiments/2026-09-07-do-asides-help.md)
records −2.5 points at the tuned turn budget, nothing once the budget stops
binding, and −15.3 on a hidden profile, where averaging with a peer inside one
correlated desk imports the shared bias instead of cancelling noise. The
matched public control settles the narrower question: privacy is worth nothing
to a decision either way. P17 therefore rests on auditability and bounded
independence, and claims nothing about answer quality.

P14 is out of order on purpose. It is not a wire-format change and does not
wait on P11 through P13: it answers the same pressure they do — a bounded
window over an unbounded log — with the two mechanisms that need no new port
and no new stored state. Search makes the transcript queryable rather than
something a turn must hold, pinning keeps a small working set arriving whether
or not anybody asked, and `BrevityPolicy` states the budget every message is
spending out of. See [`docs/specs/recall.md`](docs/specs/recall.md).

P16 is the next number free, and it sits after the phases that have landed
rather than inside the P11 through P13 block for the same reason P14 and P15
did: it is not a wire-format change, it does not wait on `SessionMessage.parent`
or on read state, and it answers a pressure none of them address. It also needs
nothing from the hive crate. See [`docs/specs/approval.md`](docs/specs/approval.md)
and [ADR 0008](docs/adr/0008-an-approval-decision-is-total.md).

The next work is the paired OpenCompany adapter integration, followed by a
gated live-provider verification in which two agents exchange an attributed
turn. The adapter initially remains disabled and uses two hops when enabled.
The hive crate is opt-in and is not part of that first adapter.

P11 through P13 come out of a survey of the biology, the group-decision
literature, and the open-source landscape of shared agent memory, recorded in
[`docs/research/`](docs/research/README.md) and specified in
[`docs/specs/shared-medium-schema.md`](docs/specs/shared-medium-schema.md). They
are ordered by leverage, and P11 and P12 are wire-format changes that need their
serde-compatibility story written down before any code.

## What P8 adds, and what it deliberately does not

P8 answers a question the first seven phases do not: how a *room* of agents
reaches a decision, rather than how one message finds its one responder. It adds
a trace grammar over the shared transcript, a decaying salience field, quorum
counted as distinct grounded supporters, cross-inhibition that silences an
advocate rather than debiting an option, and an attention market whose argmax
yields exactly one speaker.

It adds **no port**. An episode is a pure fold, and the host does its waiting
through `SessionLog`, `Selector` and `MentionTurnQueue` — the ports it already
implements. `crates/tinyhivemind-hive` is in the `pure_crates` list in
`.github/scripts/assert-pure.sh`.

It is also not a claim that group deliberation produces better answers. Almost
every positive multi-agent result in the literature is confounded by compute,
and self-consistency at a matched token budget is the honest control. P8 is a
protocol for bounded deliberation with an auditable termination reason, and
nothing more. See
[`docs/adr/0002-hive-episodes-are-sequential.md`](docs/adr/0002-hive-episodes-are-sequential.md).

## What P9 adds, and why it is off

P9 answers the second finding of
[the live hidden-profile run](docs/experiments/2026-09-01-live-hidden-profile.md):
support is counted and grounds are not weighed, so a fact that refutes a
hypothesis has no way to say so and killing one costs a turn per advocate.

It adds `!refute #topic ^N`, which caps a topic once `refutation_cap` distinct
grounded members have argued a cited fact against it, and `require_evidential`,
under which a support counts only if its citation chain reaches a stated fact.
Both are pure folds. Both are recorded in
[ADR 0003](docs/adr/0003-refutation-links-evidence-to-a-topic.md) and
[ADR 0004](docs/adr/0004-grounds-are-weighed-by-evidential-depth.md).

**Both are off in `QuorumPolicy::DEFAULT`, because the benchmark scored them and
they lost.** `hive+ref` reaches 75.0% against 82.1% for the same policy without
it — below even the matched-budget vote — `hive+ev` reaches 55.9%, and no policy with either knob on appears in the
top twelve of an 864-point grid search. The spec's acceptance criterion required
the arm to be able to lose; it did, and the result is written up rather than
buried. The mechanism stays in the library, opt-in, because the case it was
built for — a hidden profile, where one member holds the fact that overturns a
decoy — is not what the simulated benchmark measures. See
[`docs/experiments/2026-09-01-refutation-and-grounds.md`](docs/experiments/2026-09-01-refutation-and-grounds.md).

## What P10 adds, and why it is off

P10 answers the first finding of
[the live hidden-profile run](docs/experiments/2026-09-01-live-hidden-profile.md):
the member holding the fact that overturns the decoy is in the room, has
already deposited it, and never wins another turn to press it. Before this the
library could say *who is here* and *who spoke*, and had no way to say *who
knows*: the one expertise-shaped field, `AgentThreshold.affinity`, was
host-supplied and never written by anything in the workspace.

It adds `directory`, a pure fold estimating one weight per `(agent, topic)`
from grounded deposits and the citations they drew — Wegner's transactive
memory, with Lewis's specialisation and credibility as the two estimators the
transcript can support. It feeds `BidReason::Knows`, which sits between
`Dissent` and `Quiet` and gives the floor to the member the transcript says
holds the contested topic and who has taken no position on it. It also adds
`!defer #topic`, the abstention that hands a topic to whoever does hold it,
bounded by `defer_cap`. Nothing is stored: the directory is refolded on every
step. Recorded in
[`docs/specs/expert-delegation.md`](docs/specs/expert-delegation.md) and
[ADR 0007](docs/adr/0007-the-directory-is-folded-from-citations.md), with the
reading in [`docs/research/delegation.md`](docs/research/delegation.md).

**Both are off in `EpisodePolicy::DEFAULT`, because the benchmark scored them
and they did not win.** The acceptance criteria were written before any
numbers: the mechanism must be able to lose and the loss must be published;
`vote` gets the same turn budget; a mechanism that helps hidden profiles but
costs more than two points on the uniform 5000-room bench ships off; and
directory circularity is reported as the rank correlation between directory
weight and speech share.

The uniform bench predicted zero and delivered zero: `hive+dir` is `hive+` to
the digit at 82.1% over 5000 rooms, and `BidReason::Knows` never fires there at
all. On a hidden profile with an evidence-first opening it scores **65.8%
against `hive+`'s 66.3%** with `Knows` winning the floor in 77.5% of episodes,
and `hive+defer` moves `±0.5` and never leaves the interval. A *directed*
router is worse still — `ladder+dir` reaches 45.1% with two specialists against
the uninformed ladder's 52.6%, while routing to the decisive member more often
(22.3% against 18.9%) — and it degrades with shared history rather than
sharpening. The one thing that moves a hidden profile is when a member speaks:
depositing facts before taking positions takes the same rooms from 15.3% to
66.3%, `+51.3 [+49.8, +52.8]` over the matched-budget vote, which is a finding
about participants rather than about this fold. The circularity number `rho`
falls from `0.83` to `0.07` across those same arms, so the estimator can be
made to stop measuring speech — it just does not buy accuracy when it does.
Twenty-seven live rounds add the participant half of the same answer: the
fact-holder spoke before the commit in every room that had one and fourteen of
twenty-three were still wrong, no turn was ever awarded on `BidReason::Knows`,
and `!defer` was used on none of 266 turns.
See [`docs/experiments/2026-09-05-expert-delegation.md`](docs/experiments/2026-09-05-expert-delegation.md).

## What P16 adds, and what it deliberately does not

P16 answers the largest well-evidenced gap in
[the Grok Bot survey](docs/research/grok-bots/README.md): this library can say
who is here, who a mention addresses, who takes the next turn and how a room
reaches a decision, and it cannot say whether the thing that decision leads to
is allowed to happen. Six of the twelve surveyed projects gate side-effecting
actions, and in every one of them the decision is already a pure function
separated from the IO that enacts it — the core/port line this workspace draws,
arrived at independently five times.

It adds an `approval` module to `crates/tinyhivemind-core`: `approve` as a
**total** fold returning `Allow`, `Deny { reason }` or `Ask { who }`; a scope
key over `(actor, call, verb, target)` with call, action and resource-pinned
grant scopes; standing grants as a pure liveness and coverage predicate over
records the caller supplies, with `now` passed in rather than read; and
epoch-scoped consent, under which a grant issued later cannot cover a request
minted earlier. `Ask` names exactly one person, resolved through the existing
roster and desk algebra, and never an agent.

It adds **one port**, `ApprovalGate`, in `crates/tinyhivemind`, sibling to
`MentionTurnQueue` and `ReferralQueue`. Approval *decides*; every wait for a
person is IO and belongs there or in the host. That is the ADR: the contested
choice was not that the fold decides rather than enacts, but that it is total —
a gate that can fail is a gate that can be bypassed by failing, so `approve`
returns no `Result` and adds no `Error` variant. See
[ADR 0008](docs/adr/0008-an-approval-decision-is-total.md).

It does **not** execute anything, define a tool surface, embed a policy
language, store an audit trail, handle a credential, or sandbox a command. The
resource predicate is lexical path containment, not a kernel boundary, and the
spec says so where a reader would otherwise assume otherwise. It does not relax
one message, one turn: no decision variant carries a turn, an `Ask` is not a
mention and never becomes a `MentionTurnRequest`, and approval expresses no
edge to the dispatch or referral folds in either direction.

## The two defects P4 and P5 fix

**The transcript is first-person-collapsed.** The host's projection discards
the author of every reply, so on a shared desk agent B reads agent A's replies
as B's own prior turns. A system notice, a workflow report and a real teammate
are indistinguishable. P4 replaces the `(role, content)` pair with an
attributed `SessionMessage`.

**The transcript is not continuously shared.** It is re-read only when an agent
rebinds to a different chat, so an agent misses a peer's interleaved reply on
consecutive turns in one thread. P5 replaces that gate with a watermark.

## Non-goals

- **A second journal.** The host owns the append-only log; this crate borrows it
  through a port. Messages are addressed by sequence number across surfaces the
  host owns (reactions, board cards, run rows), so a second log cannot be made
  consistent with the first.
- **A web framework, or HTTP handlers.** Routes stay with the host.
- **Unbounded fan-out.** A step may authorize a *round* of concurrent turns, but
  never more than `round_width` of them and never without an approval in sight —
  the bound is the invariant, and the serialization never was. Independence is
  still a visibility filter: members writing simultaneously cannot read each
  other, so a concurrent round is a blind round. See
  [ADR 0014](docs/adr/0014-a-round-authorizes-concurrent-turns.md), which
  supersedes ADR 0002 on the terms ADR 0002 itself set, and P21 below.
