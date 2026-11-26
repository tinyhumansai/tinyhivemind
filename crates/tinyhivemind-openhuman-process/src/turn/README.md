# Turns

`run` subscribes to `/events` for the turn's own `client_id`, then submits
`channel_web_chat` on the seat's thread, records the returned `request_id` in
the seat's `Active`, and follows that request's events: progress, approvals
through the seat's approver and `approval_decide`, `chat_done` with the turn's
own usage, `chat_error` and `chat_cancelled` as failures. Reaching the
eight-iteration cap is a failed turn, as is passing a lower seat limit.
Approvals are denied when the approver takes longer than its limit, and an
`approval_request` without an id fails the turn. `deny_stale` denies what a
turn that is over left pending. A `chat_error` message is kept to one line of
200 characters.
`cancel` is `channel_web_cancel` scoped to the request, then a wait for its end.
A turn is never submitted twice.

| File | Purpose |
| --- | --- |
| `mod.rs` | `run`, `cancel`, `Turn`, `Active`, `Ended`, `Error` |
| `test.rs` | Against the fake surface |
