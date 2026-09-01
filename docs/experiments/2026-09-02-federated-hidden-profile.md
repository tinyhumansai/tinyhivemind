# Several channels, one problem

**Date:** 2026-09-02
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — `--swarm`
**Spec:** [`docs/specs/cross-desk-referral.md`](../specs/cross-desk-referral.md)
**Decision:** [ADR 0006](../adr/0006-a-referral-crosses-one-channel-at-a-time.md)

## What this asks

Every mechanism through P14 stops at the edge of one conversation. A room of
agents can pool what its members know; a *company* of them cannot. This measures
what that costs, and what closing it buys.

The claim under test is narrow and worth stating before the numbers: **a desk is
a correlation boundary.** Members of one desk read the same transcript, work the
same part of the system, and are wrong about the same things. Averaging
correlated error does not remove it, so no amount of deliberating inside a
channel cancels a mistake every member of it shares. If that is true, then
crossing a channel is not a convenience — it is the only operation that helps,
and the size of the effect should be enormous rather than marginal.

It is, and it is.

## The simulated federation

Three desks of four members decide between four options. Every member holds a
private, noisy evaluation of every option, exactly as in the single-room
benchmark, plus one thing that benchmark does not have: **each desk overrates
one option, and each desk a different one.** Within a desk that bias is
invisible — every member confirms every other. Across desks the biases are
independent and cancel.

`--bias` is bounded on both sides and both bounds matter. Above the 60-point gap
between the true option and a decoy, a desk's own average points at the wrong
answer. Below roughly `60 × desks`, the biases still cancel once every desk has
heard every other. The default of 110 at three desks sits inside that window
with room on both sides, and the sweep below shows what happens outside it.

## The arms

| arm | what it is |
| --- | --- |
| `siloed` | The same desks, members and budgets, referrals off. |
| `swarm` | The same, referrals on: two hops, desk mentions, returns. |
| `pooled` | Every desk handed every other desk's readings **for free** — no turn, no referral, no channel crossed — then deliberating siloed. |
| `merged` | Every member on one desk, given the whole federation's budget. |
| `vote` | One independent answer per member, decided by plurality. |

`pooled` is the arm that keeps the rest honest. The swarm's members exchange
numeric readings, which the siloed members never get a chance to, and a reader
is entitled to ask how much of the difference is the *protocol* and how much is
simply having the numbers. Whatever `pooled` scores is what the information is
worth; whatever `swarm` scores below it is what the boundary still costs.

Every arm is charged for every agent invocation, including the ones a referral
causes on the far desk and on the way back. A member that spends its turn asking
another desk does not also get to argue in its own that turn.

## Results

400 federations, three desks of four, four options, bias +110, noise ±50.

| arm | correct | decided | agent turns | crossings |
| --- | --- | --- | --- | --- |
| `siloed` | 0.2% | 1 | 15.9 | 0.0 |
| `swarm` | **77.5%** | 389 | 32.3 | 12.0 |
| `pooled` | 74.5% | 371 | 16.7 | 0.0 |
| `merged` | 10.5% | 96 | 33.8 | — |
| `vote` | 4.0% | 141 | 12.0 | — |

Three things in that table are worth more than the headline.

**`siloed` is not merely worse, it is destroyed.** Every desk converges — 1,199
of 1,200 desk episodes reach a recorded decision — and converges *confidently*
on its own decoy. Three confident desks disagreeing three ways produce no
plurality at all, which is why `decided` is 1. A federation of well-run rooms
that cannot talk to each other does not degrade gracefully; it produces three
firm, incompatible answers.

**`merged` is the surprise, and the most useful result here.** Putting all
twelve members on one desk — removing the boundary rather than crossing it —
scores 10.5%, not 77.5%. A larger room with three factions cannot assemble a
majority quorum, so most episodes exhaust. "Just put everyone in one channel" is
not the fix, and it costs the same turns as the swarm.

