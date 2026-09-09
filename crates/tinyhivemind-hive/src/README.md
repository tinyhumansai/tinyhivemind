# Feature modules

Each directory answers one question, folded independently and wired together
by [`lib.rs`](lib.rs). Read a module's own `README.md` for its design and
public surface; this file is only the index.

| module | answers |
| --- | --- |
| [`attention/`](attention/README.md) | Who takes the floor this turn, and how much of a bounded prompt each context source gets. |
| [`directory/`](directory/README.md) | Who knows what — transactive memory folded from grounded deposits and the citations they drew. |
| [`division/`](division/README.md) | A task's facets, split across the seats that own them, and what each owner reads. The one mechanism here whose default is *on*. |
| [`episode/`](episode/README.md) | The pure state machine: given a transcript, who speaks next, and has the room finished. |
| [`error/`](error/README.md) | The crate-wide `Error` and `Result<T>`. |
| [`exchange/`](exchange/README.md) | Private, off-floor contact between turns that spends no floor and starts no turn. |
| [`quorum/`](quorum/README.md) | Whether a topic has carried, and cross-inhibition that silences an advocate rather than debiting an option. |
| [`salience/`](salience/README.md) | Recency decay, importance, and relevance, folded into one comparable score. |
| [`trace/`](trace/README.md) | The stigmergic grammar: what a message deposits, and how it is read back. |
