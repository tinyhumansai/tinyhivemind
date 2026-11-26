// Scripted model (Chat Completions, Responses and Anthropic Messages, streaming or not) and MCP stub.
// The 2026-11-25 spike's fixture with the Messages API added for Claude Code.
// usage: node fixture.mjs <port> <logfile>   bearer for MCP: seat-a-token -> seat a, seat-b-token -> seat b
// Prompt keywords in the LATEST user message: CALLTOOL -> call *hivemind_post once; SLOW -> stream nothing for 120s.
import http from "node:http"; import fs from "node:fs";
const [port, log] = [Number(process.argv[2]), process.argv[3]];
const L = (o) => fs.appendFileSync(log, JSON.stringify({ t: Date.now(), ...o }) + "\n");
const seats = { "Bearer seat-a-token": "a", "Bearer seat-b-token": "b" };
const read = (q) => new Promise((r) => { let b = ""; q.on("data", (c) => (b += c)); q.on("end", () => r(b)); });
const txt = (c) => typeof c === "string" ? c : (c ?? []).map((p) => p.text ?? "").join("");
let n = 0;
// Anthropic Messages (`POST /v1/messages`): Claude Code's only model API.
const blocks = (c) => typeof c === "string" ? [{ type: "text", text: c }] : (c ?? []);
async function messages(q, s, b, raw, json) {
  if (q.url.includes("/count_tokens")) return json({ input_tokens: 100 });
  const items = b.messages ?? [];
  const said = (m) => m.role === "user" && /CALLTOOL|SLOW|PLAIN/.test(blocks(m.content).map((c) => c.text ?? "").join(""));
  const lastUser = items.findLastIndex(said);
  const ut = lastUser >= 0 ? blocks(items[lastUser].content).map((c) => c.text ?? "").join("") : "";
  const key = [...ut.matchAll(/CALLTOOL|SLOW|PLAIN/g)].at(-1)?.[0];
  const results = items.slice(lastUser + 1).flatMap((m) => blocks(m.content)).filter((c) => c.type === "tool_result");
  const after = results.length > 0;
  const names = (b.tools ?? []).map((t) => t.name ?? t.type);
  const tool = names.find((x) => x?.includes("hivemind_post"));
  L({ kind: "model", api: "messages", model: b.model, stream: !!b.stream, key, after, userTurns: items.filter(said).length, bytes: raw.length, tools: names, toolResults: results.map((r) => JSON.stringify(r.content).slice(0, 200)) });
  q.socket.on("close", () => { if (key === "SLOW" && !s.writableEnded) L({ kind: "model-disconnect" }); });
  if (key === "SLOW") await new Promise((r) => setTimeout(r, 120000));
  const call = key === "CALLTOOL" && !after && tool;
  const text = key === "CALLTOOL" && !after && !tool ? "NO_HIVE_TOOL" : `MOCK_REPLY turns=${items.filter(said).length}`;
  const id = `toolu_${++n}`;
  const content = call ? [{ type: "tool_use", id, name: tool, input: { body: "hello hive" } }] : [{ type: "text", text }];
  const usage = { input_tokens: 100, output_tokens: 7, cache_read_input_tokens: 10, cache_creation_input_tokens: 0 };
  const msg = { id: "msg_" + n, type: "message", role: "assistant", model: b.model, content, stop_reason: call ? "tool_use" : "end_turn", stop_sequence: null, usage };
  if (!b.stream) return json(msg);
  s.writeHead(200, { "content-type": "text/event-stream" });
  const ev = (type, o) => s.write(`event: ${type}\ndata: ${JSON.stringify({ type, ...o })}\n\n`);
  ev("message_start", { message: { ...msg, content: [], stop_reason: null, usage: { ...usage, output_tokens: 1 } } });
  if (call) {
    ev("content_block_start", { index: 0, content_block: { type: "tool_use", id, name: tool, input: {} } });
    ev("content_block_delta", { index: 0, delta: { type: "input_json_delta", partial_json: JSON.stringify({ body: "hello hive" }) } });
  } else {
    ev("content_block_start", { index: 0, content_block: { type: "text", text: "" } });
    ev("content_block_delta", { index: 0, delta: { type: "text_delta", text } });
  }
  ev("content_block_stop", { index: 0 });
  ev("message_delta", { delta: { stop_reason: msg.stop_reason, stop_sequence: null }, usage: { output_tokens: 7 } });
  ev("message_stop", {});
  s.end();
}
http.createServer(async (q, s) => {
  const raw = await read(q); let b = {}; try { b = JSON.parse(raw || "{}"); } catch {}
  const json = (o, c = 200, h = {}) => { s.writeHead(c, { "content-type": "application/json", ...h }); s.end(JSON.stringify(o)); };
  if (q.url.startsWith("/mcp")) {
    const seat = seats[q.headers.authorization];
    L({ kind: "mcp", http: q.method, rpc: b.method, seat: seat ?? null, params: b.params });
    if (!seat) return json({ error: "unauthorized" }, 401);
    if (q.method !== "POST") return json({}, 405);
    if (b.id === undefined) { s.writeHead(202); return s.end(); }
    const ok = (result) => json({ jsonrpc: "2.0", id: b.id, result });
    if (b.method === "initialize") return ok({ protocolVersion: b.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "hive-stub", version: "0" } });
    if (b.method === "tools/list") return ok({ tools: [{ name: "hivemind_post", description: "Post to the episode.", inputSchema: { type: "object", properties: { body: { type: "string" } }, required: ["body"] } }] });
    if (b.method === "tools/call") return ok({ content: [{ type: "text", text: `posted as seat ${seat}` }], isError: false });
    return json({ jsonrpc: "2.0", id: b.id, error: { code: -32601, message: "no method" } });
  }
  if (q.url.includes("/v1/messages")) return messages(q, s, b, raw, json);
  if (q.method === "GET" && q.url.includes("/models")) return json({ object: "list", data: [{ id: "mock", object: "model" }] });
  const resp = q.url.includes("/responses"); const chat = q.url.includes("/chat/completions");
  if (!resp && !chat) { L({ kind: "other", m: q.method, url: q.url }); return json({}, 404); }
  const items = resp ? (Array.isArray(b.input) ? b.input : [{ role: "user", content: b.input }]) : b.messages ?? [];
  const lastUser = items.findLastIndex((m) => m.role === "user" && /CALLTOOL|SLOW|PLAIN/.test(txt(m.content)));
  const ut = lastUser >= 0 ? txt(items[lastUser].content) : "";
  const key = [...ut.matchAll(/CALLTOOL|SLOW|PLAIN/g)].at(-1)?.[0];
  const after = items.slice(lastUser + 1).some((m) => m.role === "tool" || m.type === "function_call_output");
  // Responses may nest MCP tools in {type:"namespace",name,tools:[...]}
  const flat = (b.tools ?? []).flatMap((t) => t.type === "namespace" ? (t.tools ?? []).map((u) => ({ ns: t.name, name: u.name })) : [{ name: t.name ?? t.function?.name ?? t.type }]);
  const tool = flat.find((t) => t.name?.includes("hivemind_post"));
  const marks = items.filter((m) => m.role === "user" && /CALLTOOL|SLOW|PLAIN/.test(txt(m.content))).length;
  L({ kind: "model", api: resp ? "responses" : "chat", model: b.model, stream: !!b.stream, key, after, userTurns: marks, bytes: raw.length, tools: flat.map((t) => (t.ns ? t.ns + "/" : "") + t.name), toolResults: items.filter((m) => m.role === "tool" || m.type === "function_call_output").map((m) => txt(m.content ?? m.output).slice(0, 200)) });
  q.socket.on("close", () => { if (key === "SLOW" && !s.writableEnded) L({ kind: "model-disconnect" }); });
  if (key === "SLOW") await new Promise((r) => setTimeout(r, 120000));
  const call = key === "CALLTOOL" && !after && tool; const id = `call_${++n}`; const args = '{"body":"hello hive"}';
  const text = key === "CALLTOOL" && !after && !tool ? "NO_HIVE_TOOL" : `MOCK_REPLY turns=${marks}`;
  const sse = (o, ev) => s.write((ev ? `event: ${ev}\n` : "") + `data: ${typeof o === "string" ? o : JSON.stringify(o)}\n\n`);
  if (resp) {
    const item = call ? { type: "function_call", id: "fc_" + n, call_id: id, name: tool.name, ...(tool.ns ? { namespace: tool.ns } : {}), arguments: args, status: "completed" }
      : { type: "message", id: "msg_" + ++n, role: "assistant", status: "completed", content: [{ type: "output_text", text, annotations: [] }] };
    const r = { id: "resp_" + n, object: "response", status: "completed", model: b.model, output: [item], usage: { input_tokens: 100, input_tokens_details: { cached_tokens: 10 }, output_tokens: 7, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 107 } };
    if (!b.stream) return json(r);
    s.writeHead(200, { "content-type": "text/event-stream" });
    sse({ type: "response.created", response: { ...r, status: "in_progress", output: [] } }, "response.created");
    sse({ type: "response.output_item.added", output_index: 0, item: { ...item, status: "in_progress" } }, "response.output_item.added");
    if (!call) sse({ type: "response.output_text.delta", item_id: item.id, output_index: 0, content_index: 0, delta: text }, "response.output_text.delta");
    sse({ type: "response.output_item.done", output_index: 0, item }, "response.output_item.done");
    sse({ type: "response.completed", response: r }, "response.completed"); return s.end();
  }
  const usage = { prompt_tokens: 100, completion_tokens: 7, total_tokens: 107, prompt_tokens_details: { cached_tokens: 10 } };
  const tc = [{ index: 0, id, type: "function", function: { name: tool?.name, arguments: args } }];
  const base = { id: "c" + n, created: 1, model: b.model };
  if (!b.stream) return json({ ...base, object: "chat.completion", choices: [{ index: 0, finish_reason: call ? "tool_calls" : "stop", message: call ? { role: "assistant", content: null, tool_calls: tc } : { role: "assistant", content: text } }], usage });
  s.writeHead(200, { "content-type": "text/event-stream" });
  const ch = (delta, f = null) => sse({ ...base, object: "chat.completion.chunk", choices: [{ index: 0, delta, finish_reason: f }] });
  ch(call ? { role: "assistant", tool_calls: tc } : { role: "assistant", content: text }); ch({}, call ? "tool_calls" : "stop");
  sse({ ...base, object: "chat.completion.chunk", choices: [], usage }); sse("[DONE]"); s.end();
}).listen(port, "127.0.0.1", () => console.log("ready"));
