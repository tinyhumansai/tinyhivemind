# Harness

`OpenHumanHarness` refuses a seat `capability::check` refuses, then starts one
instance per seat and returns a `SeatRunner` around the seat. The seat starts
another instance when its child has died, resumes only a thread whose
transcript the instance has (`threads_transcript_get`), cancels by its
`Active` turn, and kills the instance when a cancel cannot be shown to work.
`stop` closes a running turn's events, cancels it and stops the instance
through the supervisor.

| File | Purpose |
| --- | --- |
| `mod.rs` | `OpenHumanHarness`, `Program`, `Options` |
| `seat.rs` | `Seat` and its `Runner` |
