# tinyhivemind-exec

Agent tools in any language. A hive package declares a tool as a program and
an argument schema — a shell script, a Python file, a binary on `PATH` — and
this crate runs it under one calling convention that fits in a paragraph:
arguments on the command line and as JSON on stdin, a scrubbed environment
naming the hive, seat and call, stdout as the result, and a deadline that
kills the whole process group. Hooks are tools too, so a policy that refuses
a call is a script.

Nothing is compiled and nothing is cached: edit a script and the next call
runs the edit. A host attaches each declared tool as a native
`tinytools::Tool` (`ProgramTool`) and calls `HookRunner` around its tool calls
and turn ends.

The convention is specified in
[`docs/specs/executable-tools.md`](../../docs/specs/executable-tools.md); the
declarations come from [`tinyhivemind-lang`](../tinyhivemind-lang/README.md).
The repository [README](../../README.md) says how this fits the rest.

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate's modules. |
