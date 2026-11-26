# Package lowering

`plan` lowers checked packages into one `config::HiveConfig` and, per seat, what
that plan does not carry: executable tools, hooks, features and skills.

It fails with `ConflictingSeat` when a seat id two packages declare differs in
its integration, and `ConflictingDeclaration` when a provider, model, MCP server,
skill, tool, permission set or the runtime is declared differently under one id.
A seat's `permissions` and `context` may differ and become that hive's narrowing
overlay.

| File | Responsibility |
| --- | --- |
| `mod.rs` | The lowering |
| `test.rs` | Shared seats must agree across packages; permissions and context may differ per hive |
