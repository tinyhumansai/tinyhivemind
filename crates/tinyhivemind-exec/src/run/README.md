# `run`

One call of an executable tool under the calling convention of
[`docs/specs/executable-tools.md`](../../../../docs/specs/executable-tools.md).

| File | Holds |
| --- | --- |
| `mod.rs` | `run` and `Output`: build the launch, run it, and turn status and stdout into a result |
| `command.rs` | The program path, the substituted arguments, package-relative `tools/…` paths in the fixed arguments, the scrubbed environment, and the working directory |
| `process.rs` | Spawning in a process group, the stdin/stdout/stderr exchange, the caps, and the deadline |
| [`test/`](test/README.md) | The convention end to end against fixture scripts |

Operational constraints:

- A `tools/…` program, and every fixed `tools/…` element of `command` after
  it (`["python3", "tools/count.py"]`), is joined to the absolute package root
  (`command::package_paths`), so it means the same file whatever the working
  directory; any other program is found on the `PATH` the program itself
  receives. Arguments substituted from `argv` placeholders are never rewritten.
- The program leads a new process group. On timeout the whole group gets
  `SIGKILL` (through the system `kill`), so a backgrounded grandchild dies
  with it, and the program is then reaped.
- Stdin, stdout and stderr are exchanged concurrently, and both outputs are
  read to the end past their caps, so a chatty program never blocks on a full
  pipe. Stdout keeps its first 1 MiB; stderr keeps its last 4 KiB.
- Starting a program that is momentarily open for writing ("text file busy",
  as when this process writes a package script while another thread forks)
  is retried with a short backoff, 450 ms in all, before it fails the call.
- Nothing is cached between calls.
