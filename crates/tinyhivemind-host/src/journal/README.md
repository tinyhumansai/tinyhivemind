# `journal`

`MemoryLog`: an append-only in-memory journal for one desk that is a real
`SessionLog`. Two rules decide who may read a row, and they are the rules a
host's own journal follows: a desk row with `only_for` reaches its author
and that one seat; a row in a conversation reaches the conversation's two
seats. `append` returns the sequence a row was given, and `append_to` puts
one on another desk of the same host, so a seat's other channels can be
read the way a host's log holds them; `desk_since`,
`thread` and `thread_since` render rows for a reader. Always compiled: the
example, the tests and the crate's doc example are hosts over it.

`test.rs` verifies sequence numbers, pagination, private conversations and
rows from another host hive. This optional research log is separate from the
coordinator's durable storage snapshot.
