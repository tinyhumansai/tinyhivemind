# Turns

What one coordinator turn is, whichever harness runs it. `TurnScope` is derived
from the authoritative `TurnRequest`; `TurnOptions` is what a host may choose
before the turn is built; `render` is the input text, led by the host's release
note when there is one.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `render` |
| `types.rs` | `TurnScope` and `TurnOptions` |
| `test.rs` | Scope derivation and prompt order |
