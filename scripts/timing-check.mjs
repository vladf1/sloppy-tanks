import { chromium } from "playwright";
import { writeFileSync, readFileSync } from "node:fs";
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: [
    "--window-size=2560,1440",
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
  ],
});
try {
  const context = await browser.newContext({
    viewport: { width: 2560, height: 1440 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage(),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  let navigations = 0;
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) navigations++;
  });
  await page.goto("http://127.0.0.1:5173/?autoplay", {
    waitUntil: "domcontentloaded",
    timeout: 60000,
  });
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    window.sloppy.exactResolution();
    window.sloppy.autoRounds();
    window.sloppy.record();
  });
  const cdp = await context.newCDPSession(page);
  const gpu = await page.evaluate(() => {
    const g = window.sloppy.view.renderer.getContext(),
      e = g.getExtension("WEBGL_debug_renderer_info");
    return e ? g.getParameter(e.UNMASKED_RENDERER_WEBGL) : "Unavailable";
  });
  const result = JSON.parse(
    readFileSync("artifacts/benchmark-results.json", "utf8"),
  );
  result.timingHistory ??= [];
  result.timingHistory.push({ normal: result.normal, stress: result.stress });
  result.previousTimingSamples = {
    normal: result.normal,
    stress: result.stress,
    conditions:
      "Collected while build and test work also ran; retained for comparison.",
  };
  result.gpu = gpu;
  result.timingRecheckStarted = new Date().toISOString();
  await page.evaluate(
    ({ seed, team }) => {
      const d = window.sloppy;
      d.sim.seed = seed;
      d.sim.humanTeam = team;
      d.start();
      d.autoplay();
      d.autoRounds();
      d.exactResolution();
      d.record();
    },
    {
      seed: result.normal.snapshot.seed,
      team: result.normal.snapshot.tanks[0].kind === "balanced" ? 0 : 1,
    },
  );
  result.errors.push(...errors);
  const save = () =>
    writeFileSync(
      "artifacts/benchmark-results.json",
      JSON.stringify(result, null, 2),
    );
  const metrics = async () => {
    const dom = await cdp.send("Memory.getDOMCounters");
    return await page.evaluate(
      (dom) => ({
        dom,
        geometry: window.sloppy.view.renderer.info.memory.geometries,
        textures: window.sloppy.view.renderer.info.memory.textures,
        bodies: window.sloppy.sim.world.bodies.len(),
        fragments: window.sloppy.sim.fragments.length,
        particles: window.sloppy.view.particles.length,
        heap: performance.memory?.usedJSHeapSize ?? null,
        round: window.sloppy.report().completedRounds,
        elapsed: window.sloppy.sim.elapsed,
      }),
      dom,
    );
  };
  for (let i = 0; i < 3; i++) {
    await page.waitForTimeout(30000);
    console.log("Normal sample", 30 * (i + 1), "seconds");
  }
  result.normal = await page.evaluate(() => window.sloppy.stop());
  if (
    result.normal.snapshot.elapsed < 80 &&
    result.normal.completedRounds === 0
  )
    throw new Error("Normal sample interrupted or paused; repeat it");
  await page.screenshot({ path: "artifacts/benchmark-normal.png" });
  save();
  console.log("NORMAL", JSON.stringify(result.normal));
  await page.evaluate(() => {
    window.sloppy.stress();
    window.sloppy.exactResolution();
    window.sloppy.overview();
    window.sloppy.record();
  });
  result.stressInitial = await page.evaluate(
    () => window.sloppy.sim.snapshot().counts,
  );
  for (let i = 0; i < 6; i++) {
    await page.evaluate(() => {
      const s = window.sloppy.sim;
      for (let i = s.fragments.length; i < s.maxFragments; i++)
        s.fragment(s.rng.range(-15, 15), s.rng.range(-15, 15), 0xc5a978, 0.5);
      for (let i = s.shots.length; i < 200; i++) {
        const a = s.rng.next() * Math.PI * 2;
        s.shots.push({
          id: s.nextId++,
          x: Math.sin(a) * 15,
          z: Math.cos(a) * 15,
          vx: Math.cos(a) * 45,
          vz: Math.sin(a) * 45,
          owner: s.tanks[i % 24].id,
          team: i % 2,
          damage: 40,
          bounces: 4,
          life: 4,
          weapon: "standard",
        });
      }
      for (const x of [-5, 0, 5]) {
        const c = s.addCover({
          kind: "drum",
          x,
          z: 0,
          w: 1.2,
          d: 1.2,
          h: 1.7,
          hp: 30,
          color: 0xe3854d,
        });
        if (x === 5) s.damageCover(c, 999, s.human.id, s.humanTeam);
      }
    });
    await page.waitForTimeout(10000);
    console.log("Stress cycle", i + 1);
  }
  result.stress = await page.evaluate(() => window.sloppy.stop());
  await page.screenshot({ path: "artifacts/benchmark-stress.png" });
  save();
  console.log("STRESS", JSON.stringify(result.stress));
  result.errors.push(...errors);
  result.timingRecheckFinished = new Date().toISOString();
  save();
  console.log("TIMING RECHECK COMPLETE");
} finally {
  await browser.close();
}