**`swarm` matches `pooled`.** 77.5% against 74.5%, and the ordering holds across
the whole sweep below. The protocol delivers essentially the entire value of the
information; what it costs is turns, not accuracy. (The swarm slightly exceeds
the free-information ceiling because its readings arrive from members who have
already pooled once, so the second exchange carries a partly-averaged number.
That is a second-order effect, not a claim.)

### The bias sweep

300 federations at each point. This is the arm that had to be able to lose, and
at both ends of the range it does.

| `--bias` | `siloed` | `swarm` | `pooled` | `merged` | `vote` |
| --- | --- | --- | --- | --- | --- |
| 0 | 99.7% | 100.0% | 100.0% | 100.0% | 100.0% |
| 60 | 59.3% | 92.7% | 95.3% | 93.0% | 83.0% |
| 110 | 0.0% | 78.3% | 76.0% | 11.0% | 4.0% |
| 160 | 0.0% | 35.7% | 35.7% | 0.0% | 0.0% |

At **bias 0** — desks with no blind spot of their own — crossing a channel
changes no answer and costs twice the turns. This is the honest "do not turn it
on" case, and it is why every knob in `ReferralPolicy::DEFAULT` is off.

At **bias 160** the information is no longer sufficient: pooling three desks
leaves the decoy at 93 against the true option's 100, a margin the individual
noise swamps. `swarm` and `pooled` score identically, 35.7% each, which is the
cleanest statement in the whole experiment that the protocol is delivering
everything there is to deliver and the shortfall is the problem's.

### Across desk counts and desk sizes

300 federations at each point, at the default bias.

| shape | `siloed` | `swarm` | `pooled` | `merged` |
| --- | --- | --- | --- | --- |
| 2 desks × 4 | 0.0% | 25.0% | 18.3% | 4.3% |
| 3 desks × 4 | 0.0% | 78.3% | 76.0% | 11.0% |
| 4 desks × 4 | 1.0% | 58.7% | 56.3% | 3.7% |
| 3 desks × 3 | 0.0% | 75.7% | 72.3% | 13.3% |
| 3 desks × 6 | 0.0% | 78.3% | 78.3% | 9.3% |

Two desks is weak for an arithmetic reason rather than a protocol one: pooling
two readings of a biased option halves the bias, leaving 95 against 100, and the
margin is inside the noise. Four desks is worse than three at a *fixed* bias
because the recoverable window widens with the desk count — at four desks a bias
of 110 is comfortably inside it, so more of the difficulty is elsewhere. Desk
size barely matters, which is the expected result: the bias is shared by the
desk, so adding members to it adds no independent information.

Three seeds at the default shape give 77.5%, 76.0% and 75.5% for `swarm` against
0.2%, 0.8% and 0.2% for `siloed`, so none of this is a seed artifact.

## The largest effect is not in the library

It is *when* a member asks.

The first version of this harness had members ask for an outside reading after
proposing their own option, which sounds more natural and is what a person
would do. Every desk committed to its own decoy anyway, with the correction
sitting three lines below the decision.

The reason is specific and generalises: **a desk whose members share a bias
reaches quorum inside its own blind opening round.** Four members who all
privately favour the same option deposit four proposals before anybody has seen
anybody, which is already four distinct supporters, which is already quorum. The
episode's first non-blind turn is a commit turn. There is no window in which an
arriving fact can change anything.

Moving the question to *before* the desk has backed anything is the difference
between the whole thing failing and 77.5%. That is a host policy decision the
library cannot make — `referral` decides where a turn goes, not when a member
should want one — and it is documented as a host obligation rather than buried
in a harness.

A related, smaller effect: an answer can be **stranded**, arriving after the
desk that asked has already closed. The harness counts those rather than
dropping them quietly, because they are compute the federation paid for and
could not use. At the default shape it is 0.0 per federation; at bias 160, where
desks take longer, it rises.

## What the simulation does not show

The participants are arithmetic. They do not misread a question, coin a second
name for an option, or answer a different desk from the one that asked. Nothing
here is evidence that language models do any of this well — that is what the
live arm is for, and the live arm's finding is a different one.

