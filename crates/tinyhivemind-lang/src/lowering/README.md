# lowering

Produces existing core desks, role-aware route candidates and host factory configuration. Goals, missions, desk hierarchy and referral channels remain
explicit activation inputs for the host. Only declared shared context documents
are included in the context map; their item markers and contents are preserved.
Lowering preserves memory constraints for host enforcement and never starts agents.

## Integration

`LoweredHive::integrations` carries the runtime settings, the default
provider, model and permission set resolved to values, the skills the package
ships, and the session, Jev, feature and workflow settings. Each
`LoweredSeat::integration` is that seat's settings after precedence: the
runtime's defaults, then the seat's profile, then the seat. A scalar (`model`,
and a permission set) takes the most specific value, though permissions are
kept as a list of every layer that applies, broadest first; lists
(`toolpacks`, `mcp`, `skills`, `hooks`, `context`, `tools.deny`) concatenate
without repeats; `limits` merge field by field (seat, profile, the seat's model, then the
runtime); `tools.allow` is the most specific non-empty list. A toolpack
expands into its tools, MCP servers, skills and tool groups first, and a
toolpack is the only way a seat is given an executable tool; a hook's tool
needs no toolpack. Setting the feature `mcp`, `skills`,
`script_tools` or `hooks` to `false` empties that integration. Lowering runs
on a validated package, so an id that does not resolve is skipped, not
reported; call `validate` first.

| File | Purpose |
| --- | --- |
| types.rs | `LoweredHive`, `LoweredSeat`, `LoweredMemoryBinding`, and the integration values: `LoweredIntegrations` (hive-wide), `LoweredSeatIntegration`, `LoweredModel` and `LoweredHook`. |
| mod.rs | Public lowering API and implementation. |
| integration.rs | Resolution of the integration: precedence, toolpack expansion and feature switches. |
| test.rs | Activation snapshot preservation, and integration resolution regressions. |
