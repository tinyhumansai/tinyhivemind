// node measure.mjs <claude|codex|openclaw> <pkgs dir> <work dir> <fixture port> <fixture log> [gateway port]
// One agent over ACP against the running fixture: start-up, the hive tool as
// seat a, usage, turn latency, memory, cancel, resume after restart, kill -9.
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process";
import { agent, memory, tree, median, sleep } from "./acp.mjs";

const [kind, pkgs, work, fport, flog, gport] = process.argv.slice(2);
const here = path.dirname(new URL(import.meta.url).pathname);
const spike = `${here}/../2026-11-25-process-harnesses`;
const node = process.execPath;
const out = (k, v) => console.log(JSON.stringify({ kind, k, v }));
const fx = (n = 0) => fs.readFileSync(flog, "utf8").trim().split("\n").filter(Boolean).map(JSON.parse).slice(n);
for (const d of ["home", "tmp", "cwd"]) fs.mkdirSync(`${work}/${d}`, { recursive: true });
const base = { PATH: `${path.dirname(node)}:/usr/bin:/bin`, HOME: `${work}/home`, TMPDIR: `${work}/tmp` };
const hive = [{ type: "http", name: "hive", url: `http://127.0.0.1:${fport}/mcp`, headers: [{ name: "Authorization", value: "Bearer seat-a-token" }] }];