The federation is also generous in one specific way: every desk is wrong about a
*different* option. Two desks sharing a blind spot would agree with each other
for the wrong reason, and pooling would confirm rather than correct. That is a
real failure mode, it is not measured here, and a host whose channels are
correlated with each other should not expect these numbers.

## The live federation

`checkout-503-federated.txt` is the single-room hidden profile from
[the earlier live run](2026-09-01-live-hidden-profile.md) with its facts split
across three desks, so the conjunction that makes `#pool` the answer is not
merely spread across members — it is spread across rooms. Each desk also holds a
fact pointing confidently at a different wrong option, and each of those decoys
is refuted only by a fact on another desk.

In this arm the harness writes no mention on anybody's behalf. Each agent is
told which channels exist and how to address one, and decides for itself whether
to spend a turn asking.

### Run one: nothing crossed

`claude -p --model sonnet`, nine agents on three desks.

Two of the three desks reached `#pool`, the federation's plurality was `#pool`,
and the matched-budget poll of the same nine agents returned `#retries` — wrong,
7 of 9. On the scoreboard the federation beat the poll.

**And not one message crossed a channel.** Across twenty-seven turns, no agent
addressed another desk.

Two things were wrong, and both were mine.

The **scenario** was not a federated hidden profile. It gave the Data desk both
"the pool caps at 20" and "the database would accept 400", which is very nearly
the answer written down. Two desks solved it alone, so the run proved nothing
about crossing a channel — the same mistake, in a new place, that the first two
designs of the single-room scenario made.

The **prompt** offered a move it had already forbidden. The cross-channel
instruction sat above the protocol block, so the last thing the model read was
still *"reply with ONE line only, beginning with exactly one of these markers"*
— and `@#platform` is not one of those markers.

### Run two: the scenario fixed, still nothing crossed

Each desk now holds a decoy fact refuted only from another desk. The desks split
three ways — Platform on `#retries`, Data on `#pool`, Release on `#rollback` —
and the federation returned **no answer**, which is the correct behaviour for
three desks that disagree and is exactly the `siloed` failure the simulation
predicts.

Still zero crossings. The fix at that point had only moved the cross-channel
block *below* the protocol, not *into* it.

### Run three: the move in the list of moves

`@#deskid` is now a line in the marker list itself, with its rules beside the
rules for the other markers. **Agents crossed** — three messages, and the round
trip worked exactly as specified: `data-lead` asked `@#platform`,
`platform-lead` answered on the Platform desk, and the answer came home as one
line, which `data-lead` relayed into its own transcript. The mention was read by
the real grammar and routed by the real fold; nothing about the path was
special-cased.

The answer was worse.

```text
Data      4  data-lead     @#platform is the API's request timeout longer than 250ms …
Platform  5  platform-lead !evidence … that's the wrong lever regardless — the pool is
                           exhausted by connection-hold-time under retry-driven
                           concurrency (^2, ^3) … which still points at #retries over #pool.
Data      5  data-lead     !evidence @#platform said … pointing at #retries over #pool.
Data      6  data-lead     !support #retries ^5
```

Data asked the wrong question, got a **hypothesis instead of a fact**, and
adopted it. Platform's decoy crossed the channel intact and took the one desk
that was reasoning correctly with it. The federation decided `#retries`, and so
did the poll. Both wrong. A second `@#platform` from `data-dba` arrived after
Platform had converged and was **stranded** — the simulated failure mode, in
prose.

The harness defect behind that is mine and is fixed. The prompt for an answering
turn was built from scratch and never included the answering agent's **own
private facts** — it was handed the question and its desk's transcript and
nothing else. So the one agent that was asked a direct question was structurally
unable to answer it with anything but an argument. It argued.

### Run four: correct facts crossed, and the room still got it wrong

Six crossings, all three desks participating, before that fix. This time the
facts that crossed were *right*: `platform-sre` asked `@#release` whether the
retry path was enabled, and Release answered accurately — never enabled
anywhere, zero retries fired today, and the timeout moved 2s→10s independently.

Platform relayed that home and then committed `#retries` anyway, four turns
later, on the grounds that "no counter-evidence to ^10's timeout finding has
been offered". It had relayed evidence that `#retries` fixes nothing and then
shipped `#retries`. All three desks landed there. Wrong.

