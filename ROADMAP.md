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

P15 is also out of order, and for a related reason: it is not a wire-format
change either, and it answers a pressure none of P11 through P13 address. Every
mechanism before it stops at the edge of one conversation, so a room of agents
can pool what its members know and a *company* of them cannot. A desk is a
correlation boundary — members of one desk are wrong about the same things —
and no amount of deliberating inside a channel cancels an error every member
shares. See [`docs/specs/cross-desk-referral.md`](docs/specs/cross-desk-referral.md),
[ADR 0006](docs/adr/0006-a-referral-crosses-one-channel-at-a-time.md) and
[the federated experiment](docs/experiments/2026-09-02-federated-hidden-profile.md).

P14 is out of order on purpose. It is not a wire-format change and does not
wait on P11 through P13: it answers the same pressure they do — a bounded
window over an unbounded log — with the two mechanisms that need no new port
and no new stored state. Search makes the transcript queryable rather than
something a turn must hold, pinning keeps a small working set arriving whether
or not anybody asked, and `BrevityPolicy` states the budget every message is
spending out of. See [`docs/specs/recall.md`](docs/specs/recall.md).

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
fact-holder spoke before the commit in every room that decided and fourteen of
twenty were still wrong, no turn was ever awarded on `BidReason::Knows`, and
`!defer` was used on none of 266 turns.
See [`docs/experiments/2026-09-05-expert-delegation.md`](docs/experiments/2026-09-05-expert-delegation.md).

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
- **Fan-out.** One message triggers exactly one turn. `@everyone` is a list, not
  a broadcast — see P7. P8's deliberation episodes do not relax this: an episode
  is a bounded *sequence* of single turns, and `HiveStep::Speak` cannot
  represent two. Independence between participants is bought as a visibility
  filter on the projection, never as concurrency.
