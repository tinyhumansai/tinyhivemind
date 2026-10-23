# `context`

Where and for whom a call runs, and how its secrets resolve.

| File | Holds |
| --- | --- |
| `mod.rs` | `ExecContext` (package root, workspace, hive, seat, features), the `Secrets` port, and `EnvSecrets` |
| `test.rs` | `EnvSecrets` resolving a set variable and refusing an unset one and a store reference |

`Secrets::resolve` runs at call time, once per declared `env` entry, so a
credential is never held by this crate. `EnvSecrets` reads this process's
environment; a host with a secret store implements `Secrets` over it.
