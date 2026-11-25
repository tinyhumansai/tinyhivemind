// node run.mjs <openclaw package dir> <work dir> <gateway port> <fixture port> <fixture log>
// The gate and the measurements for an OpenClaw Gateway with seats a and b, against a running fixture.
import fs from "node:fs"; import path from "node:path"; import { spawn, execFileSync, execSync } from "node:child_process";
import { ready, turn, sleep } from "./gw.mjs";
const [pkg, work, port, fport, log] = process.argv.slice(2); const here = path.dirname(new URL(import.meta.url).pathname);
const node = process.execPath; const out = (k, v) => console.log(k, JSON.stringify(v));
const fx = (n = 0) => fs.readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map(JSON.parse).slice(n);
execFileSync(node, [`${here}/mkcfg.mjs`, work, port, fport, "multi"]);
for (const d of ["home", "state", "tmp"]) fs.mkdirSync(`${work}/${d}`, { recursive: true });
const env = { HOME: `${work}/home`, PATH: `${path.dirname(node)}:/usr/bin:/bin`, TMPDIR: `${work}/tmp`, OPENCLAW_STATE_DIR: `${work}/state`, OPENCLAW_CONFIG_PATH: `${work}/openclaw.json`, OPENCLAW_SKIP_CHANNELS: "1", OPENCLAW_DISABLE_BONJOUR: "1", OPENCLAW_NO_RESPAWN: "1", OPENCLAW_EXEC_SHELL_SNAPSHOT: "0" };
let gw;
const start = async () => { const t0 = Date.now(), f = fs.openSync(`${work}/gw.log`, "a"); gw = spawn(node, [`${pkg}/openclaw.mjs`, "gateway"], { env, cwd: work, detached: true, stdio: ["ignore", f, f] }); const c = await ready({ port: +port, idFile: `${work}/id.json`, log: `${work}/frames.jsonl` }); out("hello ms", Date.now() - t0); return c; };
const tree = () => { const all = execSync("ps -eo pid,ppid", { encoding: "utf8" }).trim().split("\n").slice(1).map((l) => l.trim().split(/\s+/).map(Number)); const s = [gw.pid]; for (let i = 0; i < s.length; i++) for (const [a, b] of all) if (b === s[i]) s.push(a); return s; };
const pss = () => Math.round(tree().reduce((a, p) => { try { return a + +fs.readFileSync(`/proc/${p}/smaps_rollup`, "utf8").match(/Pss:\s+(\d+)/)[1]; } catch { return a; } }, 0) / 1024);
const kill = async (sig) => { const ps = tree(), done = new Promise((r) => gw.on("exit", r)); process.kill(gw.pid, sig); await done; out("left after " + sig, ps.filter((p) => fs.existsSync(`/proc/${p}`)).length); for (const p of ps) try { process.kill(p, "SIGKILL"); } catch {} await sleep(500); };
let id = 0; const run = async (c, seat, key, message) => { for (;;) { const r = await turn(c, { agentId: seat, sessionKey: `agent:${seat}:${key}`, message, idempotencyKey: `${process.pid}-${++id}` }); if (r.accept.ok || r.accept.error?.code !== "UNAVAILABLE") return r; await sleep(300); } };
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];

let c = await start(); await run(c, "a", "warm", "PLAIN");
for (const s of ["a", "b"]) { const n = fx().length, r = await run(c, s, "ep", "CALLTOOL"); out("seat " + s, [r.wait?.payload?.terminalReply?.text, fx(n).filter((x) => x.rpc === "tools/call").map((x) => x.seat), fx(n).find((x) => x.kind === "model")?.tools]); }
// A turn's usage is on the second response to the same `agent` request, so read it from the frame log.
{ await run(c, "a", "ep", "PLAIN"); await sleep(1000); const f = fs.readFileSync(`${work}/frames.jsonl`, "utf8").trim().split("\n").map(JSON.parse); const m = f.findLast((x) => x.d === "in" && x.f.type === "res" && x.f.payload?.result); out("usage", m?.f.payload.result.meta?.agentMeta?.usage ?? Object.keys(m?.f.payload ?? {})); }
await sleep(30000); out("idle PSS MB (30 s)", pss());
for (const k of ["PLAIN", "CALLTOOL"]) { const d = []; for (let i = 0; i < 5; i++) { d.push((await run(c, "a", "perf", k)).ms); await sleep(300); } out(`${k} ms`, [d, med(d)]); }
out("after PSS MB", pss());
const slow = async (key) => { const n = fx().length; await c.send("agent", { agentId: "a", sessionKey: key, message: "SLOW", idempotencyKey: key, deliver: false }); while (!fx(n).some((x) => x.kind === "model")) await sleep(50); await sleep(300); return n; };
await slow("agent:a:cancel"); let t = Date.now(); await c.send("chat.abort", { sessionKey: "agent:a:cancel" }); await sleep(1000);
out("cancel closed the model call after ms", fx().filter((x) => x.kind === "model-disconnect" && x.t >= t).map((x) => x.t - t));
c.close(); await kill("SIGTERM"); c = await start(); out("same session after restart", (await run(c, "a", "ep", "PLAIN")).wait?.payload?.terminalReply?.text);
await slow("agent:a:crash"); c.close(); await kill("SIGKILL"); const n = fx().length; t = Date.now(); c = await start(); await sleep(20000);
out("model calls after kill -9, ms (non-empty = self-resume)", fx(n).filter((x) => x.kind === "model").map((x) => x.t - t));
c.close(); await kill("SIGTERM"); process.exit(0);
