# OpenHuman run

| File | Does |
| --- | --- |
| `config.toml.in` | the final seat config, with `@FIXTURE_PORT@` and `@HIVE_TOOL@` |
| `start.sh` | starts one `openhuman-core run --headless-api` with the exact cleared environment |
| `rpc.mjs` | JSON-RPC, `/events` and `web_chat` helpers |
| `run.mjs` | the gate and the measurements |

- The model-visible tool name carries a hash, so `run.mjs` starts once to read it
  from `agent_registry_available_tools`, then writes the exact allowlist.
- The provider is `auth_style = "none"`; `primary_cloud` must be set or the hosted
  provider is consulted.
- `OPENHUMAN_BACKEND_API_KEY=dummy` and the keyring key made by `start.sh` (random,
  in the work directory) keep the binary from asking for real ones.
- Bad TOML resets to defaults without an error, so check the tool list printed.
