# Thread-scoped conversations

Status: proposed. Cross-repository — the behavior described here is split
between this repository and `opencompany`, and the split is the substance of the
document.

`opencompany` issue [#1890] made a chat thread a context boundary rather than a
rendering fold. Seven sub-issues landed there (A–I; F excludes `find_thread`).
Roughly half of that work is in the layer this repository is taking over, and
the rest sits above it. This says which is which, and why.

[#1890]: https://github.com/tinyhumansai/opencompany/issues/1890

## The problem #1890 fixed

`OperatorMessage.parent` was "a parent id, not a thread object". The console
folded a transcript by it and nothing downstream knew a thread existed, so **the
model answering inside a thread was handed the whole channel.** In a channel
with two live threads, "make it shorter" arrived with the other thread's answer
as its most recent context.

That is the same class of defect as the two in [`../../ROADMAP.md`](../../ROADMAP.md):
the projection was lossy in a way no reader could detect. First-person collapse
loses *who spoke*; thread collapse loses *which conversation*.

## What this repository already has

More than the phase list suggests. `crates/tinyhivemind/src/session/mod.rs`
already carries #1890 A verbatim:

```rust
match conversation.thread_root {
    None => message.parent.is_none(),
    Some(root) => message.sequence == root || message.parent == Some(root),
}
```

`Conversation { desk_id, desk_name, thread_root }` is the host's `ChatTarget`
with the desk name attached, and `SessionQuery` is its seed request. P1 supplies
`is_general_chat` and `same_conversation`, which #1890 leans on throughout.

So the question is not whether threads belong here. They already do. It is which
of the remaining six sub-issues this layer owns.

## The split is by layer, not by sub-issue

Each sub-issue divides into a **conversation-addressing half** and a
**storage-or-wiring half**. Reading the charter's "owns no storage, no board, no
HTTP" as "none of B, C, E, F belongs here" is the wrong cut: it is the *host's
copy of the vocabulary* that should go, not the feature.

| Sub-issue | This repository | The host |
| --- | --- | --- |
| **A** thread-scoped seed | done | — |
| **B** a card records its thread | the `Conversation` it stores | the field on the card |
| **C** settle markers as briefing | the briefing slot | which cards, and their state |
| **D** replies land in a thread | the channel-level projection rule | journaling, and the renderer |
| **E** cold-start briefings | the thread index over the transcript | "where its work landed" |
| **F** `read_thread` | the read — it is `project_session` | the tool schema and belt |
| **G** bounded page read | the `SessionLog` contract | the implementation behind it |
| **H** ACP session scope | `Conversation::equivalent_to` as the key | the session lifecycle |
| **I** identity ≠ streaming | `Conversation` as a parameter | nothing — see below |

### B — the field stays, its type should not

`TaskRecord` is a board card and stays in the host. But it currently holds
`origin_chat_id: Option<String>` beside `origin_parent: Option<Sequence>`, which
is a hand-rolled `Conversation` — two fields that must agree and nothing making
them.

They drifted. #1890 B stamped `origin_parent` from the message's own `parent`,
recording only threads an operator opened by hand; #1890 D then made every
question a root. For one channel-level message the answer went into a thread and
the card recorded none, so its settle marker landed in the channel while the
answer sat in the thread — the split B exists to prevent, reintroduced by D
moving the ground under it.

Storing a `Conversation` makes that unrepresentable. One value, one rule
constructing it.

### C and E — the briefing seam already exists, and is better

`TeamBriefing` is "ephemeral team context assembled separately from durable
history", returned as a typed `SessionInitialization`. `opencompany` assembles
the same kind of context by **appending text to the operator's message** —
`OPEN_WORK_ANNOTATION`, and then three more markers for C, E and attachments.

That is why `operator_words` exists there: every appended block has to be cut
back off before anything reasons about what the operator asked for. It was not
free. A desk-addressed "thanks!" once scored as substantial work because the
appended briefing is long and length reads as substance, and it opened a card —
which lengthened the next briefing. The cut list has grown four times.

A typed briefing has nothing to strip. Porting C and E means giving
`SessionInitialization` room for host-supplied context, not porting the strings.

### E's index is mostly transcript data

Roots in a channel, their opening words, reply counts, recency — all of it is
already in the paging walk. Only "finished → In review" needs host state. The
index is a fold this crate can compute; the landing is a field the host fills.

### F is a query, not a feature

`read_thread(root)` is `project_session` with `thread_root: Some(root)`. The
channel scoping the tool must not break is the scoping the projection already
does. What stays above is the tool's schema, its place on the belt, and the
ambient conversation a tool call reads.

### I disappears

#1890 I exists because `opencompany` derived chat identity from whether a turn
was live-streaming, so a turn with a conversation but no stream — an approval's
re-issued call — could not be expressed. `project_session` takes a
`Conversation` as a parameter. Adopting it makes the defect unrepresentable
rather than fixed.

## The one place this repository is behind

`in_thread`'s channel-level arm is the pre-D rule:

```rust
None => message.parent.is_none(),
```

#1890 D changed what an unparented message means. Once every answer threads under
the message that opened it, "unparented lines only" selects a run of questions
with no answers — the channel emptied for the model, exactly as folding every
reply empties it on screen. D's rule is **roots plus each root's first reply**,
and deliberately not "one level flattened", which is the pre-A leak with extra
steps.

One constraint on where that can live: `SessionMessage` carries `sequence`,
`author` and `content` — **not `parent`**. The narrowing needs the parent to pick
each root's first reply, so it must happen inside the walk, where `LogMessage`
still has it, and cannot be a fold a caller applies afterwards.

## G is a port contract, not code

`SessionLog::read_before(before, limit)` is the primitive #1890 G optimised. The
host's filesystem implementation streamed from the head of the journal and
parsed every line to keep the last few, so one page cost O(total events):
72.8ms against 0.4ms at 100k events, linear against flat.

Nothing here can fix that — the host owns storage. What this repository can do is
say so in the port's documentation: `read_before` is called once per rebind and
now once per watermark tick, so an implementation linear in the journal is
linear in the company's whole history on every turn.

## Sequence

Ordered so each step is independently reviewable, and none depends on the
adapter landing first.

1. **The channel-level rule.** Teach the walk roots-plus-first-reply, inside the
   paging walk where `parent` is still in scope. Self-contained, and the only
   place this repository is behind.
2. **`read_before`'s cost, in the port's docs.** No code. It stops the next host
   from paying what `opencompany` paid.
3. **A thread index over the projection.** E's fold, with the host-supplied
   landing left as an option on the row.
4. **Host-supplied briefing context** on `SessionInitialization`, so C and E have
   a typed home rather than an appended string.
5. **`Conversation` as the stored origin** — a host-side change in `opencompany`,
   listed here because it is the one that makes B and D unable to disagree.

Steps 1 and 2 are worth doing before the adapter integration. Steps 3–5 are
better after it, when there is a real consumer to shape them against.

## Non-goals

- **Porting the tools.** `read_thread`'s schema and registration are the host's;
  only the read is here.
- **Porting the renderer.** D's flat-when-nothing-overlaps rule is a console
  decision, re-made on every render precisely so `parent` never becomes a
  function of race timing.
- **Owning card state.** C and E need to know where work landed. That is a
  question for the host, asked through a briefing the host fills.
- **`find_thread`.** It was gated on G's bound and has no home yet in either
  repository.

## Open questions

- **Does the index belong in `-core` or the runtime crate?** The fold is pure,
  but it needs the walk's output, which is the runtime crate's. The charter's
  rule — decision in core, waiting in the runtime — suggests core takes a
  projected slice and the runtime does the reading.
- **What shape carries the landing?** An `Option<String>` on an index row is
  enough for "finished → In review" and admits nothing else; a typed status
  would need this crate to know what a board is, which it must not.
- **Does the watermark change the index's liveness bound?** P5 re-seeds on a
  watermark rather than a rebind, so "threads with activity inside the page the
  seed already walks" now has a moving definition.
