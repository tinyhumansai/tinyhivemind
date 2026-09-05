# The live matrix, in full

Companion to
[`2026-09-05-expert-delegation.md`](2026-09-05-expert-delegation.md#the-live-arm):
the full ten-row live matrix, its per-row narrative and the federated round,
split out to keep that report at or below the 500-line cap.

## The live matrix

Ten rows, twenty-seven rounds, two hundred and sixty-six agent turns. Every row ran
both arms: the room, and the matched-budget poll of the same seats through the
same backend. `expert` is rounds in which the scenario's `truth_expert` spoke
before the commit, over rounds whose commit chain reaches something it said.
Tokens and cost print for the HTTP backend only, in the harness's unit — 1 per
1000 tokens times the model's price, so a `reasoning` round costs ten times a
`flash` round of the same length.

| row | backend | rounds | hive ✓/decided | poll ✓ | turns/ep | s/ep | tokens/ep | cost/ep | expert | `!defer` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `checkout-503` | HTTP `flash` | 3 | **3 / 3** | 0 / 3 | 7.3 | 451 | 27,543 | 24.7 | — | 0 |
| `index-lock-expert` | HTTP `flash` | 3 | 1 / 2 | 0 / 3 | 11.7 | 842 | 48,483 | 46.0 | 3 / 2 | 0 |
| `index-lock-expert` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 7.0 | 322 | 19,999 | 197.7 | 3 / 1 | 0 |
| `index-lock-expert` | `claude -p --model flash` | 3 | **3 / 3** | 0 / 3 | 13.3 | 697 | — | — | 3 / 2 | 0 |
| `index-lock-expert` | `opencode run -m ladder/flash` | 3 | 1 / 2 | 0 / 3 | 12.3 | 374 | — | — | 3 / 1 | 0 |
| `index-lock-expert` | `codex exec` → `deepseek/deepseek-v4-flash` | 3 | 0 / 2 | 0 / 3 | 12.3 | 305 | — | — | 3 / 0 | 0 |
| `index-lock-tiers` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 6.0 | 318 | 16,865 | 166.0 | 3 / 1 | 0 |
| `index-lock-tiers` | HTTP, every seat `reasoning` | 2 | 0 / 2 | 0 / 2 | 7.5 | 489 | 20,858 | 207.0 | 2 / 1 | 0 |
| `checkout-503-federated` | HTTP `flash`, `--swarm` | 1 | 0 / 1 | 0 / 1 | 15 | 764 | 31,192 | 28.0 | — | 0 |
| `index-lock-tiers` | HTTP, four `flash` seats + `dba` on `reasoning` | 3 | 1 / 3 | 0 / 3 | 8.7 | 363 | 32,979 | 108.7 | 3 / 3 | 0 |

The last row is the corrected mixed-tier run, the harness defect described
below now fixed: only `dba` runs on `reasoning`, the other four seats stay on
`flash`. Round 1 committed `#batch` (correct) in 14 turns, 152 units, 48
cheap / 104 reasoning; rounds 2 and 3 both committed `#rollback` (wrong) in 6
turns each, at 108 and 66 units. `dba` spoke before every commit and was cited
by all three; it wrote 17–43% of a round's tokens for 54–90% of its cost, and
the one correct round was also the longest.

**The poll never found the answer, in any row** — twenty-seven rounds, wrong or
tied every time. On `index-lock-expert` and `index-lock-tiers` it returned
`#rollback` or `#archive`; on `checkout-503`, `#retries` three times in three,
the decoy the brief plants.

**The fact-holder spoke before the commit in every room that had one** — 23 of
23 rounds across eight rows, at turn 4 in eighteen of the first twenty. Q1's
live answer is yes, and the evidence-first opening buys it. **And fourteen of
those twenty-three rooms still got it wrong.** Reaching the holder is not the
bottleneck; weighing what it says is.

**`!defer` was never used.** Not one of the 266 turns, in any harness, on any
row, although `!defer #topic` sits in the move list every participant reads,
with its rules beside the markers they did use. The federated runs established
that a move *outside* the list is never used; this establishes something
narrower and less comfortable — a move inside it is not necessarily used either.
A model asked for one line still prefers a position on a topic it knows nothing
about to saying so, and the simulated arms that priced `!defer` were pricing a
move real participants did not make.

**The directory was in the prompt and never won a turn.** `directory_block`
renders into every deliberation prompt from the moment any topiced trace exists
— one `#topic: agent weight (spec N, cred N)` line per contested topic, holders
in descending weight — empty only on the blind first turn and absent from the
commit prompt. Across all 266 turns none was awarded on `BidReason::Knows`; the
reasons recorded are `salience`, `dissent`, `addressed` and `quiet`. Same
structural reason as the simulation: by the time a topic is contested, its top
holder has taken a position on it.

**The reasoning model on the expert seat did not help, and the rooms with it
lost faster.** Both `--specialist-model` rows converged on the decoy inside
seven turns — `index-lock-tiers` in six, `index-lock-expert` in seven — well
under the thirteen-plus turns the same scenario took on `flash`, because those
rooms had no deliberation phase at all: three of the five blind opening turns
were `!propose #rollback`, which is quorum, so the first non-blind turn was a
commit turn. What `dba` said in them is the finding. It never stated its
numbers — in all five rounds it spent its blind turn proposing `#rollback`,
"the archive and reconciliation jobs are unchanged from their year-long
pattern", turning the two batch sizes into an argument that they are *not* the
cause, exactly the non-event reading the scenario header predicts of a member
holding them. `scout`'s threshold was on the floor in the same blind round;
nobody held the two against each other, because by the time anyone could read
both the room had carried `#rollback`.

**`claude` scored 3 of 3 on a scenario the same model lost through three other
harnesses.** Same ladder, same `flash` behind it; what differs is the agent
around it, and the transcripts differ in three ways. The `claude` rooms are
longer — 13.3 turns and 697 s an episode against 7.0 and 322 s for the HTTP
specialist row — and spend the extra turns killing the decoy: `planner` deposits
the forward-only rollback fact and two members `!object` to `#rollback` on it,
so `#rollback` is dead before anything else is settled. They carry `#batch` on a
chain of `!support` lines citing prior turns by sequence, and commit on that
chain rather than restating a position. And — the honest caveat — they did not
do the arithmetic the scenario was built around: `scout` names the
reconciliation job in its *blind* turn, before any threshold or size is on the
floor, and the room converges on a guess that happens to be right. The room that
visibly performed the intended inference is an `opencode` one, where `dba` put
both commit sizes on the floor and used them to refute its own `#archive`
support — and that round **exhausted** at 15 turns without a commit.

**On cost, the live rows say what the simulation said, more bluntly.** The two
`flash` rows that decided anything cost 25 and 46 units an episode; the three
rows with a reasoning model cost 166, 198 and 207 and scored **0 correct in 8
rounds**. One caveat is load-bearing and was a harness defect, now fixed:
`is_specialist` was true for any seat with an `expert_on` line, and every seat
in both scenarios has one, so `--specialist-model reasoning` put the whole room
on `reasoning` rather than `dba` alone. **So this matrix did not test the
mixed-tier claim at all** — it compared flash-only rooms against all-reasoning
rooms, and the cheap rooms won.

Seating by tier fixed the defect (the corrected row above). That run scored 1
correct in 3 rounds at 108.7 units — better than either buggy all-reasoning
row, still costlier than `flash`-only without clearly beating it on accuracy.
One round is not a rate, but a specialist's presence alone did not fix a room
that never weighs what it hears — `dba` spoke before and was cited by every
commit, including the two it got wrong.

**The federation completed one round and decided wrongly.** Platform and Data
converged on `#retries`, Release on `#scale`, the plurality was `#retries`, and
the poll of the same nine returned `#retries` too — both wrong, and the truth
`#pool` was nobody's. Two messages crossed a channel and nothing was stranded.
What crossed was accurate and useless: `data-lead` asked `@#release` whether the
retry path was active, and Release answered — shipped disabled, zero retries
fired today, the release moved the client timeout 2s→10s — then said the same
on its own desk. `release-sdk` read it and supported `#scale`; `data-dba` read
it and committed `#retries`. That is run four of the federated experiment again
on a different model: the protocol moved the evidence and the rooms reasoned
past it.
