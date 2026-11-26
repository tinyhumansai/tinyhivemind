# Fixtures

A package root for the integration tests. Every script runs in the seat's
temporary workspace, so the logs it writes are the test's evidence of how many
times it ran.

| File | Does |
| --- | --- |
| `tools/stamp.sh` | Write tool; appends its arguments to `stamp.log`. |
| `tools/slow.sh` | Write tool; logs a start to `slow.log`, then waits for a `release` file. |
| `tools/look.sh` | Read tool; logs each run to `look.log`. |
| `tools/guard.sh` | `pre_tool_use` hook; logs to `guard.log` and refuses an event naming `forbidden`. |
| `tools/audit.sh` | `post_tool_use` hook; logs to `audit.log`. |
