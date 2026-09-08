// @ts-check
import { chromium } from 'playwright';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';

const label = process.argv[2] ?? 'before';
const url = process.env.SLOPPY_URL ?? 'http://127.0.0.1:5173/sloppy-tanks/';
if (!['before', 'after'].includes(label)) throw new Error('Use before or after as the measurement label.');
const out = `artifacts/performance/${label}`;
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: 'chrome', headless: false,
  args: ['--window-size=2560,1440', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding'] });
const context = await browser.newContext({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
const page = await context.newPage();
/** @type {string[]} */
const errors = [];
page.on('pageerror', e => errors.push(e.message));
let navigations = 0;
page.on('framenavigated', frame => { if (frame === page.mainFrame()) navigations++; });
const cdp = await context.newCDPSession(page);
/** @type {Array<ReturnType<Window['sloppy']['report']> & {scenario: string, seed: number}>} */
const runs = [];
/** @type {Record<string, [string, number][]>} */
const profiles = {};
const results = { label, date: new Date().toISOString(), chrome: browser.version(), resolution: [2560,1440], seeds: [12345, 45678, 98765], warmupSeconds: 5, sampleSeconds: 15, errors, runs, profiles, complete: false };
const save = () => writeFileSync(`${out}/results.json`, JSON.stringify(results, null, 2));
const checkErrors = () => { if (errors.length) throw new Error(errors.join('\n')); };
/** @param {string} scenario @param {number} seed */
async function setup(scenario, seed) {
  await page.goto(`${url}?autoplay`);
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(({ scenario, seed }) => {
    const d = window.sloppy;
    d.sim.seed = seed;
    d.sim.roundCount = 12;
    d.sim.humanTeam = 0;
    d.start();
    if (scenario === 'stress') d.stress();
    d.exactResolution();
    d.overview(scenario === 'stress');
    d.autoplay();
    if (scenario === 'stress') {
      let nextBurst = 5;
      const step = d.sim.step.bind(d.sim);
      d.sim.step = (...args) => {
        step(...args);
        if (d.sim.elapsed < nextBurst) return;
        nextBurst += 5;
        const s = d.sim;
        for (let i = s.fragments.length; i < s.maxFragments; i++) s.fragment(s.rng.range(-15,15),s.rng.range(-15,15),0xc5a978,0.5);
        for (let i = s.shots.length; i < 200; i++) {
          const a = i * Math.PI * 2 / 200;
          s.shots.push({id:s.nextId++,x:Math.sin(a)*15,z:Math.cos(a)*15,vx:Math.cos(a)*45,vz:Math.sin(a)*45,owner:s.tanks[i%24].id,team:i%2 === 0 ? 0 : 1,damage:40,bounces:4,life:4,piercing:0,weapon:'standard'});
        }
        for (const x of [-5,0,5]) {
          const c = s.addCover({kind:'drum',x,z:0,w:1.2,d:1.2,h:1.7,hp:30,color:0xe3854d});
          if (x === 5) s.damageCover(c,999,s.human.id,s.humanTeam);
        }
      };
    }
    d.record();
  }, {scenario, seed});
}
try {
  for (const scenario of ['normal', 'stress']) {
    for (const seed of results.seeds) {
      await setup(scenario, seed);
      const navigation = navigations;
      await page.waitForTimeout(20000);
      checkErrors();
      if (navigations !== navigation) throw new Error('Page reloaded during timing; discard this run and repeat.');
      const report = await page.evaluate(() => window.sloppy.stop());
      if (report.snapshot.elapsed < 18) throw new Error('Measurement paused or could not keep up: ' + report.snapshot.elapsed);
      results.runs.push({scenario, seed, ...report}); save();
      console.log(scenario, seed, JSON.stringify({fps:report.fps, p95:report.frameP95, sim:report.simulationMean,render:report.renderMean,calls:report.drawCalls}));
    }
    await setup(scenario, 12345);
    await page.waitForTimeout(6000);
    await cdp.send('Profiler.enable');
    await cdp.send('Profiler.setSamplingInterval', {interval: 1000});
    await cdp.send('Profiler.start');
    await page.waitForTimeout(10000);
    const {profile} = await cdp.send('Profiler.stop');
    checkErrors();
    if (!profile.samples || !profile.timeDeltas) throw new Error('CPU profile contains no samples.');
    writeFileSync(`${out}/${scenario}.cpuprofile`, JSON.stringify(profile));
    const nodes = new Map(profile.nodes.map(n=>[n.id,n]));
    const self = new Map();
    for (let i=0;i<profile.samples.length;i++) {
      const n=nodes.get(profile.samples[i]);
      if (!n) throw new Error('CPU profile references a missing node.');
      const key=`${n.callFrame.functionName || '(anonymous)'} @ ${n.callFrame.url}:${n.callFrame.lineNumber+1}`;
      self.set(key,(self.get(key)||0)+profile.timeDeltas[i]/1000);
    }
    results.profiles[scenario] = [...self].sort((a,b)=>b[1]-a[1]).slice(0,35);
    await page.screenshot({path:`${out}/${scenario}.png`});
    save(); console.log('PROFILE',scenario,JSON.stringify(results.profiles[scenario].slice(0,15)));
  }
  results.complete = true;
} finally { save(); await browser.close(); }
// Keep the notebook data compact; full snapshots, screenshots and importable
// DevTools profiles remain in the local, ignored performance directory.
const paths = ['before', 'after'].map(name => `artifacts/performance/${name}/results.json`);
if (label === 'after' && paths.every(path => existsSync(path))) {
  const reports = paths.map(path => JSON.parse(readFileSync(path, 'utf8')));
  if (reports.every(report => report.complete && !report.errors.length && report.runs.length === 6 && Object.keys(report.profiles).length === 2)) {
    /** @param {typeof results} report */
    const summarize = report => ({...report, runs: report.runs.map(({snapshot, userAgent, memory, ...metrics}) => metrics)});
    writeFileSync('artifacts/performance-results.json', JSON.stringify({before:summarize(reports[0]), after:summarize(reports[1])}, null, 2));
  }
}
