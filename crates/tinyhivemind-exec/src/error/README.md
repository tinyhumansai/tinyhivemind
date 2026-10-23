# `error`

| File | Holds |
| --- | --- |
| `mod.rs` | `Error` and the `Result` alias |

Each variant is one way a call fails: an unresolved secret, a missing
required argument, an empty command, a program that cannot start, a pipe
failure, a non-zero exit (with the last 4 KiB of stderr), a timeout, and
`output: json` stdout that does not parse. The variants are produced and
asserted by the tests of `run/`, `context/` and `program/`.
