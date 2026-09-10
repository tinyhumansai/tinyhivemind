# Several channels

How a federation of desks reaches across a channel boundary, what a crossing
costs, and what a live federation prints. Split out of
[`README.md`](README.md) when that file passed the repository's 500-line cap;
nothing here changed with the move.


`--swarm` runs a different experiment on the same machinery: not one room
deciding, but a **federation** of desks that cannot read each other's
transcripts and can only reach one another by a `referral`.

The task changes shape to make the boundary cost something. Each desk carries a
bias of its own — one option every member of that desk overrates, because they
read the same transcript and are wrong about the same thing. Within a desk that
bias is invisible: every member confirms every other, and averaging correlated
error does not cancel it. Across desks the biases are independent and do. So
the answer is reachable only by pooling across channels, which is the
multi-channel form of the hidden profile the live scenarios use, written in
numbers so it can be run ten thousand times.

`--bias` is bounded on both sides, and both bounds matter. Above the 60-point
gap between the true option and a decoy, a desk's own average points at the
wrong answer, so no amount of deliberating inside one channel finds the right
one. Below roughly `60 × desks`, the biases still cancel once every desk has
heard every other. Outside that window the experiment measures nothing, which
`--bias 0` and `--bias 160` both demonstrate.

| arm | what it is |
| --- | --- |
| `siloed` | The same desks, members and budgets, with referrals off. A desk can only talk to itself. |
| `swarm` / `swarm°` / `swarm◦` | The same, with referrals on: two hops, desk mentions and returns. `swarm°` asks **off the floor** under `--ask-cap` rather than spending the authorized turn — what keeps a federation above eight desks deciding at all — and `swarm◦` also publishes each desk's reading to every channel under `--digest`, the only bounded arm left standing once desks share blind spots. See [`SCALE.md`](SCALE.md#correlated-desks-and---digest). Under `--evidence` the same arms also carry what a desk can **disqualify**, not only what it scores — which is what closes `swarm◦`'s gap to `pooled`. See [`SCALE.md`](SCALE.md#--evidence-what-a-channel-carries-not-how-wide-it-is). |
| `pooled` | The ceiling control. Every desk is handed every other desk's readings — and, under `--evidence`, facts — *for free*: no turn, no referral, no channel crossed. Then deliberates siloed. |
| `merged` | Every member of every desk on one desk, given the whole federation's budget. The control that removes the boundary rather than crossing it. |
| `vote` | One independent answer per member, decided by plurality. |

`pooled` is the arm that keeps the swarm honest. The swarm's members exchange
numeric readings, which the siloed members never get a chance to, and a reader
is entitled to ask how much of the difference is the *protocol* and how much is
simply having the numbers. Whatever `pooled` scores is what the information is
worth; whatever `swarm` scores below it is what the channel boundary still
costs after `referral` has done its work.

Every arm is charged for every agent invocation, including the ones a referral
causes on the far desk and on the way back. A member that spends its turn
asking another desk does not also get to argue in its own that turn.

## How a member decides to cross

A simulated member asks one question of each peer channel, before its desk has
backed anything: *I will not put an option on the floor on the strength of what
my own desk thinks, when nobody outside it has told me anything about it.*

The timing is the whole ballgame, and an earlier version of this harness got it
wrong. It asked *after* proposing — which sounds more natural — and every desk
committed to its own decoy with the correction sitting three lines below the
decision. A desk whose members share a bias reaches quorum inside its own blind
opening round, and an answer arriving after that is information the desk has
already voted past.

Asks are counted **per desk**, not per member. The answer lands in the desk's
own transcript where every member reads it, so a second member asking the same
desk the same question spends a turn to learn what it could have read.
`referral` bounds how deep a chain goes; bounding how wide one desk may go is
the host's job.

What crosses is **information, never a vote**. The message that lands on the
far desk is `!evidence`, which adds no supporter to any topic: the members
there hear another channel's reading, average it into their own, and still have
to spend their own turns saying so before anything is counted.

## A live federation

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --swarm \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503-federated.txt \
  --agent-cmd "claude -p --model sonnet"
```

The scenario file grows `[desk ...]` sections and a `desk:` line per agent.
`checkout-503-federated.txt` is the single-room hidden profile with its facts
split across three desks, so the conjunction that makes the answer is not
merely spread across members — it is spread across *rooms*.

In the live arm the harness writes no mention on anybody's behalf. Each agent
is told which channels exist and how to address one, and decides for itself
whether to spend its turn asking. The line it writes is read by the real
mention grammar and routed by the real `referral` fold, exactly as the
simulated ask is. A run in which nothing crosses is a finding about the agents
rather than a failure of the harness.

