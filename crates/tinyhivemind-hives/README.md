# tinyhivemind-hives

Durable coordination of host-supplied agents across dynamic hives. A hive and
a desk have one identity. Each registered agent has one continuing session and
one globally serialized turn stream, even when it belongs to several hives.

Use `Coordinator` with `MemoryStorage`, default-feature `SqliteStorage`, or a
host implementation of the async `Storage` port. A commit writes one bounded
state row and appends only the new transcript rows; `RetentionPolicy` bounds
settled episodes and acknowledged deliveries. Mutating APIs are `async` and
executor-neutral; commits run outside the live lock and reload on a conflict
with another process. The host supplies `AgentRunner` handles from
one runtime; this crate never constructs agents or serializes live handles.
The OpenHuman-specific boundary lives in `tinyhivemind-openhuman`.

Register an existing conversation atomically with
`register_agent_in_session(registration, session_id)` before a live scheduler
claims it. Hosts can create empty hives and join/leave registered agents while
the scheduler runs; unstarted removed seats are retired before claiming.
An active turn retains its captured membership.

The host reads everything with `read_transcript`, including replies to
`send_as_host`, and watches `subscribe()` (the committed revision) and
`episodes()` for settlement. `SendMessage::starters` chooses who starts an
episode without hiding the message, and `release_with` hands a parked agent
a note on its next turn.

A host serving tools out of process identifies each state-changing call with
a `TurnCall` and uses the `*_in_turn` entry points: a retry is answered from a
record written with the effect, and a call naming a turn that is not running is
refused.

Direct sends return durable receipts without awaiting peers. `read_direct`
exposes only the caller/peer pair, including returned replies, and starts no
turn. Hive reads enforce membership and private thread audiences.

Storage snapshots contain no live handles. On restart reattach runners with
durable agent IDs from the newly constructed runtime. Pending work resumes;
previously running work becomes interrupted without automatic replay.
Failed finalization carrying a valid completed session retains that binding
while suppressing acknowledgements and staged output.

See [source modules](src/README.md), the
[accepted specification](../../docs/specs/dynamic-hives.md), and the
[repository overview](../../README.md).

Start with the [offline coordinator examples](../../examples/hives/README.md) to see a
reviewer handoff and one continuing agent session across three hives.
