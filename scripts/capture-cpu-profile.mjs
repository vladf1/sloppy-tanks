// Read-only Chrome DevTools Protocol capture. Navigation and input stay in the browser UI.
import { writeFile } from 'node:fs/promises';
const [label='before-normal', seconds='20'] = process.argv.slice(2);
const targets=await (await fetch('http://127.0.0.1:9227/json/list')).json();
const target=targets.find(t=>t.type==='page' && t.url.includes('/tools/profile.html'));
if(!target)throw new Error('Open the profiling page in the isolated Chrome instance first.');
const socket=new WebSocket(target.webSocketDebuggerUrl),pending=new Map();let id=0;
await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
socket.onmessage=e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(m.error);else p.resolve(m.result);}};
const send=(method,params={})=>new Promise((resolve,reject)=>{const request=++id;pending.set(request,{resolve,reject});socket.send(JSON.stringify({id:request,method,params}));});
await send('Profiler.enable');await send('Profiler.setSamplingInterval',{interval:1000});await send('Profiler.start');
console.log(`Chrome CPU profiler recording ${label} for ${seconds}s`);
await new Promise(r=>setTimeout(r,Number(seconds)*1000));
const {profile}=await send('Profiler.stop');await send('Profiler.disable');socket.close();
await writeFile(`artifacts/profiles/${label}.cpuprofile`,JSON.stringify(profile));
const nodes=new Map(profile.nodes.map(n=>[n.id,n]));const weights=new Map();
for(let i=0;i<(profile.samples??[]).length;i++){const id=profile.samples[i];weights.set(id,(weights.get(id)??0)+(profile.timeDeltas?.[i]??1000));}
const rows=[...weights].map(([id,us])=>({ms:+(us/1000).toFixed(1),fn:nodes.get(id).callFrame.functionName,url:nodes.get(id).callFrame.url,line:nodes.get(id).callFrame.lineNumber+1})).sort((a,b)=>b.ms-a.ms).slice(0,30);
await writeFile(`artifacts/profiles/${label}-top.json`,JSON.stringify(rows,null,2));console.log(JSON.stringify(rows.slice(0,18),null,2));
