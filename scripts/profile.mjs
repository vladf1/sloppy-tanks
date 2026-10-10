// @ts-check
import { chromium } from "playwright";
import { gameUrl, headless } from "./browser-helpers.mjs";
import { mkdirSync, writeFileSync } from "node:fs";

const label = process.argv[2] ?? "before";
if (!["before", "after"].includes(label))
  throw new Error("Use before or after as the measurement label.");
const width = Number(process.env.SLOPPY_WIDTH ?? 1280);
const height = Number(process.env.SLOPPY_HEIGHT ?? 720);
const out = `artifacts/performance/${label}`;
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome",
  headless,
  args: [
    `--window-size=${width},${height + 100}`,
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
  ],
});
const context = await browser.newContext({
  viewport: { width, height },
  deviceScaleFactor: 1,
});
const page = await context.newPage();
/** @type {string[]} */
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
let navigations = 0;
page.on("framenavigated", (frame) => {
  if (frame === page.mainFrame()) navigations++;
});
const cdp = await context.newCDPSession(page);
/** @type {Array<ReturnType<Window['sloppy']['report']> & {scenario: string, seed: number}>} */
const runs = [];
/** @type {Record<string, [string, number][]>} */
const profiles = {};
const results = {
  label,
  date: new Date().toISOString(),
  chrome: browser.version(),
  resolution: [width, height],
  seeds: [12345, 45678, 98765],
  warmupSeconds: 5,
  sampleSeconds: 15,
  errors,
  runs,
  profiles,
  complete: false,
};
const save = () => writeFileSync(`${out}/results.json`, JSON.stringify(results, null, 2));
const checkErrors = () => {
  if (errors.length) throw new Error(errors.join("\n"));
};
/** @param {string} scenario @param {number} seed */
async function setup(scenario, seed) {
  await page.goto(`${gameUrl}?autoplay`);
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(
    ({ scenario, seed }) => {
      const d = window.sloppy;
      d.game.debug_configure(seed, 12, 0);
      d.start();
      if (scenario === "stress") d.stress();
      d.exactResolution();
      d.overview(scenario === "stress");
      d.autoplay();
      if (scenario === "stress") {
        // Refill debris and shells and blow up a drum every five simulated seconds.
        let nextBurst = 5;
        const burst = () => {
          if (d.sim.elapsed >= nextBurst) {
            nextBurst += 5;
            d.game.debug_stress_burst();
          }
          requestAnimationFrame(burst);
        };
        requestAnimationFrame(burst);
      }
      d.record();
    },
    { scenario, seed },
  );
}
try {
  for (const scenario of ["normal", "stress"]) {
    for (const seed of results.seeds) {
      await setup(scenario, seed);
      const navigation = navigations;
      await page.waitForTimeout(20000);
      checkErrors();
      if (navigations !== navigation)
        throw new Error("Page reloaded during timing; discard this run and repeat.");
      const report = await page.evaluate(() => window.sloppy.stop());
      const elapsed = /** @type {{ elapsed: number }} */ (report.snapshot).elapsed;
      if (elapsed < 18) throw new Error("Measurement paused or could not keep up: " + elapsed);
      results.runs.push({ scenario, seed, ...report });
      save();
      console.log(
        scenario,
        seed,
        JSON.stringify({
          fps: report.fps,
          p95: report.frameP95,
          sim: report.simulationMean,
          render: report.renderMean,
          calls: report.drawCalls,
        }),
      );
    }
    await setup(scenario, 12345);
    await page.waitForTimeout(6000);
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 1000 });
    await cdp.send("Profiler.start");
    await page.waitForTimeout(10000);
    const { profile } = await cdp.send("Profiler.stop");
    checkErrors();
    if (!profile.samples || !profile.timeDeltas)
      throw new Error("CPU profile contains no samples.");
    writeFileSync(`${out}/${scenario}.cpuprofile`, JSON.stringify(profile));
    const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
    const self = new Map();
    for (let i = 0; i < profile.samples.length; i++) {
      const n = nodes.get(profile.samples[i]);
      if (!n) throw new Error("CPU profile references a missing node.");
      const key = `${n.callFrame.functionName || "(anonymous)"} @ ${n.callFrame.url}:${n.callFrame.lineNumber + 1}`;
      self.set(key, (self.get(key) || 0) + profile.timeDeltas[i] / 1000);
    }
    results.profiles[scenario] = [...self].sort((a, b) => b[1] - a[1]).slice(0, 35);
    await page.screenshot({ path: `${out}/${scenario}.png` });
    save();
    console.log("PROFILE", scenario, JSON.stringify(results.profiles[scenario].slice(0, 15)));
  }
  results.complete = true;
} finally {
  save();
  await browser.close();
}
