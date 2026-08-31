# Pure mention dispatch

This module turns one committed, revalidated agent reply into either no action
or one canonical child-turn request. It performs no IO and retains no state.

The decision order is intentionally observable: disabled policy, exhausted
hop budget, active source, then the first reading-order nonquiet direct agent
mention. Once a direct agent mention is reached, a self or inactive target
stops selection; a later target cannot become a fallback. Person, desk, and
everyone mentions never dispatch.

`DispatchConversation` is the minimal pure snapshot needed to bind an enqueue
scope. Runtime hosts map their conversation record into its canonical desk id
and optional raw thread-root sequence. `DispatchKey` supplies the committed
trigger sequence. The runtime queue must use both values as its idempotency
scope.

`max_hops` is a host-owned finite `u32`. The core imposes no smaller limit and
uses checked child-hop arithmetic.
