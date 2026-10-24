# integration

The wire types for what a hive's agents run with. `hive.json` carries these as
the sections `runtime`, `providers`, `models`, `mcp`, `skills`, `tools`,
`toolpacks`, `permissions`, `profiles`, `hooks`, `session`, `jev`, `features`
and `workflows`, plus the integration fields of a seat (`profile`, `agent_id`,
`toolpacks`, `hooks`, `permissions`, `limits`, `context`). The vocabulary is
OpenHuman's, since every setting must lower to something OpenHuman can run,
but no type here names a harness type.

Every struct denies unknown fields, and every field has a default, so a
manifest writes only what it sets and an existing package, which has none of
these sections, is unchanged. A credential, an MCP `env` value or header, and a
tool's `env` value is a `SecretRef` (`{"env": "NAME"}` or `{"store": "key"}`),
never a literal; any other shape is a decode error.

Defaults the types supply: `concurrency` 4, `session_store` `memory`,
`memory_engine` `disabled`, `autonomy` `supervised`, `sandbox` `none`, Jev
`ranker` `jev`, `route` `auto`, `top_k` 3, a tool's `timeout_ms` 60000, `stdin`
`json`, `output` `text`, `side_effects` `write`, `cwd` `workspace`, a hook's
`event` `pre_tool_use`, and a workflow `enabled` and `single_flight`.

This module only declares. References between sections are checked in
[`../../validation/integration.rs`](../../validation/integration.rs) and
resolved by precedence in [`../../lowering/integration.rs`](../../lowering/integration.rs);
the adapter acts on the result. The executable calling convention is in the
[executable tools specification](../../../../../docs/specs/executable-tools.md)
and is implemented by `tinyhivemind-exec`.

| File | Purpose |
| --- | --- |
| `mod.rs` | Module doc; re-exports both files |
| `types.rs` | `SecretRef`, `Limits`, `Access`, `Sandbox`, `RuntimeSettings`, `Provider`, `Model`, `McpServer`, `Skill`, `Toolpack`, `PermissionSet`, `Profile`, `Hook`, `SessionSettings`, `JevSettings` and `Workflow`, with their enums |
| `tool.rs` | `ExecTool`: an executable and its argument schema, `argv` template, stdin, output, timeout, environment, side effects and directory, with `placeholder` parsing |
