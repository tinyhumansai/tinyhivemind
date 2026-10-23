# pe1006_hive context

| File | Purpose |
| --- | --- |
| `sealed.md` | Composed into every seat but the researcher |
| `research.md` | Composed into the researcher |
| `agents.md` | Workspace `AGENTS.md` template, embedded by the binary |
| `memory.md` | Workspace `MEMORY.md` template, embedded by the binary |
| `task.md` | The PE1006 statement, embedded by the binary |

`sealed.md` and `research.md` open with an `<!-- item:id -->` marker, as the
hive language requires of composed context. The three templates are composed
into no prompt; the binary copies them into the workspace verbatim, without
overwriting existing files.
