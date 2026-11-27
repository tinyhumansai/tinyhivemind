# Protocol

The ACP version 1 messages a seat needs. Reading is tolerant; usage is read where the measured agents put it.

| File | Purpose |
| --- | --- |
| `mod.rs` | `initialize` and accepting its result, MCP server, prompt, usage, stop reasons, updates, permission answers |
| `test.rs` | Each message built and read |
