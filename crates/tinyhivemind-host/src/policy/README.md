# Policies

What a host decides about its seats, as ports the belt consults.

| Item | Decides |
| --- | --- |
| `SendAuthorizer` | Whether a `SendRequest` may go out, optionally per session |
| `ManagementAuthorizer` | Whether a `ManagementRequest` may act, before any mutation |
| `AgentCreator` | How a `NewAgent` is built and registered; the one management step that needs a harness |
| `Management` | The two management ports, as configured together |

The requests are wire types. `ManagementRequest::CreateAgent` omits absent
optional fields, so older serialized requests still decode.

| File | Responsibility |
| --- | --- |
| `mod.rs` | The ports |
| `types.rs` | The requests they are shown |
| `test.rs` | The wire shapes |
