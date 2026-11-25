# Server

`HiveToolServer::start` binds 127.0.0.1 on a random port and serves HTTP/1.1
(hyper 1, no framework) at `/mcp`. `serve_seat` draws a bearer and returns the
seat's `ToolEndpoint`; `withdraw_seat` refuses that bearer from then on;
`shutdown` stops accepting, lets open requests finish within
`ServerOptions::shutdown_grace`, and closes the listener.

Bounds: `max_connections` connections at once (one request in flight each, so
this bounds calls too; more wait to be accepted), a header read timeout, a body
timeout, and `max_body_bytes`.

Protocol: revisions `2025-11-25`, `2025-06-18` and `2025-03-26`. The spike's
harnesses asked for the first two (OpenHuman 0.64.15 and OpenClaw
`2025-11-25`, Codex 0.171 `2025-06-18`). One JSON-RPC message per POST, always
answered as `application/json`; batches are refused. Notifications and client
responses get 202 with no body. No `Mcp-Session-Id` is issued: the transport
makes sessions optional, the bearer already names the seat, and none of the
three harnesses needed one. A GET for the optional event stream, a DELETE or any
other method gets 405 with `Allow: POST`.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `HiveToolServer`, `Services`, `ServerOptions`, the accept loop |
| `http.rs` | Host, Origin, path, bearer, method, version header, body bound |
| `rpc.rs` | JSON-RPC framing and the MCP methods: initialize, ping, tools, empty resource and prompt lists |
