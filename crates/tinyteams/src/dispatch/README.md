# Mention-turn queue boundary

The runtime dispatch module composes the pure core decision with exactly one
host call. A no-dispatch decision makes no call. A one-target decision passes
the canonical request unchanged to `MentionTurnQueue::enqueue_once`; expected
refusals are final outcomes and unexpected failures retain their source.

The host implementation is the transaction boundary. Under a key composed of
the request conversation and committed trigger sequence, one transaction must:

1. re-read the committed reply and match sequence, source, content, and scope;
2. revalidate live feature policy, authorization, and target availability;
3. durably enqueue no more than one child turn; and
4. return `Already` for a duplicate without creating another turn.

A rollback must leave no idempotency marker and no child turn. This crate owns
no journal and never retries. Environment variables and Cargo features do not
enable dispatch; the host passes an explicit policy into every decision.
