# Example hive packages

A gallery of complete packages in directory form. Between them they use every
part of the language; each one's `README.md` opens with what it demonstrates.
`tests/gallery_packages.rs` parses and lowers every one, and
[`hive_view`](../hive_view/README.md) renders and runs them offline.

| Package | Seats | Desks | What it shows |
| --- | --- | --- | --- |
| `review-board/` | 3 | 1 | The minimal hive: blind round, grounded quorum, a replayed `script.json` |
| `research-debate/` | 4 | 1 | Goals with subgoals, missions, an itemized `context/` document, `responder_mode: auto` |
| `incident-response/` | 6 | 2 | Topology, referrals with a hop limit, an approval policy, private asides |
| `memory-guild/` | 4 | 1 | Agent, pool, forked, hive-lifetime and read-only memories; bindings; JSON frontmatter |
| `self-improving/` | 3 | 1 | A constitution with permitted edits, canaries and protected namespaces |
| `math-panel/` | 5 | 1 | Five unreliable seats solving exact problems (`sim.json`) |
| `math-swarm/` | 50 | 5 | Fifty seats in five linked desks on the same problems, rounds of ten |
| `integrations/` | 2 | 1 | Every integration section: runtime, provider, models, an MCP server, a skill, bash and Python tools, a toolpack, permissions, profiles, a script hook, session, Jev, features and a workflow. `hive_check` accepts it; `broken/integrations` is the same package with five faults |
| `euler-hive/` | 14 | 5 | A live, problem-agnostic hive: per-seat tool access (web search on research only, Python on compute and verify only), referrals, an agent memory per seat and a shared verified pool. Its own test is `tests/euler_hive_package.rs`. |

`sim.json` and `script.json` are neither canonical nor Markdown, so `parse`
drops them: they belong to the example, not to the package.

No seat may reach the web: every seat's `tools.deny` lists `web_search`,
`web_fetch`, `browser` and `http`, and no `allow` list names one. The one
exception is `euler-hive`, whose research-desk seats opt in to `web_search`
(and only that) to search the literature. `tests/gallery_packages.rs` reads
every package directory and asserts both. A package of
several desks runs one floor per desk in `hive_view`, linked only by referral
along its declared topology and referrals. The `README.md`
files are accepted as unreferenced Markdown documents.
