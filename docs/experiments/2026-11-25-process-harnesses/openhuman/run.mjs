// node run.mjs <openhuman-core> <work dir> <core port> <fixture port> <fixture log>
// The gate and the measurements for OpenHuman web_chat, against a running fixture.
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process";
import { rpc, turn, events, fx, sleep } from "./rpc.mjs";
const [bin, work, portArg, fport, log] = process.argv.slice(2); const P = +portArg, T = "core-token";
const here = path.dirname(new URL(import.meta.url).pathname); const out = (k, v) => console.log(k, JSON.stringify(v));
const pid = () => +fs.readFileSync(`${work}/pid`, "utf8");
const alive = (p) => { try { process.kill(p, 0); return true; } catch { return false; } };
const start = () => out("start", execFileSync(`${here}/start.sh`, [bin, work, portArg]).toString().trim());
const stop = async (sig) => { const p = pid(); process.kill(p, sig); while (alive(p)) await sleep(20); };
const render = (tool) => { fs.mkdirSync(`${work}/ws`, { recursive: true }); fs.writeFileSync(`${work}/ws/config.toml`, fs.readFileSync(`${here}/config.toml.in`, "utf8").replaceAll("@FIXTURE_PORT@", fport).replaceAll("@HIVE_TOOL@", tool), { mode: 0o600 }); };
const mem = () => { const g = (f, k) => Math.round(+fs.readFileSync(`/proc/${pid()}/${f}`, "utf8").match(new RegExp(`${k}:\\s+(\\d+)`))[1] / 1024); return { rss: g("status", "VmRSS"), pss: g("smaps_rollup", "Pss") }; };
const models = (t) => fx(log, t).filter((x) => x.kind === "model"); const calls = (t) => fx(log, t).filter((x) => x.rpc === "tools/call").map((x) => x.seat);
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];

// The tool name carries a hash. Start once with a placeholder, read it, then write the exact allowlist.
render("placeholder"); start(); let name;
for (let i = 0; i < 100 && !name; i++) { const r = await rpc(P, T, "openhuman.agent_registry_available_tools"); name = r.result?.tools?.map((t) => t.name).find((n) => /^mcp_hivea_hivemind_post/.test(n)); await sleep(100); }
out("tool", name); await stop("SIGTERM"); render(name); start();
await rpc(P, T, "openhuman.agent_registry_available_tools"); await sleep(1500); out("idle MB", mem());
let t = Date.now(), r = await turn(P, T, "th", "CALLTOOL one"); out("tool turn", [r.done?.p.full_response, r.done?.p.usage, calls(t), models(t)[0]?.tools]);
for (const k of ["PLAIN", "CALLTOOL"]) { const d = []; for (let i = 0; i < 7; i++) { const x = await turn(P, T, "perf" + k, k); if (i > 1) d.push(x.dt); } out(`${k} ms`, [d, med(d)]); }
out("after MB", mem());
const slow = async (th) => { const e = await events(P, T, "slow" + th); await rpc(P, T, "openhuman.channel_web_chat", { client_id: "slow" + th, thread_id: th, message: "SLOW" }); await sleep(800); return e; };
let e = await slow("cancel"); t = Date.now(); await rpc(P, T, "openhuman.channel_web_cancel", { client_id: "slowcancel", thread_id: "cancel" }); await sleep(700);
out("cancel closed the model call after ms", fx(log, t).filter((x) => x.kind === "model-disconnect").map((x) => x.t - t)); e.close();
await stop("SIGTERM"); start(); r = await turn(P, T, "th", "PLAIN two"); out("same thread after restart (turns=2 expected)", r.done?.p.full_response);
e = await slow("kill"); await stop("SIGKILL"); e.close(); t = Date.now(); start(); await sleep(5000); out("model calls in 5 s after kill -9 (0 = no replay)", models(t).length);
await stop("SIGTERM"); process.exit(0);
