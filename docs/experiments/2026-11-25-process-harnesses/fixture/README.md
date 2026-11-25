# Fixture

Used by every run in [the companion](../README.md).

| File | Does |
| --- | --- |
| `server.mjs` | `node server.mjs <port> <log>`. Scripted Chat Completions and Responses model at `/v1`, streaming or not, and an MCP stub at `/mcp` that maps a bearer to a seat. Logs each request as one JSON line. |

It does not check the key on a model request. That is why the spike proves nothing
about provider credentials.
