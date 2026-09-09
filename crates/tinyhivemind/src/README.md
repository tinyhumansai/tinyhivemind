# `tinyhivemind` feature modules

One directory per feature area, wired together and re-exported from
[`lib.rs`](lib.rs). Each answers a different question a host needs answered
about a live session; see its own `README.md` for the how and why.

| Module | Question it answers |
| --- | --- |
| [`session`](session) | How does a turn walk a host-owned, globally sequenced log into an attributed, audience-filtered transcript? |
| [`briefing`](briefing) | What ephemeral context (teammates, coordination rules, history, threads, pins) does one viewer's turn open with? |
| [`sharing`](sharing) | How does a host hand an already-briefed session only what changed since its last watermark, instead of re-briefing it? |
| [`search`](search) | How does a turn reach a message or thread outside its window, on request? |
| [`pins`](pins) | Which messages does every turn see whether or not it asked? |
| [`threads`](threads) | What live threads exist in one desk, ranked by recency, for a viewer that has been away? |
| [`responder`](responder) | Who answers an unaddressed message, when the ladder must ask a model to name a candidate? |
| [`dispatch`](dispatch) | When an agent mentions a peer, how does exactly one child turn get enqueued on the host, never zero-or-many? |
| [`speech`](speech) | What may a seat say, what makes a call valid, and what does exactly one accepted utterance become? |
| [`referral`](referral) | The same one-child-turn edge as `dispatch`, but crossing to a different desk and carrying one answer back. |
| [`error`](error) | The one `Error`/`Result<T>` every fallible function in this crate returns. |
