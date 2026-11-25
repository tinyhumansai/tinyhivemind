// node run.mjs <codex> <work dir> <fixture port> <fixture log>
// The gate and the measurements for codex app-server, against a running fixture.
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process";
import { server, sleep, rss } from "./lib.mjs";
const [codex, work, fport, log] = process.argv.slice(2); const here = path.dirname(new URL(import.meta.url).pathname);
const out = (k, v) => console.log(k, JSON.stringify(v)); const fx = (n = 0) => fs.readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map(JSON.parse).slice(n);
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
for (const d of ["h", "home", "cwd"]) fs.mkdirSync(`${work}/${d}`, { recursive: true });
execFileSync(process.execPath, [`${here}/catalog.mjs`, codex, `${work}/h/catalog.json`]);
fs.writeFileSync(`${work}/home/config.toml`, fs.readFileSync(`${here}/config.toml.in`, "utf8").replaceAll("@FIXTURE_PORT@", fport).replaceAll("@CATALOG@", `${work}/h/catalog.json`));
// MCP and the bearer are sent on every thread/start and thread/resume.
const config = (seat) => ({ mcp_servers: { hive: { url: `http://127.0.0.1:${fport}/mcp`, http_headers: { Authorization: `Bearer seat-${seat}-token` }, required: true, default_tools_approval_mode: "approve" } } });
const thread = (s, seat, model = "mock") => s.call("thread/start", { model, cwd: `${work}/cwd`, config: config(seat) }).then((r) => r.thread.id);

const ready = []; let s;
for (let i = 0; i < 5; i++) { if (s) await s.stop(); const t = performance.now(); s = server(codex, work); await s.init(); await thread(s, "a"); ready.push(Math.round(performance.now() - t)); }
out("cold start to MCP-ready thread ms", [ready, med(ready)]); await sleep(1500); out("idle RSS MB", rss(s.p.pid));
for (const [seat, model] of [["a", "mock"], ["b", "mock"], ["a", "gpt-6.2-sol"]]) { const n = fx().length, id = await thread(s, seat, model), r = await s.turn(id, "CALLTOOL"); out(`seat ${seat} ${model}`, [r, fx(n).filter((x) => x.rpc === "tools/call").map((x) => x.seat), fx(n).find((x) => x.kind === "model")?.tools]); }
const id = await thread(s, "a");
for (const k of ["PLAIN", "CALLTOOL"]) { const d = []; for (let i = 0; i < 6; i++) { const r = await s.turn(id, k); if (i) d.push(r.ms); } out(`${k} ms`, [d, med(d)]); }
out("after RSS MB", rss(s.p.pid));
const slow = async () => { const r = await s.call("turn/start", { threadId: id, input: [{ type: "text", text: "SLOW" }] }); await sleep(700); return r.turn.id; };
let turnId = await slow(), n = fx().length, t = Date.now(); await s.call("turn/interrupt", { threadId: id, turnId }); await s.wait((m) => m.method === "turn/completed" && m.params.turn.id === turnId); await sleep(500);
out("cancel closed the model call after ms", fx(n).filter((x) => x.kind === "model-disconnect").map((x) => x.t - t));
await s.stop(); s = server(codex, work); await s.init(); let q = await s.call("thread/resume", { threadId: id, config: config("a") });
out("same thread after restart", [q.thread.id === id, (await s.turn(id, "CALLTOOL")).items]);
await slow(); s.p.kill("SIGKILL"); await s.exited; n = fx().length; s = server(codex, work); await s.init(); await s.call("thread/resume", { threadId: id, config: config("a") }); await sleep(3000);
out("model calls after kill -9 (0 = no replay)", fx(n).filter((x) => x.kind === "model").length);
out("bogus resume", await s.call("thread/resume", { threadId: "01a213ec-285c-7533-a828-000000000000" }).then(() => "accepted", (e) => e.message.slice(0, 120)));
await s.stop();
