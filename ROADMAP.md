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

The next work is the paired OpenCompany adapter integration, followed by a
gated live-provider verification in which two agents exchange an attributed
turn. The adapter initially remains disabled and uses two hops when enabled.
The hive crate is opt-in and is not part of that first adapter.

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
