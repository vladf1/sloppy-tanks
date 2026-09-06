import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
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
  await page.goto("http://127.0.0.1:5173/?autoplay");
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    window.sloppy.exactResolution();
    window.sloppy.autoRounds();
    window.sloppy.record();
  });
  const cdp = await context.newCDPSession(page);
  const result = {
    hardware: "Apple M3 Max, 16 CPU cores, 40 GPU cores, 48 GB RAM",
    chrome: browser.version(),
    started: new Date().toISOString(),
    errors,
    normal: null,
    stress: null,
    longevity: null,
    resets: [],
    checkpoints: [],
  };
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
  // Ten rendered resets with collection expose GPU, DOM, listener and live-body growth.
  for (let i = 0; i < 10; i++) {
    await page.evaluate(() => {
      const d = window.sloppy;
      d.sim.roundCount = 12;
      d.start();
      d.autoplay();
      d.overview(false);
      d.exactResolution();
    });
    await page.waitForTimeout(300);
    await cdp.send("HeapProfiler.collectGarbage");
    result.resets.push(await metrics());
  }
  save();
  console.log("RESETS", JSON.stringify(result.resets));
  await page.evaluate(() => {
    window.sloppy.autoRounds();
    window.sloppy.record();
  });
  await page.evaluate(() => {
    window.soakActiveSeconds = 0;
    const sim = window.sloppy.sim,
      step = sim.step.bind(sim);
    sim.step = (...args) => {
      const before = sim.elapsed;
      step(...args);
      window.soakActiveSeconds += Math.max(0, sim.elapsed - before);
    };
  });
  const soakStart = Date.now();
  let activeSeconds = 0,
    i = 0;
  while (activeSeconds < 1200) {
    await page.waitForTimeout(30000);
    activeSeconds = await page.evaluate(() => {
      if (window.sloppy.sim.match.phase === "paused") window.sloppy.sim.start();
      return window.soakActiveSeconds;
    });
    if (!Number.isFinite(activeSeconds))
      throw new Error("Longevity counter lost: unexpected page reload");
    if (i++ % 2 === 1) {
      await cdp.send("HeapProfiler.collectGarbage");
      const point = {
        wallSeconds: (Date.now() - soakStart) / 1000,
        activeSeconds,
        ...(await metrics()),
      };
      result.checkpoints.push(point);
      save();
      console.log("LONGEVITY", JSON.stringify(point));
    } else
      console.log(
        "Longevity",
        Math.round(activeSeconds),
        "active simulation seconds",
      );
  }
  result.longevity = {
    wallSeconds: (Date.now() - soakStart) / 1000,
    activeSeconds,
    ...(await page.evaluate(() => window.sloppy.stop())),
  };
  if (activeSeconds < 1200 || navigations !== 1)
    throw new Error(
      `Incomplete longevity: ${activeSeconds}s, ${navigations} navigations`,
    );
  result.navigations = navigations;
  result.finished = new Date().toISOString();
  save();
  console.log("COMPLETE", JSON.stringify(result.longevity));
} finally {
  await browser.close();
}
