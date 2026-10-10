# `conduct`

One completion-driven episode as a host steps it: the desk episode, a child
episode for every conversation an `ask` opens, and the rules between them
that no single fold can hold.

| file | holds |
| --- | --- |
| `mod.rs` | `Conductor`, `starters`; opening the desk, beginning a wave, proposing turns, opening a turn with its brief, recording what it called |
| `types.rs` | `ConductPolicy`, `Door`, `ConductorState`, conductor state fields: episode walls, opening inputs and resumable snapshots; private snapshot fields remain scoped to the conductor |
| `wave.rs` | After a wave: the phase machine that hands the host one `Step` at a time -- commits in conversations, silent askees, commits on the desk with their consequences, conclusions, the turn wall |
| `child.rs` | A conversation: its root, its asker and the seats it asked, its own driver state, its turns, its nudges; and one that concluded |
| `steps.rs` | `Turn`, `Note`, `Commit`, `Event`, `Refusal`, `Step`: the wire forms a host journals and streams |
| `test/` | Every rule, driven by a host that is only a journal; the exact wire forms; the links from a row to its conversation and from an event to its row |

The rules, each with the decision it comes from:

- **A conversation runs first** (ADR 0023): it is what unblocks a desk turn.
  It concludes when every seat asked completes, at `child_turn_wall` per seat
  asked, or when nothing is due anywhere; its outcome reaches the asker as a
  private row from each of them, which releases the asker's hold seat by seat.
  `ask_teammates` names a group, and then the group is one conversation whose
  seats read each other (ADR 0026). The seats that had it are shown it whole once,
  on their next desk turn.
- **Nudges** (ADR 0024): a desk seat that holds open work, ran for it and
  has been shown everything is told once per assignment and owed a turn. A
  seat asked that took its turn without answering is told once and owed a
  turn; a second silence stands.
- **Parking**: a turn that stopped on something only the host can settle
  -- an approval, typically -- is recorded with `record_parked` instead of
  its calls. The seat is held where it parked: not nudged for silence, not
  counted toward a stall, not proposed again, and a parked askee's
  conversation waits with it rather than concluding for want of a turn.
  Nothing due with a seat parked is a wait, not a stall; `parked` says who,
  and `resume_seat` puts the seat back in the next wave, owed a turn where
  it parked. `Event::Parked` and `Event::Resumed` mark both. A held seat
  also keeps `finished` false even where its own work closed some other way,
  because ending the episode there would strand whatever the host queued to
  hold it: the operator answers an approval with no loop to return to.
- **Checkpointing**: `snapshot` carries the wave in progress as well as the
  episode, so a host checkpoints after **every committed row** rather than
  once per wave, and a crash replays at most the one row whose sequence had
  not been reported yet. It answers `None` only while the host holds a
  commit it has not reported through `committed`: the conductor cannot say
  whether that row landed, so it writes no claim either way. `resume`
  rebuilds from a snapshot on a driver and a routing the host supplies
  again, and a caller drains `step` before proposing a new wave -- a
  restored wave is dropped otherwise, because `begin_wave` resets the phase.
- **Sorting**: a broadcast or an ask made inside a conversation is desk
  work; only a post or a completion is a row of the conversation.
- **Refusals**: a completion the ledger refuses is explained to the seat on
  the desk. A spent broadcast budget completes the seat with the work.
- **Walls**: turns per conversation, turns per episode.

The conductor appends nothing. It hands the host a `Note` to append, a
`Commit` to append and report the sequence of, or an `Event` to log, and
takes the sequence back through `committed`. The host owns the journal,
the rendering of a row, the prompt, and running the turn.

What a host reads back to draw the desk:

- **A conversation, whole.** The ask row's sequence is the conversation's
  root. Every other row of it carries that root as `Commit::conversation`:
  what was said inside it, desk work a seat lifted out of it, and the row
  that concluded it to the asker. `Event::Asked` and `Event::Concluded`
  mark when it opened and closed.
- **An event's row.** `Broadcast`, `Unplaced`, `CompletedByBroadcast`,
  `Refused`, `Discharged` and `Concluded` carry `at`, the sequence the host
  gave the row they are about; `Handoff` carries the broadcast row it came
  from as `origin`. A refused row is already on the journal, and its event
  is what marks it refused.
