// node mkcfg.mjs <dir> <gwport> <modelport> <single:a|b | multi> 
import fs from "node:fs";
const [dir,port,mp,kind,mcpUrl]=process.argv.slice(2);
const DENY=["group:runtime","group:fs","group:web","group:ui","group:automation","group:messaging","group:nodes","group:agents","group:media","group:memory","group:sessions","group:openclaw","transcripts"];
const seats=kind==="multi"?["a","b"]:[kind];
const url=mcpUrl||`http://127.0.0.1:${mp}/mcp`;
const cfg={
 gateway:{mode:"local",bind:"loopback",port:+port,auth:{mode:"token",token:"gw-secret-token"}},
 update:{checkOnStart:false},
 models:{mode:"replace",catalogRefresh:{enabled:false},providers:{proxy:{baseUrl:`http://127.0.0.1:${mp}/v1`,apiKey:"dummy",api:"openai-completions",models:[{id:"mock",name:"mock",contextWindow:200000,maxTokens:8192}]}}},
 agents:{...(seats.length>1?{ownership:"explicit"}:{}),defaults:{model:{primary:"proxy/mock"},skipBootstrap:true,contextInjection:"never",heartbeat:{every:"0m"}},
  entries:Object.fromEntries(seats.map(s=>[s,{workspace:`${dir}/ws-${s}`,tools:{allow:[`hive_${s}__*`],deny:DENY}}]))},
 mcp:{servers:Object.fromEntries(seats.map(s=>[`hive_${s}`,{url,transport:"streamable-http",headers:{Authorization:`Bearer seat-${s}-token`}}]))},
 logging:{file:`${dir}/openclaw.log`},
 plugins:{enabled:process.env.PLUGINS!=="off"},
 tools:{toolSearch:false,codeMode:false,agentToAgent:{enabled:false},sessions:{visibility:"self"},deny:DENY},
};
fs.mkdirSync(dir,{recursive:true});for(const s of seats)fs.mkdirSync(`${dir}/ws-${s}`,{recursive:true});
fs.writeFileSync(`${dir}/openclaw.json`,JSON.stringify(cfg,null,1));
