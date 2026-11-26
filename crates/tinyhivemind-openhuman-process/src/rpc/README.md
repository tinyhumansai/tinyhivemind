# Loopback client

`Client` talks to one instance at `http://127.0.0.1:<port>` with its core
token as bearer: `call` for JSON-RPC at `/rpc` (30 s each), `health` for
`/health`, and `events` for the `/events` stream of one `client_id`, parsed by
`sse::Parser`. No proxy, no TLS, and no URL, token or body in an error.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Client`, `Events`, `Error` |
| `sse.rs` | Server-Sent Events framing |
| `test.rs` | Against the fake surface, and the parser on every split |
