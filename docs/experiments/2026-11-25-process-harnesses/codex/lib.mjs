// A minimal codex app-server client: one JSON object per line over stdio.
import { spawn } from "node:child_process"; import fs from "node:fs"; import readline from "node:readline";
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export const rss = (pid) => Math.round(+fs.readFileSync(`/proc/${pid}/status`, "utf8").match(/VmRSS:\s+(\d+)/)[1] / 1024);
export function server(codex, work) {
  const t0 = performance.now();
  const p = spawn(codex, ["app-server"], { env: { PATH: "/usr/bin:/bin", HOME: `${work}/h`, CODEX_HOME: `${work}/home`, MOCK_KEY: "dummy" }, cwd: `${work}/cwd`, stdio: ["pipe", "pipe", "pipe"] });
  const log = fs.createWriteStream(`${work}/app-server.log`, { flags: "a" }); p.stderr.on("data", (d) => log.write("STDERR " + d));
  let id = 0; const pend = new Map(), subs = new Set(), notes = [];
  const send = (o) => { log.write("> " + JSON.stringify(o) + "\n"); p.stdin.write(JSON.stringify(o) + "\n"); };
  readline.createInterface({ input: p.stdout }).on("line", (l) => {
    log.write("< " + l + "\n"); const m = JSON.parse(l);
    if (m.method) { notes.push(m); for (const s of subs) s(m); return; }
    const q = pend.get(m.id); pend.delete(m.id); m.error ? q[1](new Error(JSON.stringify(m.error))) : q[0](m.result);
  });
  const call = (method, params = {}) => new Promise((a, b) => { const i = ++id; pend.set(i, [a, b]); send({ id: i, method, params }); });
  const wait = (f, ms = 40000) => new Promise((a, b) => {
    const hit = notes.find(f); if (hit) return a(hit);
    const s = (m) => { if (f(m)) { clearTimeout(t); subs.delete(s); a(m); } }, t = setTimeout(() => { subs.delete(s); b(new Error("wait timeout")); }, ms); subs.add(s);
  });
  const init = async () => { await call("initialize", { clientInfo: { name: "hive_spike", title: null, version: "0" }, capabilities: { experimentalApi: true } }); send({ method: "initialized" }); return Math.round(performance.now() - t0); };
  const turn = async (threadId, text) => {
    const a = performance.now(), r = await call("turn/start", { threadId, input: [{ type: "text", text }] }), tid = r.turn.id;
    const d = await wait((m) => m.method === "turn/completed" && m.params.turn.id === tid);
    const items = notes.filter((m) => m.method === "item/completed" && m.params.turnId === tid).map((m) => m.params.item);
    return { ms: Math.round(performance.now() - a), status: d.params.turn.status, items: items.filter((i) => i.type !== "userMessage").map((i) => i.text ?? [i.type, i.status]), total: notes.findLast((m) => m.method === "thread/tokenUsage/updated" && m.params.threadId === threadId)?.params.tokenUsage?.total };
  };
  const exited = new Promise((r) => p.on("exit", r));
  const stop = async () => { p.stdin.end(); const t = setTimeout(() => p.kill("SIGKILL"), 5000); await exited; clearTimeout(t); };
  return { p, call, wait, init, turn, stop, exited };
}
