import "../src/main";
import { tuneSpeed } from "../src/game/speed-tuning";
import { idleCommand } from "../src/game/types";
// This page is a dev-only control surface for repeatable Chrome profiling.
const game = (window as any).sloppy;
const panel = document.createElement("aside");
panel.style.cssText = "position:fixed;z-index:100;left:8px;top:8px;background:#102334f2;color:white;padding:12px;font:12px monospace;max-width:650px;max-height:90vh;overflow:auto";
panel.innerHTML = '<button id="normal">Normal · 40 seconds</button> <button id="busy">Crowded · 40 seconds</button> <button id="tree">Tree geometry preview</button><pre id="profile-status">Ready · fixed seed 207 · 2560×1440 · speeds 115%</pre>';
document.body.append(panel);
const output = panel.querySelector("pre")!;
let running = false, busy = false, stressAt = 0;
const gl = game.view.renderer.getContext();
const timer = gl.getExtension("EXT_disjoint_timer_query_webgl2");
const pending: WebGLQuery[] = [], gpu: number[] = [];
let frames = 0;
const render = game.view.render.bind(game.view);
game.view.render = (...args: any[]) => {
  if (running && busy && game.sim.elapsed >= stressAt) {
    stressAt += 6;
    for (let i = 0; i < 100; i++) {
      const a = i * Math.PI * 2 / 100, owner = game.sim.tanks[i % game.sim.tanks.length];
      game.sim.shots.push({id:game.sim.nextId++,x:Math.sin(a)*16,z:Math.cos(a)*16,
        vx:-Math.sin(a)*22,vz:-Math.cos(a)*22,owner:owner.id,team:owner.team,
        damage:40,bounces:3,life:4,weapon:"standard"});
    }
  }
  const measure = running && timer && frames++ % 10 === 0 && pending.length < 4;
  const query = measure ? gl.createQuery() : null;
  if (query) gl.beginQuery(timer.TIME_ELAPSED_EXT, query);
  render(...args);
  if (query) {gl.endQuery(timer.TIME_ELAPSED_EXT);pending.push(query);}
  if (timer && pending.length && gl.getQueryParameter(pending[0], gl.QUERY_RESULT_AVAILABLE)) {
    const q = pending.shift()!;
    if (!gl.getParameter(timer.GPU_DISJOINT_EXT)) gpu.push(gl.getQueryParameter(q, gl.QUERY_RESULT) / 1e6);
    gl.deleteQuery(q);
  }
};
async function run(crowded: boolean) {
  if (running) return;
  document.body.classList.remove("scenery-preview");output.hidden=false;
  game.sim.seed = 207; game.sim.humanTeam = 0; game.sim.humanKind = "balanced";
  game.sim.reset(crowded ? 24 : 12);game.view.reset(game.sim);game.sim.start();
  game.controls.clear();game.autoplay(true);game.overview(false);game.autoRounds(true);
  tuneSpeed(game.sim,"tank-speed",1.15);tuneSpeed(game.sim,"bullet-speed",1.15);
  game.view.zoom=34;game.view.resize(2560,1440,true);
  // Bring the camera into the village and warm the deterministic simulation for combat.
  for(let i=0;i<1800;i++)game.sim.step(idleCommand(),true);
  game.view.reset(game.sim);
  busy = crowded;stressAt = game.sim.elapsed+2;gpu.length=0;frames=0;
  game.record();running=true;
  output.textContent=`Running ${crowded?'crowded':'normal'}…`; 
  const start=performance.now();
  await new Promise<void>(resolve=>{
    const check=()=>{if(performance.now()-start>=40000)resolve();else setTimeout(check,500);};check();
  });
  running=false;
  const result=game.stop();game.sim.match.phase="paused";
  const p=(a:number[],q:number)=>[...a].sort((a,b)=>a-b)[Math.floor((a.length-1)*q)]??null;
  output.textContent=JSON.stringify({scenario:crowded?'crowded':'normal',fps:result.fps,
    frameP50:result.frameP50,frameP95:result.frameP95,frameP99:result.frameP99,
    simulationMean:result.simulationMean,simulationP95:result.simulationP95,
    renderMean:result.renderMean,drawCalls:result.drawCalls,triangles:result.triangles,
    maxBodies:result.maxBodies,maxProjectiles:result.maxProjectiles,maxFragments:result.maxFragments,
    gpuTimerSupported:!!timer,gpuSamples:gpu.length,gpuMedian:p(gpu,.5),gpuP95:p(gpu,.95),
    resolution:result.resolution,activeSeconds:game.sim.elapsed-30,heap:result.memory,
    geometries:game.view.renderer.info.memory.geometries,textures:game.view.renderer.info.memory.textures},null,2);
}
panel.querySelector("#normal")!.addEventListener("click",()=>run(false));
panel.querySelector("#busy")!.addEventListener("click",()=>run(true));
const previewStyle=document.createElement("style");
previewStyle.textContent=".scenery-preview #overlay,.scenery-preview #hud{display:none!important}";
document.head.append(previewStyle);
panel.querySelector("#tree")!.addEventListener("click",()=>{
  if(running)return;
  game.sim.seed=207;game.sim.reset(12);game.view.reset(game.sim);
  const tree=game.sim.covers.find((c:any)=>c.kind==="tree");
  game.sim.human.body.setTranslation({x:tree.x+5,y:.65,z:tree.z+3},true);
  game.sim.human.previous={x:tree.x+5,y:.65,z:tree.z+3};
  game.sim.match.phase="paused";game.overview(false);game.view.zoom=24;
  document.body.classList.add("scenery-preview");output.hidden=true;
});
