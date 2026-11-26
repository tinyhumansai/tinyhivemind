# Errors

`Error` is every way a hive tool call or a management request fails, and
`ArgumentError` why a tool's arguments were refused. Their text is read by
models, so `test.rs` pins it. `Error::Host` carries a host-supplied policy's or
agent creator's own error through the belt unchanged; an adapter downcasts it.

Plan and package failures are `config::ConfigError`.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `Error`, `ArgumentError` and `Result` |
| `test.rs` | The pinned text |
