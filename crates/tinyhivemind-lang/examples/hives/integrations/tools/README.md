# Executable tools

Programs the manifest's `tools` section declares. Each is called by the
convention in the [executable tools spec](../../../../../../docs/specs/executable-tools.md):
arguments as JSON on stdin or as `argv`, stdout as the result.

| File | Used as |
| --- | --- |
| `grep_repo.sh` | The `grep_repo` tool: `argv` is `{pattern}` and `{path?}`, stdin is closed, stdout is text |
| `word_count.py` | The `word_count` tool: reads `{"text": ...}` from stdin and prints a JSON object; runs in the package directory |
| `guard_shell.sh` | The `guard_shell` tool, run by the `no_force_push` `pre_tool_use` hook; prints `{"permission":"deny",...}` to refuse a destructive git command |
