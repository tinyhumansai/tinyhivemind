# The fake harness

| File | Purpose |
| --- | --- |
| `child.rs` | The child: a real process that speaks JSON lines, plays scripted model steps, keeps a session file and can leave a grandchild in its own session. Built as the `tinyhivemind-fake-harness` binary. |
| `client.rs` | Its client, over the same port a real one implements, and the `Fault`s it can be told to commit. |
| `subject.rs` | Both as a `Subject`, observed through the child's call log and `/proc`. |