let gateway = null;
const setups = {
  claude: () => ({
    cmd: node, args: [`${pkgs}/claude/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js`],
    env: { ...base, CLAUDE_CONFIG_DIR: `${work}/claude`, ANTHROPIC_BASE_URL: `http://127.0.0.1:${fport}`, ANTHROPIC_API_KEY: "dummy", ANTHROPIC_MODEL: "mock", ANTHROPIC_SMALL_FAST_MODEL: "mock", DISABLE_TELEMETRY: "1", CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1", DISABLE_AUTOUPDATER: "1", DISABLE_ERROR_REPORTING: "1", ENABLE_TOOL_SEARCH: "false" },
    mcp: hive,
    // No built-in tools, no settings files, and the hive tool allowed without a prompt.
    meta: { claudeCode: { options: { tools: [], settingSources: [], allowedTools: ["mcp__hive__hivemind_post"] } } },
  }),
  codex: () => {
    const home = `${work}/codex`; fs.mkdirSync(home, { recursive: true });
    const bin = execFileSync("find", [`${pkgs}/codex/node_modules/@openai`, "-type", "f", "-name", "codex", "-path", "*vendor*"], { encoding: "utf8" }).trim().split("\n")[0];
    execFileSync(node, [`${spike}/codex/catalog.mjs`, bin, `${home}/catalog.json`]);
    // The fixture's model is `mock`: give it the first model's metadata.
    const catalog = JSON.parse(fs.readFileSync(`${home}/catalog.json`, "utf8"));
    catalog.models.unshift({ ...catalog.models[0], slug: "mock", display_name: "mock" });
    fs.writeFileSync(`${home}/catalog.json`, JSON.stringify(catalog));
    const cfg = fs.readFileSync(`${spike}/codex/config.toml.in`, "utf8").replaceAll("@FIXTURE_PORT@", fport).replaceAll("@CATALOG@", `${home}/catalog.json`);
    fs.writeFileSync(`${home}/config.toml`, cfg);
    return {
      cmd: node, args: [`${pkgs}/codex/node_modules/@agentclientprotocol/codex-acp/dist/index.js`],
      env: { ...base, CODEX_HOME: home, MOCK_KEY: "dummy" }, mcp: hive,
      // The adapter's default mode has an AI reviewer approve each tool call;
      // read-only sends approvals to the client instead.
      mode: "read-only",
    };
  },
  openclaw: () => {
    const pkg = `${pkgs}/openclaw/node_modules/openclaw`;
    execFileSync(node, [`${spike}/openclaw/mkcfg.mjs`, `${work}/oc`, gport, fport, "a"]);
    fs.mkdirSync(`${work}/oc/state`, { recursive: true });
    const env = { ...base, OPENCLAW_STATE_DIR: `${work}/oc/state`, OPENCLAW_CONFIG_PATH: `${work}/oc/openclaw.json`, OPENCLAW_SKIP_CHANNELS: "1", OPENCLAW_DISABLE_BONJOUR: "1", OPENCLAW_NO_RESPAWN: "1", OPENCLAW_EXEC_SHELL_SNAPSHOT: "0" };
    return {
      gateway: { cmd: node, args: [`${pkg}/openclaw.mjs`, "gateway"], env },
      cmd: node, args: [`${pkg}/openclaw.mjs`, "acp", "--url", `ws://127.0.0.1:${gport}`, "--token", "gw-secret-token", "--session", "agent:a:acp", "--no-prefix-cwd"],
      env, mcp: [],
    };
  },
};
const setup = setups[kind]();

const roots = () => [a.child.pid, ...(gateway ? [gateway.child.pid] : [])];
const mem = () => roots().map(memory).reduce((x, y) => ({ processes: x.processes + y.processes, rssMB: x.rssMB + y.rssMB, pssMB: x.pssMB + y.pssMB, commands: [...x.commands, ...y.commands] }));
const waitGateway = async () => {
  const t0 = Date.now();
  for (;;) { try { const r = await fetch(`http://127.0.0.1:${gport}/`); if (r) break; } catch {} if (Date.now() - t0 > 60000) throw new Error("gateway never listened"); await sleep(200); }
  return Date.now() - t0;
};

let a;
const start = async () => {
  if (setup.gateway && !gateway) {
    gateway = agent(setup.gateway.cmd, setup.gateway.args, { env: setup.gateway.env, cwd: `${work}/cwd`, log: `${work}/gateway.log` });
    out("gateway listening ms", await waitGateway());
  }
  const t0 = Date.now();
  a = agent(setup.cmd, setup.args, { env: setup.env, cwd: `${work}/cwd`, log: `${work}/agent.log` });
  const init = await a.call("initialize", { protocolVersion: 1, clientCapabilities: { fs: { readTextFile: false, writeTextFile: false }, terminal: false }, clientInfo: { name: "thm-acp-spike", version: "0" } });
  return { ms: Date.now() - t0, init };
};
const prompt = async (sessionId, text) => {
  const t0 = Date.now(), n = a.updates.length;
  const r = await a.call("session/prompt", { sessionId, prompt: [{ type: "text", text }] });
  const ups = a.updates.slice(n);
  const reply = ups.filter((u) => u.sessionUpdate === "agent_message_chunk").map((u) => u.content?.text ?? "").join("");
  return { ms: Date.now() - t0, stopReason: r.stopReason, usage: r.usage ?? r._meta?.usage ?? null, meta: r._meta ? Object.keys(r._meta) : [], reply: reply.slice(0, 80), updates: [...new Set(ups.map((u) => u.sessionUpdate))], usageUpdates: ups.filter((u) => /usage/i.test(u.sessionUpdate)).map((u) => u.used ?? u.usage ?? u) };
};

try {
  const s = await start();
  out("cold start to initialize ms", s.ms);
  out("agent capabilities", s.init.agentCapabilities);
  let t0 = Date.now();
  const sess = await a.call("session/new", { cwd: `${work}/cwd`, mcpServers: setup.mcp, ...(setup.meta ? { _meta: setup.meta } : {}) });
  out("session/new ms", Date.now() - t0);
  out("session/new keys", Object.keys(sess));
  const id = sess.sessionId;
  if (setup.mode) { await a.call("session/set_mode", { sessionId: id, modeId: setup.mode }); out("mode", setup.mode); }
  out("warm PLAIN", await prompt(id, "PLAIN"));
  let n = fx().length;
  const tool = await prompt(id, "CALLTOOL");
  out("CALLTOOL", tool);
  out("hive tool calls by seat", fx(n).filter((x) => x.rpc === "tools/call").map((x) => x.seat));
  out("tools the model was offered", fx(n).find((x) => x.kind === "model")?.tools);
  out("permission requests so far", a.requests.map((r) => r.method));
  await sleep(10000); out("idle memory (10 s)", mem());
  for (const k of ["PLAIN", "CALLTOOL"]) { const d = []; for (let i = 0; i < 5; i++) d.push((await prompt(id, k)).ms); out(`${k} ms`, { runs: d, median: median(d) }); }
  out("memory after 13 turns", mem());
  n = fx().length;
  const slow = a.call("session/prompt", { sessionId: id, prompt: [{ type: "text", text: "SLOW" }] });
  while (!fx(n).some((x) => x.kind === "model" && x.key === "SLOW")) await sleep(50);
  await sleep(300); t0 = Date.now(); a.notify("session/cancel", { sessionId: id });
  const cancelled = await Promise.race([slow.then((r) => r.stopReason, (e) => "error: " + e.message), sleep(15000).then(() => "no answer in 15 s")]);
  await sleep(1000);
  out("cancel", { stopReason: cancelled, answeredAfterMs: Date.now() - t0 - 1000, modelCallClosedAfterMs: fx(n).filter((x) => x.kind === "model-disconnect").map((x) => x.t - t0) });
  // Restart the agent (not the gateway) and resume the session.
  const pids = tree(a.child.pid); a.child.stdin.end(); process.kill(-a.child.pid, "SIGTERM");
  await Promise.race([a.exited, sleep(5000)]); for (const p of pids) try { process.kill(p, "SIGKILL"); } catch {}
  const s2 = await start();
  if (s.init.agentCapabilities?.loadSession) {
    try { await a.call("session/load", { sessionId: id, cwd: `${work}/cwd`, mcpServers: setup.mcp, ...(setup.meta ? { _meta: setup.meta } : {}) }); out("resumed after restart", await prompt(id, "PLAIN")); }
    catch (e) { out("resume failed", e.rpc ?? e.message); }
  } else out("resume", "agent does not offer loadSession");
  // kill -9 mid-turn: what is left, and does a new agent replay the turn?
  const id2 = (await a.call("session/new", { cwd: `${work}/cwd`, mcpServers: setup.mcp, ...(setup.meta ? { _meta: setup.meta } : {}) })).sessionId;
  if (setup.mode) await a.call("session/set_mode", { sessionId: id2, modeId: setup.mode });
  n = fx().length; a.call("session/prompt", { sessionId: id2, prompt: [{ type: "text", text: "SLOW" }] }).catch(() => {});
  while (!fx(n).some((x) => x.kind === "model" && x.key === "SLOW")) await sleep(50);
  const before = tree(a.child.pid); process.kill(a.child.pid, "SIGKILL"); await sleep(2000);
  out("left after kill -9 of the agent", before.filter((p) => fs.existsSync(`/proc/${p}`)).length);
  for (const p of before) try { process.kill(p, "SIGKILL"); } catch {}
  n = fx().length; await start(); await sleep(15000);
  // The interrupted session is not loaded: this shows only what a new agent
  // does when it starts, not whether loading that session replays its turn.
  out("model calls a new agent made on its own within 15 s (session not loaded)", fx(n).filter((x) => x.kind === "model").length);
} catch (e) {
  out("error", e.rpc ?? e.message);
} finally {
  for (const h of [a, gateway]) if (h) { for (const p of tree(h.child.pid)) try { process.kill(p, "SIGKILL"); } catch {} }
  process.exit(0);
}
