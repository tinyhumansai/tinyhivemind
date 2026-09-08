# Room account tests

Unit tests for `../digest.rs`: what the folder behind
`tinyhivemind::Digester` is actually asked.

## Files

- `test.rs` — that the prompt asks for an *account* rather than a summary of
  activity, states the character budget, and forbids a number the messages do
  not contain; that a prior account is handed over to be rewritten rather than
  appended to; and that every author kind the room can carry renders,
  including the `system/workspace` feedthrough row.
