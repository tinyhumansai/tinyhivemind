// A minimal Agent Client Protocol client over stdio (newline-delimited
// JSON-RPC), and the process-tree measurements the runs share.
import { spawn, execSync } from "node:child_process";
import fs from "node:fs";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Start `cmd` as an ACP agent in its own process group. */
export function agent(cmd, args, { env, cwd, log }) {
  const child = spawn(cmd, args, { env, cwd, detached: true, stdio: ["pipe", "pipe", "pipe"] });
  const err = fs.openSync(log, "a");
  child.stderr.on("data", (d) => fs.writeSync(err, d));
  let buf = "", id = 0;
  const pending = new Map();
  const updates = [];
  const requests = [];
  const write = (o) => child.stdin.write(JSON.stringify(o) + "\n");
  child.stdout.on("data", (d) => {
    buf += d;
    for (let i; (i = buf.indexOf("\n")) >= 0;) {
      const line = buf.slice(0, i).trim(); buf = buf.slice(i + 1);
      if (!line) continue;
      let m; try { m = JSON.parse(line); } catch { fs.writeSync(err, "non-json: " + line + "\n"); continue; }
      if (m.id !== undefined && (m.result !== undefined || m.error !== undefined) && pending.has(m.id)) {
        const p = pending.get(m.id); pending.delete(m.id);
        m.error ? p.reject(Object.assign(new Error(m.error.message), { rpc: m.error })) : p.resolve(m.result);
      } else if (m.method === "session/update") {
        updates.push({ t: Date.now(), ...m.params.update });
      } else if (m.method && m.id !== undefined) {
        // A request from the agent: allow tool permissions once, refuse the rest.
        requests.push({ t: Date.now(), method: m.method, params: m.params });
        if (m.method === "session/request_permission") {
          const allow = m.params.options?.find((o) => o.kind?.startsWith("allow"));
          write({ jsonrpc: "2.0", id: m.id, result: { outcome: allow ? { outcome: "selected", optionId: allow.optionId } : { outcome: "cancelled" } } });
        } else {
          write({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "not offered" } });
        }
      }
    }
  });
  const call = (method, params) => new Promise((resolve, reject) => {
    const n = ++id; pending.set(n, { resolve, reject }); write({ jsonrpc: "2.0", id: n, method, params });
  });
  const notify = (method, params) => write({ jsonrpc: "2.0", method, params });
  const exited = new Promise((r) => child.on("exit", (code, signal) => r({ code, signal })));
  return { child, call, notify, updates, requests, exited };
}

/** The pids of `root` and every descendant. */
export function tree(root) {
  const all = execSync("ps -eo pid,ppid", { encoding: "utf8" }).trim().split("\n").slice(1).map((l) => l.trim().split(/\s+/).map(Number));
  const s = [root];
  for (let i = 0; i < s.length; i++) for (const [a, b] of all) if (b === s[i]) s.push(a);
  return s;
}

/** RSS and PSS of a process tree, in MB, and its size. */
export function memory(root) {
  const pids = tree(root);
  let rss = 0, pss = 0;
  for (const p of pids) {
    try {
      const r = fs.readFileSync(`/proc/${p}/smaps_rollup`, "utf8");
      rss += +r.match(/Rss:\s+(\d+)/)[1]; pss += +r.match(/Pss:\s+(\d+)/)[1];
    } catch {}
  }
  return { processes: pids.length, rssMB: Math.round(rss / 1024), pssMB: Math.round(pss / 1024), commands: pids.map((p) => { try { return fs.readFileSync(`/proc/${p}/cmdline`, "utf8").split("\0").slice(0, 3).join(" ").slice(0, 90); } catch { return "?"; } }) };
}

export const median = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
