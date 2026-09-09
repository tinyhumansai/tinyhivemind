# What one seat is told

`compose_prompt` assembles the single string an agent process reads for one
turn. The order is deliberate — who it is, who else is here, what it knows,
what the room has said, then what it was actually asked — because a model reads
the last thing best, and the trigger is the thing it must act on.

## The roster problem

The library's briefing lists the seats with their roles. That is a *static*
roster, and it is not enough to route work. A seat deciding who to hand the
turn to also needs to know who is carrying it: who has spoken, when, and who
has not spoken at all. It cannot derive that from the window it was handed,
because the window is bounded and a seat that has fallen out of it is exactly
the seat nobody thinks to call on.

`who_is_here` adds that half, folded from the whole transcript rather than the
window. Run 28 is the evidence: nine of twelve turns ran inside one seat while
three sat idle, and `desk_dm` was never called once across the run.

The section also states the mechanism plainly — naming a seat with `@id` is
what runs it next, and naming nobody ends the chain. The library will say this
too, but only through `system_text_with_dispatch`, and only when the run's
policy and hop actually allow a child turn; `run.rs` passes both so a seat at
the hop cap is not offered something that would refuse it.

## The desk before the detail

`desk_so_far` is the first section after the roster, and it always says
something. When a fold has happened it carries the standing account; when none
has, it says so and names the sequence the room actually starts at. The
distinction matters: a seat told nothing about the desk's history cannot tell
"there is none" from "you were not shown it", and the second reading is the one
that makes it re-derive work somebody already finished.

It is placed above the seat's own notebook deliberately. A fresh process reads
the last thing best and the first thing next best, and where the *desk* is
should outrank where this seat left its notes.

## One private channel, two names

The shared-session rules call a private message `!aside @peer`; the desk's own
rules call it `desk_dm(to, message)`. They are the same mechanism — the tool
call is routed through the same `aside` policy and can be refused the same way,
leaving the row desk-visible. The prompt says so explicitly, because two names
for one capability with no statement that they are one is a good way to get
neither used.
