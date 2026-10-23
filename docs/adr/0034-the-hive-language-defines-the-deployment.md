# 34. The hive language defines the deployment

- **Status:** Accepted — supersedes the parallel-schema decision of
  [the OpenHuman hive configuration spec](../specs/openhuman-hive-config.md)
- **Date:** 2026-10-23
- **Specification:** [hive language integrations](../specs/hive-language-integrations.md),
  [executable tools](../specs/executable-tools.md)

## Context

Two schemas described one hive. A hive package (ADR 0032) held structure and
deliberation policy, and an editor could change it under a constitution
(ADR 0033). The OpenHuman adapter's `HiveConfig` held everything an agent runs
with: provider, model, MCP servers, skills, permissions, workflows. Nothing
converted one into the other, so a package's seat `model`, `mcp` and `skills`
were names that reached no runtime, and a hive could not edit the settings
that decide what its agents can do.

Every hive this repository runs is hosted by OpenHuman. Keeping the language
free of OpenHuman types is still right — it keeps `tinyhivemind-lang` pure and
reviewable — but keeping its *vocabulary* free of what OpenHuman runs left the
language unable to say anything about a deployment.

## Decision

- The hive package is the one definition of a hive mind, integrations
  included. `hive.json` gains `runtime`, `providers`, `models`, `mcp`,
  `skills`, `tools`, `toolpacks`, `permissions`, `profiles`, `hooks`,
  `session`, `jev`, `features` and `workflows`, with lang-owned mirror types.
- The vocabulary is OpenHuman's: a field exists because OpenHuman can run it,
  and every field lowers to OpenHuman or fails `check`.
- `HiveConfig` is retired as an input. The adapter deploys lowered packages;
  the deployment types it keeps are internal.
- Tools and hooks may be executables in the package, called by a
  language-neutral convention from a new `tinyhivemind-exec` crate, so client
  code is written in any language and changed without rebuilding the harness.
- Integration sections are a new `integration` edit class, so a constitution
  decides whether a hive may change what it runs with.

## Consequences

- One schema to document, edit, check and version.
- Lowering grows: seats carry resolved models, servers, skills, tools, hooks
  and permissions rather than names.
- Deploying several hives on one runtime is several packages sharing seat ids;
  agreement across them is a deployment check in the adapter.
- `tinyhivemind-exec` runs processes, so it sits beside the adapter, outside
  the pure crates `assert-pure.sh` guards.
