# Long context: why the window is the wrong place to fix this

Companion to [`biology.md`](biology.md) and
[`shared-context.md`](shared-context.md). Those ask how a room converges and
what a trace should contain. This one asks the question P14 answers: an agent's
turn sees a bounded slice of an unbounded transcript, so what should be done
about the rest of it?

The default answer — show more — is the one the literature is least kind to.

## Position in the window is not neutral

Liu, Lin, Hewitt, Paranjape, Bevilacqua, Petroni & Liang, "Lost in the Middle:
How Language Models Use Long Contexts", *TACL* 12:157–173 (2024),
[arXiv:2307.03172](https://arxiv.org/abs/2307.03172).

Performance on multi-document QA and key-value retrieval is highest when the
relevant document is at the beginning or the end of the input and degrades
markedly when it is in the middle — a U-shaped curve. Models with longer
context windows do not do better on this than their shorter siblings once the
input fits in both.

The consequence for a shared transcript is direct: enlarging the window does
not make the middle of it readable. A message that arrives in the middle of a
larger window is *less* likely to be used than the same message at the edge of
a smaller one. "Show more" is not a null-cost change with a bounded upside; it
has a real downside, and the downside grows with the thing being fixed.

## Degradation is not a cliff at the context limit

The informal name for this is **context rot**: quality falls off well before a
model runs out of window, and falls off faster when the input is dense and
distractor-rich rather than merely long. The practical reading, and the one
this workspace acts on, is that a context window is a *budget* rather than a
capacity — spending it is a cost paid by every participant reading the same
desk, not just by the agent that wrote the long message.

That is exactly what `BrevityPolicy` states, and why it is stated in the
briefing rather than enforced in code: the cost is real, the accounting is
honest, and the decision about what to do about it stays with whoever is
writing.

## Recursive language models

Alex L. Zhang, Tim Kraska & Omar Khattab (MIT CSAIL), "Recursive Language
Models", [arXiv:2512.24601](https://arxiv.org/abs/2512.24601). Reference
implementation at [`alexzhang13/rlm`](https://github.com/alexzhang13/rlm).

The proposal, in one line: **treat a long prompt as part of an external
environment rather than as a prefix to be swallowed.** The context is held as a
variable in a persistent REPL, and the model writes code to inspect it, chunk
it, and recursively call itself over the snippets that turned out to matter.

The reported results are the interesting part. RLMs handle inputs up to two
orders of magnitude beyond the model's context window, and — the finding that
matters here — they *also* beat base models and common long-context scaffolds
on prompts that would have fit, at comparable or lower cost per query, across
S-NIAH, BrowseComp-Plus, OOLONG, OOLONG-Pairs and LongBench-v2 CodeQA. The
benefit is therefore not only "you can now exceed the window"; it is that
querying a context beats holding it even when holding it was an option.

Two caveats before borrowing the idea:

- It is an *inference strategy*, evaluated on single-prompt benchmarks. Nothing
  in it is about several agents sharing one transcript, and the recursion —
  a model calling itself on a sub-range — is not what this workspace does.
- The comparison it wins is against long-context scaffolds and base models on
  retrieval-shaped tasks. A deliberation is not a retrieval task, and no result
  here transfers to whether a room reaches a *better decision*. The same
  discipline P8 and P9 were held to applies: a mechanism is adopted because its
  cost is bounded and its behavior auditable, not because a paper reported a
  number on a different question.

> **What this workspace would have to represent.** The environment already
> exists: the host's append-only log, reachable through `SessionLog`. What was
> missing was a way for a turn to *interrogate* it rather than receive a
> prefix of it. P14 adds exactly that and nothing more —
> `search_messages`/`search_threads` are the query, `select` is the ordering,
> and the results come back as addresses plus excerpts so a turn can decide
> what to read. There is no recursion: the caller is an agent taking one turn,
> and a fold that could call a model would be a port, which the charter puts in
> the host. The pinboard is the complementary half and has no analogue in the
> paper — a single-prompt setting has nobody else to lose a message on, whereas
> a shared desk does, so a small set of messages is made to arrive whether or
> not anybody queried for them.

## What is deliberately not borrowed

- **No index, no embeddings, no background job.** Search is a bounded backward
  walk over the same port everything else uses, honest about its bound. An
  index would be a second store, and a second store is the thing the charter's
  first rule forbids.
- **No recursion, and no model call in a fold.** Every decision in P14 is a
  pure fold; the only waiting is the log read that was already there.
- **No claim of a quality result.** The mechanisms here bound a cost. Whether
  querying beats holding *for group deliberation specifically* is unmeasured,
  and the benchmark that would settle it is not the one in
  [`../../wiki/Benchmarks.md`](../../wiki/Benchmarks.md).

| mechanism | state this workspace already holds | state it does not |
| --- | --- | --- |
| context as an environment | the host log, behind `SessionLog` | nothing; this is the whole of it |
| querying that environment | `search_messages`, `search_threads`, `select` | ranking by recency as well as score |
| a persistent working set | the pinboard, folded from `!pin` markers | who is permitted to unpin |
| an honest budget | `BrevityPolicy`, stated in the briefing | any enforcement, deliberately |
| recursive decomposition | — | a fold cannot call a model; a port would be the host's |
