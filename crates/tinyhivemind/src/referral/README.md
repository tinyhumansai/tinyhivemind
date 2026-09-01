# Referral queue boundary

The runtime referral module composes the pure core decision with exactly one
host call, the same way `dispatch` does. It differs in one thing, and every
extra host obligation follows from it: **the child turn may run on a different
conversation from the one that triggered it.**

`Referral` therefore names two conversations. `from` is where the trigger was
committed; `to` is where the child turn runs. A referral that crosses always
lands on the target desk's desk channel, never in a thread, because a thread
root is a sequence number in the conversation that owns it.

The host implementation is the transaction boundary. Under a key composed of
`referral.from` and the committed trigger sequence, one transaction must:

1. re-read the committed reply and match sequence, source, content, and scope;
2. revalidate live feature policy, authorization, and target availability **on
   both conversations** — a crossing referral writes into a channel the author
   is not a member of, so authorizing only the source desk authorizes nothing;
3. durably enqueue no more than one child turn; and
4. return `Already` for a duplicate without creating another turn.

A rollback must leave no idempotency marker and no child turn. This crate owns
no journal and never retries.

The back edge is the host's obligation too. When a host runs a child turn that
carried a `ReferralOrigin`, it must pass that origin back in the next
`ReferralInput`, or the answer has no way home. Nothing in the library
remembers it.

Every knob is off in `ReferralPolicy::DEFAULT`, and with only `enabled` and
`max_hops` set the decision is exactly the one `mention_dispatch` makes. That
equivalence is asserted in `tinyhivemind-core`, not merely documented.
