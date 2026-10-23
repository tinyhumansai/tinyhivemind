# Executable tools

Status: Accepted. Part of [hive language integrations](hive-language-integrations.md).
Implementation: [plan](../plans/2026-10-23-hive-language-integrations.md), crate
`tinyhivemind-exec`.

## Problem

A tool an agent calls had to be Rust compiled into the harness. A client that
writes its agents' tools in Python, TypeScript or shell had to wrap each one,
and every change to a tool meant rebuilding and restarting the harness. The
harness is Rust; its clients are not, and should not have to be.

## Goals

- A hive package declares a tool as an executable plus an argument schema.
  The executable may be any program: a shell script in the package, a Python
  file, a binary on `PATH`.
- The calling convention is language-neutral and fits in a paragraph.
- Editing a script takes effect on the next call, with no rebuild and no
  restart. Editing a declaration takes effect on the next deployment, which is
  a fast restart because nothing is compiled.
- Hooks use the same convention, so a policy hook is a script too.

## Non-goals

- A sandbox of its own. A script runs under the seat's OpenHuman sandbox and
  permission layers like any other tool; `side_effects` feeds that gate.
- Long-running tool servers. Those are MCP servers, already a section.

## Declaration

In `hive.json` under `tools`:

```json
{
  "id": "grep_repo",
  "description": "Search the repository for a pattern",
  "command": ["tools/grep_repo.sh"],
  "args": {
    "type": "object",
    "properties": {"pattern": {"type": "string"}, "path": {"type": "string"}},
    "required": ["pattern"]
  },
  "argv": ["--pattern", "{pattern}", "{path?}"],
  "stdin": "json",
  "output": "text",
  "timeout_ms": 30000,
  "env": {"GITHUB_TOKEN": {"env": "GH_TOKEN"}},
  "side_effects": "read"
}
```

| Field | Default | Meaning |
| --- | --- | --- |
| `id` | required | Tool name the model calls; `[a-z0-9_]+`, unique in the package |
| `description` | required | What the model is told the tool does |
| `command` | required | Program and fixed leading arguments. A first element starting with `tools/` is a package file and must exist; any other is resolved on `PATH` by the host |
| `args` | `{"type":"object"}` | JSON Schema of the call arguments; must be a plain object schema (strict providers reject anything else) |
| `argv` | `[]` | Extra arguments: literal strings, `{name}` substituted with that argument, `{name?}` dropped when the argument is absent |
| `stdin` | `json` | `json` writes the argument object to stdin; `none` closes it |
| `output` | `text` | `text` returns stdout verbatim; `json` requires stdout to parse as JSON |
| `timeout_ms` | 60000 | Wall clock; on expiry the process group is killed and the call fails |
| `env` | `{}` | Extra environment, each value a secret reference |
| `side_effects` | `write` | `read`, `write` or `network`; the permission gate classifies the call by it |
| `cwd` | `workspace` | `workspace` (the seat's OpenHuman workspace) or `package` (the package root) |

An `argv` placeholder must name a property of `args`.

## Calling convention

1. The host resolves the program: a `tools/…` path against the package root
   on disk, anything else on `PATH`.
2. It runs `command ++ argv` with placeholders substituted. A non-string
   argument is substituted as its compact JSON.
3. The environment is cleared and then holds only `PATH`, `HOME`, `LANG`, the
   declared `env` with each reference resolved, and:
   `THM_HIVE`, `THM_SEAT`, `THM_TOOL`, `THM_CALL_ID`, `THM_PACKAGE_ROOT`, and
   `THM_FEATURE_<NAME>` for every feature flag.
4. With `stdin: json` the argument object is written to stdin and stdin is
   closed.
5. Exit status 0: stdout is the result (parsed when `output` is `json`).
   Any other status: the call fails with the exit code and the last 4 KiB of
   stderr. A timeout fails the call and kills the process group.
6. Stdout beyond 1 MiB is truncated and the result says so.

Scripts are read from disk at call time, never cached, which is what makes an
edit take effect on the next call.

## Hooks

A hook names a tool and an event. The hook's tool is called with the event as
its argument object:

| Event | Argument object | Stdout |
| --- | --- | --- |
| `pre_tool_use` | `{"event", "tool", "arguments", "seat", "hive"}` | optional JSON `{"permission": "deny", "message": "…"}` refuses the call |
| `post_tool_use` | `{"event", "tool", "arguments", "result", "seat", "hive"}` | ignored |
| `stop` | `{"event", "reply", "seat", "hive"}` | optional JSON `{"followup": "…"}` asks for one more turn |

A failing hook is ignored unless `fail_closed` is true, in which case a failing
`pre_tool_use` hook refuses the call.

## Acceptance criteria

1. Placeholders, `{name?}` omission, JSON stdin, JSON output, a non-zero exit,
   a timeout, environment scrubbing and an unknown program each have a test in
   `tinyhivemind-exec`.
2. A seat on the offline OpenHuman backend calls a shell tool and a Python tool
   from a package and sees their output.
3. Editing the shell script between two calls changes the second result.
4. A `pre_tool_use` script hook refuses a call and the model sees its message.

## Open questions

None.
