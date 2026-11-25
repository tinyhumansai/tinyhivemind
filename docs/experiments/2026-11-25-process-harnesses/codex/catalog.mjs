// node catalog.mjs <codex> <out.json>: write a direct-tool model catalog from the binary's own one.
// The catalog is vendor data and is generated, never committed.
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process";
const [codex, file] = process.argv.slice(2);
const c = JSON.parse(execFileSync(codex, ["debug", "models", "--bundled"], { maxBuffer: 1 << 28, env: { PATH: "/usr/bin:/bin", HOME: path.dirname(file), CODEX_HOME: path.dirname(file) } }));
for (const m of c.models) {
  const set = (k, v) => { m[k] = v; };
  set("tool_mode", "direct"); set("shell_type", "disabled"); set("apply_patch_tool_type", null);
  set("supports_search_tool", false);
  for (const [k, v] of [["multi_agent_version", null], ["use_responses_lite", false], ["supports_reasoning_effort_updates", false], ["experimental_supported_tools", []]]) if (k in m) set(k, v);
}
fs.writeFileSync(file, JSON.stringify(c));
