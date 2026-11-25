import crypto from "node:crypto";import fs from "node:fs";
export const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const b64u=b=>Buffer.from(b).toString("base64url");
export function identity(file){ if(fs.existsSync(file)){const j=JSON.parse(fs.readFileSync(file));return{...j,priv:crypto.createPrivateKey(j.pem)}}
 const {publicKey,privateKey}=crypto.generateKeyPairSync("ed25519"); const raw=publicKey.export({type:"spki",format:"der"}).subarray(-32);
 const j={id:crypto.createHash("sha256").update(raw).digest("hex"),pub:b64u(raw),pem:privateKey.export({type:"pkcs8",format:"pem"})};fs.writeFileSync(file,JSON.stringify(j),{mode:0o600});return{...j,priv:privateKey}}
export async function connect({port,idFile,token="gw-secret-token",deviceToken,scopes=["operator.read","operator.write"],clientId="cli",mode="cli",log,caps=["tool-events"],noDevice=false,min=4,max=4}){
 const L=(d,o)=>log&&fs.appendFileSync(log,JSON.stringify({t:Date.now(),d,f:typeof o==="string"?JSON.parse(o):o})+"\n");
 const ws=new WebSocket(`ws://127.0.0.1:${port}`);const pend=new Map();const events=[];const waiters=[];let n=0;
 const c={ws,events,closed:null,handlers:[]};
 c.send=(method,params,tmo=200000)=>{const id=`r${++n}`;const f={type:"req",id,method,params};L("out",f);ws.send(JSON.stringify(f));return new Promise((res,rej)=>{const to=setTimeout(()=>rej(new Error("rpc timeout "+method)),tmo);pend.set(id,{res:m=>{clearTimeout(to);res(m)},rej})})};
 c.close=()=>ws.close();
 const ch=new Promise((res,rej)=>{ws.onmessage=m=>{const f=JSON.parse(m.data);L("in",f);if(f.type==="event"){events.push({t:Date.now(),...f});c.handlers.forEach(h=>h(f));if(f.event==="connect.challenge")res(f.payload)}else if(f.type==="res"){const p=pend.get(f.id);if(p&&!(f.ok&&f.payload?.status==="accepted"&&p.keep)){pend.delete(f.id);p.res(f)}}};
  ws.onerror=e=>rej(new Error("ws error"));ws.onclose=e=>{c.closed={code:e.code,reason:e.reason};L("in",{type:"close",...c.closed});for(const p of pend.values())p.rej(new Error("closed "+e.code+" "+e.reason));rej(new Error("closed"))}});
 const {nonce,ts}=await ch;c.tChallenge=Date.now();
 const id=noDevice?null:identity(idFile);const role="operator",platform="linux",tok=deviceToken??token;
 const params={minProtocol:min,maxProtocol:max,client:{id:clientId,version:"0.1.0",platform,mode},role,scopes,caps,commands:[],permissions:{},auth:deviceToken?{deviceToken}:{token},locale:"en-US",userAgent:"hive-spike/0.1"};
 if(id){const payload=["v3",id.id,clientId,mode,role,scopes.join(","),String(ts),tok,nonce,platform,""].join("|");
 params.device={id:id.id,publicKey:id.pub,signature:b64u(crypto.sign(null,Buffer.from(payload),id.priv)),signedAt:ts,nonce}}
 c.hello=await c.send("connect",params);return c}
export async function ready(opts,tmo=60000){const t0=Date.now();let last;while(Date.now()-t0<tmo){try{const c=await connect(opts);if(c.hello.ok)return c;last=c.hello.error;c.close();if(last.code!=="UNAVAILABLE")throw Object.assign(new Error(JSON.stringify(last)),{fatal:1})}catch(e){if(e.fatal)throw e;last=e.message}await sleep(50)}throw new Error("not ready "+JSON.stringify(last))}
export async function turn(c,p,timeoutMs=30000){const t0=Date.now();const a=await c.send("agent",{deliver:false,...p});if(!a.ok)return{accept:a};const runId=a.payload.runId;const w=await c.send("agent.wait",{runId,timeoutMs});return{accept:a,wait:w,ms:Date.now()-t0}}
