# `run` tests

| File | Covers |
| --- | --- |
| `mod.rs` | Wires the submodules |
| `support.rs` | The fixture package, a context over it, declarations, and a fixed `Secrets` |
| `argv.rs` | Fixed arguments, `{name}` and `{name?}` substitution, JSON-encoded non-strings, a missing required argument |
| `process.rs` | Arguments and stdin reaching the program, JSON output, a non-zero exit, the process-group timeout, truncation, unknown programs, an empty command, an edit taking effect on the next call, and a script briefly held open for writing |
| `environment.rs` | The scrubbed environment, an unresolved secret, and the working directory for `cwd` |
| `fixtures/tools/` | POSIX `sh` scripts the tests run, each saying what it does in its header |
