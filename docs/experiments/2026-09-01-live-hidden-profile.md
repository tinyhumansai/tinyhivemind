# A live room on a real problem

Date: 2026-09-01. Branch: `hive-live-problem`.

What follows is a record of running `tinyhivemind-hive` with real agents on a
problem that has an answer, what it showed, and what should change because of
it. It asserts nothing about model quality; the sample is small and is stated
run by run so it can be read for what it is.

## Why the existing live mode was not enough

`--agent-cmd` already drove a live room, but over the synthetic brief `"We must
choose one rollout strategy for a risky migration. Decide together."` and a
room whose private information is a vector of numbers no agent is ever shown.
That measures one thing — whether a model can hold the trace grammar — and it
cannot measure the thing the protocol exists for, which is whether a room
pools information a single member does not have.

So live mode gained a `--scenario` file: a shared brief, a private brief per
member, the options with the ids the room should use, and a recorded answer.
The private briefs are deliberately *not* appended to the shared journal. A
fact every member can already read is not private information, and a room
whose members all start from the same facts has nothing to pool.

`--repeat N` runs the whole thing N times, because a live room is sampled
rather than computed and one episode is an anecdote.

## Designing a task the control cannot win

The control is the matched-budget vote: every member answers the same brief
alone and the room's answer is the plurality. Two scenario designs were thrown
away before one separated the arms, and how they failed is the most portable
finding here.

**Design 1 and 2 — the answer was in the option list.** Both made the correct
option `#retries`: a client retry storm after a release. That is the canonical
503 story, and a language model reaches for it unprompted. Every member solved
it alone and the vote scored **5 of 5 in both designs**. An arm that cannot
lose is not a control, and a scenario whose answer survives deleting every
private brief is not measuring deliberation.

**Design 3 — the seductive story is the decoy.** The brief itself plants the
retry storm, and the recorded answer is the boring one, `#pool`. Reaching it
needs four facts held by four different members:

- the pool caps at 20 and a request that waits 250ms is failed with a 503
  (archivist),
- each in-flight request holds one connection for its whole life (critic),
- in-flight requests have sat at 24–31 since 09:14, with traffic flat
  (auditor),
- the release quintupled the request timeout, so each one lasts five times
  longer (planner),

and one fact that kills the decoy: the retry path shipped disabled and has
never fired (scout). No member holds two of the four, and any one alone is
inert.

## What the rooms did

Five members, quorum 3, budget 15. One episode per row.

| agent CLI | hive | turns | vote plurality |
| --- | --- | --- | --- |
| `claude -p --model sonnet` | **#pool, correct** | 10 | #retries, wrong (3/5) |
| `opencode` → `openai/gpt-5-mini` | **#pool, correct** | 8 | #retries, wrong (4/5) |
| `opencode` → `openai/gpt-5-mini` | **#pool, correct** | 8 | #retries, wrong (3/5) |
| `opencode` → `qwen3-235b-a22b` | **#pool, correct** | 11 | tied 2–2–1, no answer |
| `opencode` → `gpt-5-mini` (revised prompt) | #retries, wrong | 9 | #retries, wrong (3/5) |
| `opencode` → `gpt-5-mini` (revised prompt) | #retries, wrong | — | — |

Four rooms out of six reached an answer no member could reach alone, at 8–11
turns of a 15-turn budget, in 45–75 seconds of wall clock. The vote never once
returned the recorded answer.

That is the result the harness was built to produce, and it is worth being
precise about what it is: evidence that *this* task shape separates the arms,
on a sample of six. It is not an accuracy claim.

## Six things the runs showed

### 1. The blind round is where the pooling happens

In every correct episode the five blind turns are five `!evidence` lines, one
per member, each depositing that member's private facts and nothing else. The
argument only starts once the room goes to `Visibility::Full`. The simulation
already scored the blind round at 24 points of accuracy; live rooms show the
mechanism behind that number, and it is not subtle — without it, the first
speaker's framing is in the transcript before anybody has stated a fact.

### 2. Support is counted; grounds are not weighed

This is the important one. In both failed rooms scout's refutation — the retry
path shipped disabled, zero retries fired today — was in the transcript, at a
sequence every later message could cite. It changed nothing. Members went on
depositing `!support #retries ^6`, and three grounded supporters carried the
decoy.