Nothing in the protocol failed. The message went, the answer came back, the room
read it, and the room reasoned badly.

### Run five: the answering prompt fixed

Six crossings again, and this time one of them did the job the whole mechanism
exists for:

```text
Data      7  data-dba      @#platform can you tell me how many concurrent requests
                           checkout API instances are holding open per instance and
                           average connection hold time since 09:14 …?
Platform  8  platform-lead !evidence in-flight checkout API requests have held 24–31
                           concurrent per shared pool since 09:14 vs an 11 baseline, and
                           each in-flight request pins one database connection for its
                           full lifetime …
Data      8  data-dba      !evidence ^7 in-flight … 24–31 concurrent … vs an 11 baseline …
Data      9  data-analyst  !support #pool ^8 concurrent in-flight requests jumped from an
                           11 baseline to 24–31 while traffic stayed flat per ^4, so the
                           pool cap of 20 is now genuinely undersized for load …
Data     10  data-dba      !commit #pool ^9
```

That is the experiment working. The Data desk holds the cap and cannot see the
in-flight count; it asked the desk that can, received **the number rather than a
conclusion**, and reached `#pool` — an answer no member of that desk could have
reached alone and no amount of deliberating on that desk could have produced.

The federation still did not agree. Platform converged on `#retries`, never
having asked Release the one question that would have killed its own decoy;
Release exhausted; Data reached `#pool`. One each, no plurality, **no answer**.
The poll of the same nine agents returned `#retries`, wrong.

### What the live arm shows, and does not

Five runs is not a rate, and each is a single episode. What they establish is
narrow, and only the first three points are about this library:

- **The mechanism works end to end with real agents.** A desk mention written in
  ordinary prose routes one turn onto another channel, one answer comes home,
  and the hop budget closes the trip. Runs three, four and five each did it.
- **Agents will not use a move that is not in the list of moves.** Two runs and
  fifty-odd turns produced zero crossings while the move was explained above the
  marker list rather than placed in it. That is a host-prompt obligation, and it
  is now on the [Host integration](../../wiki/Host-integration.md) page.
- **An answering turn needs the answerer's private facts.** Without them the
  only thing a desk can send across a channel is its opinion.
- **Crossing a channel is not automatically an improvement.** What crosses is
  whatever the far desk chose to say, and a hypothesis crosses as easily as a
  fact. Run three exported an error; run five exported a number. The difference
  was entirely in the prompt.
- **The protocol moving messages does not make a room reason.** In run four
  every fact needed to rule out `#retries` reached the desk that shipped
  `#retries`.

The simulation measures what pooling readings is worth. The live arm measures
whether real agents will ask, and what they send when they do — and on the
evidence here, both are the host's problem to get right, not the library's.

One incidental finding worth recording: agents wrote `@#release` and `@#data`
*inside* `!question` lines, mid-sentence, and the fold routed them. That is the
documented rule — the lowest-offset nonquiet candidate wins — but a host should
know that a desk mention buried in a sentence still spends a turn on another
channel.

## Reproducing

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --swarm
cargo run --release -p tinyhivemind-hive --example bench -- --swarm --trace --episodes 1
cargo run --release -p tinyhivemind-hive --example bench -- --swarm --bias 0
cargo run --release -p tinyhivemind-hive --example bench -- --swarm \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503-federated.txt \
  --agent-cmd "claude -p --model sonnet"
```

| run | crossings | federation | poll |
| --- | --- | --- | --- |
| one — move above the list, leaky scenario | 0 | `#pool`, correct | `#retries`, wrong |
| two — scenario fixed | 0 | no answer | — |
| three — move in the list | 3 | `#retries`, wrong | `#retries`, wrong |
| four — variance sample | 6 | `#retries`, wrong | `#retries`, wrong |
| five — answering prompt fixed | 6 | no answer (Data reached `#pool`) | `#retries`, wrong |

The simulated arms are seeded and reproduce exactly. The live arm does not, and
one run of it is an anecdote.
