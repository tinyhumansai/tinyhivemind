# Codex run

| File | Does |
| --- | --- |
| `config.toml.in` | the final `CODEX_HOME/config.toml`, with `@FIXTURE_PORT@` and `@CATALOG@` |
| `catalog.mjs` | generates the model catalog from the pinned binary |
| `lib.mjs` | `codex app-server` client over stdio with the cleared environment |
| `run.mjs` | the gate and the measurements |

The catalog is vendor data and is not committed. `catalog.mjs` runs
`codex debug models --bundled` with a private `CODEX_HOME` and changes these fields
on every model, leaving the rest:

| Field | Set to |
| --- | --- |
| `tool_mode` | `direct` |
| `shell_type` | `disabled` |
| `apply_patch_tool_type` | `null` |
| `supports_search_tool` | `false` |
| `multi_agent_version`, where present | `null` |
| `use_responses_lite`, `supports_reasoning_effort_updates`, where present | `false` |
| `experimental_supported_tools`, where present | empty |

Without it, models in the catalog hid MCP tools or routed them through code mode.
The provider is `wire_api = "responses"` with `MOCK_KEY=dummy`. MCP config and the
bearer go on both `thread/start` and `thread/resume`.
