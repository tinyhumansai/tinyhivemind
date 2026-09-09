# Run 29: two turns of real work, delivered as narration

**Date** 2026-09-09
**Status** Recorded — aborted after 2 turns
**Code** `cargo run --release -p tinyhivemind --example desk` on `utterance-surface`,
with `--desk crates/tinyhivemind/examples/desk/desks/pe1006.txt --window 12
--rounds 8 --max-turns 40 --chair-every 6 --fold-tokens 50000 --no-memory`,
one `opencode run … -m ladder/max-reasoning` per turn.
**Spec** [`../specs/the-utterance-surface.md`](../specs/the-utterance-surface.md)
**Sample** one run, stopped at turn 3. Read it as a defect report, not a result.
Run 30, on the fixes below, is the rerun.

## What it was for

The one open criterion of
[`folding-by-size.md`](../specs/folding-by-size.md): no live run has ever
folded the room's standing account. Run 28 closed at 23 rows against a row
threshold of 32, so the account has been built, shipped, and never once
exercised. `--fold-tokens 50000` exists to make that impossible to repeat, and
this run was the first to carry it.

It did not get far enough to fold. It found two other things instead.

## What happened

| turn | seat | wall clock | tokens | tool calls | files written | reached the room |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `@theory` | 1046s | 63,440 | 25 | 4 | narration |
| 2 | `@solver` | 1024s | 68,359 | 22 | 0 | narration |

Both turns did real work. Neither said anything.

The row `@theory` left at `^2` is

> Found it — the actual PE 1006 says **sum of their squares**, not sum of
> values. Let me recompute and verify. Confirmed: Ψ(3)=20302, Ψ(10)≡10699667.

and `@solver`'s at `^5` is

> Let me verify the small cases and understand the structure better.

Those are mid-turn commentary, not messages. Neither seat called `desk_post`;
the tool lists for the two turns are 47 calls of `bash`, `read`, `write` and
`webfetch` with no `desk_*` among them. Because narration names no peer,
`dispatch_mention` refused twice with `NoDirectAgentMention` and the chair
nudged `@solver` on both rounds — the room could not hand a turn on.

## The two defects

### 1. The router advanced a rung under a thinking model — theirs

Both turns carried

```text
400 {"error":{"message":"The `reasoning_content` in the thinking mode must be
                        passed back to the API.","type":"invalid_request_error"}}
```

`max-reasoning` is a *ladder*, not a model: it fans
`deepseek-v4-pro` → `glm-5.3` → `glm-5.2`. The router's own log shows **8 rung
advances during the run window**. When it advances mid-conversation, the next
provider receives the accumulated history; a thinking model that emitted
`reasoning_content` requires it echoed back, an ordinary OpenAI-dialect client
does not keep the field, and every later turn of that conversation is refused
the same way. The session dies where it stands.

Fixed upstream in `llm-ladder-router` (PR #16, `fix(surplus): advance on
missing reasoning content`): the 400 is classified `Advance` rather than
returned, so the rung beside it takes the identical body. Verified after
redeploying: a three-step tool loop plus a `desk_post` now completes with zero
rung failures.

The general lesson is one this workspace already had in another form. A ladder
that advances is fine for a *stateless* completion and is a different thing
entirely mid-conversation, because the conversation is state the rungs do not
share.

### 2. Narration reached the transcript as speech — ours

This is the one worth keeping.

`turn.rs::deliver` gated its happy path on
`!output.timed_out && !output.message.trim().is_empty()`. That asks whether the
turn produced *text*, and a turn whose provider fails mid-flight has produced
plenty — everything the model narrated before it died. `settle` then found an
empty outbox and wrapped that text as a `Post`.

So the host had a tool surface whose whole argument is that free text reaches
nobody, and then delivered free text to the room anyway. Worse, it did so on
exactly the path where the landing rung would have helped: the seat still had
its session, its tools and its workspace, and a landing would have asked it to
write its work down and post. The rung existed and did not fire, because it is
gated on `timed_out || stalled` and this turn was neither.

`TurnOutput::posted` already carried the right rule in its own doc comment —
*"A turn that did not mark a post has not spoken"* — and nothing enforced it.

**Fixed.** `settle` now returns `Option<Said>`: a tool call is speech, a fence
is speech because the fence is the documented fallback for a CLI that cannot
reach the tools, and everything else is `None`. A `None` goes to the landing
rung and, failing that, to a forfeit. Five regression tests, including the
run-29 sentence verbatim.

### 3. A rescued turn's files were not announced — ours, found in run 30

Not a run-29 defect, but the same shape and found while watching the fix work.
`run.rs` announces `output.files_written` in the feedthrough row, and a landing
writes its files into the separate rescue turn — which `land` never folded
back. A turn that spends its whole budget before writing anything therefore
saves everything in the landing and the room is told about *none* of it.

Run 30's turn 1 is exactly that case: 2400s, deadline hit, nothing written
until the landing, and no `@theory wrote …` row in the transcript. It cost
nothing only because the seat happened to list its files in its own message,
which is luck rather than design — the same argument as defect 3 of
[`2026-09-09-desk-lessons.md`](2026-09-09-desk-lessons.md), where the workspace
rescued a failure by accident.

**Fixed.** `land` folds the rescue turn's written paths into the turn the room
is told about, in written order, naming a path written in both phases once.

## What this says about the tool surface

The surface itself was not at fault and is worth defending on this evidence.
A probe against the same binary lists all four tools to the model and
`desk_post` reaches the outbox in the library's wire form. What the run exposed
is that a *host* can build the surface correctly and then route around it, and
that the failure is silent: a room full of narration looks exactly like a room
having a slow conversation.

It also sharpens why the surface is worth having. Under a fence, "said nothing"
and "said something unmarked" are the same observation and the host must guess.
With tools, an empty outbox is a fact, and the host can act on it — which is
what the fix does.

## Not established

- **Whether the account folds.** The run never reached a fold. Criterion 6 of
  [`folding-by-size.md`](../specs/folding-by-size.md) stays open.
- **Whether the in-turn aside refusal fires live.** No `desk_dm` was attempted.
- **Anything about cadence.** Two turns at ~17 minutes each is run 27's number,
  but both turns were failing, so it measures a broken loop rather than a room.
- **The mathematics.** Nothing was established beyond `@theory` catching that
  the task brief handed to it was wrong: PE 1006 sums the *squares* of the
  decimal values, and both given anchors reproduce under that reading and not
  under the one the brief stated. That the desk refused a premise it could not
  reproduce is the best thing in the run.