The library counts *distinct grounded supporters*. It does not, and as a pure
fold cannot, check that the cited message supports the claim. That is correct
as far as it goes, but the grammar gives a member no way to say the thing that
would have mattered: **this evidence refutes that topic**. `!object >N ^M`
silences one advocate message. Killing a hypothesis with a fact means
objecting to every advocate separately, one turn each, and the room runs out
of budget before it runs out of advocates.

A negative evidence-to-topic link — a `!refute #topic ^N`, deducting from or
capping a topic's standing rather than silencing one author — is the missing
move. It is a pure fold over the same traces and needs no port. Whether it
should gate quorum or only weight it is a design question, not a coding one,
and it should have an ADR before it has an implementation.

### 3. A member's fact can attach to the wrong hypothesis

The auditor's concurrency evidence — 24–31 in flight, traffic flat — is
*compatible* with both the true cause and the decoy. In the failed rooms the
auditor deposited it and then proposed `#retries` from it, and every later
member cited that message as grounds for the decoy. Pooled information does
not help if it lands under the wrong heading, and nothing in the protocol
notices. This is a real limit on what quorum-over-grounded-support can do, and
it belongs in the benchmark write-up's "what this does not show" section.

### 4. Cross-inhibition fires, and it fires against the truth

The one live `!object` observed across every run was the auditor objecting to
`#pool` — the correct option — on the argument that raising the pool masks a
duration problem. Cross-inhibition worked exactly as specified and removed a
supporter from the right answer. The mechanism is not wrong; it is neutral,
and a benchmark that only reports its wins is not reporting it.

### 5. The prompt teaches placeholder syntax by accident

`gpt-5-mini` copied the prompt's `<one sentence>` placeholders into its output
verbatim: `!propose #retries <one sentence>Turning off the client retry…`. The
angle brackets survive into the transcript and into every later citation of
it. The placeholders were replaced with plain prose descriptions, and an
explicit "angle brackets are not part of any line" was added.

`claude -p --model sonnet` produced the other prompt failure:
`!support #retries ^5 … points at #pool instead` — a support marker on one
topic whose prose argues for another. The library counted a supporter for
`#retries` that the author plainly did not intend. The protocol text now says
the marker is what the room counts and the prose is not.

**Neither fix is verified.** The two rooms run after the prompt change both
landed on the decoy, against four out of four before it. Two runs is not
evidence that the change hurt, and the two are not paired against the four —
but it is also not evidence that it helped, and it should not be written up as
a fix until it has been run properly. This is the open item.

### 6. The vote control was quietly cheating

The qwen room's poll came back `#pool, #retries, #retries, #rollback, #pool` —
two, two and one. The harness reported "plurality #pool — correct", because
the tally was sorted stably and `#pool` had been inserted first. A tie is not
a decision, and resolving it by arrival order hands the control a win it did
not earn. `plurality` now returns `None` on a tie and the round is scored as
no answer.

## Changes made

- `examples/bench/scenario.rs` — the scenario file format, the shared brief,
  and the per-member private brief.
- `examples/bench/scenarios/checkout-503.txt` — design 3, with the two failed
  designs recorded in its header comment so they are not rediscovered.
- `examples/bench/live.rs` — private briefs in the prompt, the fixed-id
  naming rule when a brief already names the options, the placeholder and
  marker-versus-prose fixes, and `poll`, the independent-vote control run
  against the same real agents.
- `examples/bench/main.rs` — `--scenario`, `--repeat`, honest tie handling.

## Open items

1. Run design 3 at `--repeat 10` per model, before and after the protocol
   prompt change, and settle whether the change helped, hurt, or did nothing.
   Nothing in item 5 should be written up until this exists.
2. An ADR for a negative evidence-to-topic link (item 2), then an
   implementation and a simulated arm for it, so it is scored against the
   matched-budget vote like everything else.
3. `--scenario` is untested beyond one file. The parser has no unit tests
   because examples do not run them; if the format outlives the experiment it
   belongs in the crate rather than in an example.
4. The OpenRouter account used here permits only the `openai`, `deepinfra` and
   `streamlake` providers, so `google/gemini-2.5-flash` failed with a provider
   error the harness surfaced as `opencode exited with exit status: 1`. Live
   mode should say what the CLI printed on a failure rather than only its
   status.
