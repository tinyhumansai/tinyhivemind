# Activation

`Activation` is the barrier between attaching a seat's tools and the coordinator
recording its registration. Tools ask `is_ready`; a turn claimed in between
awaits `wait`; `activate` opens it once and for good.

| File | Responsibility |
| --- | --- |
| `mod.rs` | The barrier |
| `test.rs` | A waiting claim wakes, and a later one does not wait |
