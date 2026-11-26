# Seat

`SeatTools` is what one seat is served beyond the hive tools: its executable
tools, its hooks and the context they run in, its access, and its permission
layers' tool names. `SeatTools::from_lowered` builds it from a package's
lowered seat; access is the strictest of the runtime's `autonomy` and every
permission layer's `access`. It refuses a seat with package MCP servers (not
proxied yet), with any sandbox other than `none` (nothing confines a tool
here), and a tool whose own `args` declare `turn` or `call`.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `SeatTools`, its builders and checks |
| `bearer.rs` | A random 256-bit bearer, compared in constant time; `Debug` prints `redacted` |
| `test.rs` | Bearer generation, comparison and redaction |
