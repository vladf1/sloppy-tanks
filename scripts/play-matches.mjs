import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: ["--window-size=1600,1000"],
});
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:5173/", {
    waitUntil: "domcontentloaded",
    timeout: 60000,
  });
  await page.waitForFunction(() => !!window.sloppy, {}, { timeout: 60000 });
  await page.evaluate(() => {
    window.playEvents = [];
    const original = window.sloppy.view.event.bind(window.sloppy.view);
    window.sloppy.view.event = (e) => {
      if (e.id === window.sloppy.sim.human.id)
        window.playEvents.push({ type: e.type, label: e.label });
      original(e);
    };
  });
  const results = [];
  const held = new Set();
  for (const kind of ["scout", "balanced", "heavy"]) {
    await page.locator(`[data-kind="${kind}"]`).click();
    await page.evaluate(() => (window.playEvents = []));
    let iterations = 0,
      shots = 0,
      pickups = 0,
      mines = 0,
      previousWeapon = "standard";
    const begin = Date.now();
    while (iterations < 4000) {
      const state = await page.evaluate(() => {
        const d = window.sloppy,
          s = d.sim,
          t = s.human;
        if (s.match.phase !== "playing") return { phase: s.match.phase };
        if (!t.alive) return { phase: "dead" };
        const p = t.body.translation(),
          enemies = s.tanks
            .filter((e) => e.alive && e.team !== t.team)
            .sort(
              (a, b) =>
                Math.hypot(
                  a.body.translation().x - p.x,
                  a.body.translation().z - p.z,
                ) -
                Math.hypot(
                  b.body.translation().x - p.x,
                  b.body.translation().z - p.z,
                ),
            );
        const q = enemies[0]?.body.translation() ?? { x: 0, y: 0, z: 0 };
        const useful = s.pickups
          .filter((q) => q.available)
          .sort(
            (a, b) =>
              Math.hypot(a.x - p.x, a.z - p.z) -
              Math.hypot(b.x - p.x, b.z - p.z),
          );
        const goal =
          useful[0] && Math.hypot(useful[0].x - p.x, useful[0].z - p.z) < 12
            ? useful[0]
            : q;
        const path = s.nav.find(p, goal),
          next =
            path.find((n) => Math.hypot(n.x - p.x, n.z - p.z) > 1.5) ?? goal;
        const aim = d.view.follow
          .clone()
          .set(q.x, 1, q.z)
          .project(d.view.camera);
        return {
          phase: "playing",
          dx: next.x - p.x,
          dz: next.z - p.z,
          x: (aim.x + 1) * 800,
          y: (1 - aim.y) * 450,
          fire: s.visible(p, q),
          weapon: t.weapon,
          mine: t.mineCooldown === 0,
        };
      });
      if (state.phase === "results") break;
      const desired = new Set();
      if (state.phase === "playing") {
        if (Math.abs(state.dx) > 0.7) desired.add(state.dx > 0 ? "d" : "a");
        if (Math.abs(state.dz) > 0.7) desired.add(state.dz > 0 ? "s" : "w");
        await page.mouse.move(
          Math.max(2, Math.min(1598, state.x)),
          Math.max(2, Math.min(898, state.y)),
        );
        if (state.fire) {
          await page.mouse.down();
          shots++;
        } else await page.mouse.up();
        if (state.mine && iterations % 20 === 0) {
          await page.mouse.click(800, 450, { button: "right" });
          mines++;
        }
        if (state.weapon !== previousWeapon && state.weapon !== "standard")
          pickups++;
        previousWeapon = state.weapon;
      } else {
        await page.mouse.up();
        if (state.phase === "paused") await page.locator("#resume").click();
      }
      for (const key of held)
        if (!desired.has(key)) {
          await page.keyboard.up(key);
          held.delete(key);
        }
      for (const key of desired)
        if (!held.has(key)) {
          await page.keyboard.down(key);
          held.add(key);
        }
      await page.waitForTimeout(160);
      iterations++;
      if (iterations % 180 === 0)
        console.log(
          "Playing",
          kind,
          Math.round((Date.now() - begin) / 1000),
          "seconds",
        );
    }
    await page.mouse.up();
    for (const key of held) await page.keyboard.up(key);
    held.clear();
    const result = await page.evaluate(() => ({
      audio: window.Howler
        ? {
            state: window.Howler.state,
            sounds: window.Howler._howls.map((h) => h._sounds.length),
            queues: window.Howler._howls.map((h) => h._queue.length),
          }
        : null,
      match: window.sloppy.sim.match,
      human: {
        kills: window.sloppy.sim.human.kills,
        deaths: window.sloppy.sim.human.deaths,
        kind: window.sloppy.sim.human.kind,
      },
      snapshot: window.sloppy.sim.snapshot(),
      events: window.playEvents,
    }));
    if (result.match.phase !== "results")
      throw new Error("Round did not finish");
    results.push({
      kind,
      wallSeconds: (Date.now() - begin) / 1000,
      aimAndFireCommands: shots,
      observedWeaponPickups: pickups,
      mineClicks: mines,
      ...result,
    });
    await page.screenshot({ path: `artifacts/match-${kind}.png` });
    writeFileSync(
      "artifacts/play-results.json",
      JSON.stringify(
        {
          method:
            "Real Playwright keyboard and mouse input; read-only game-state navigation assistance, no direct damage or teleportation",
          results,
          errors,
        },
        null,
        2,
      ),
    );
    console.log("MATCH COMPLETE", kind, JSON.stringify(result.match));
    await page.locator("#restart").click();
  }
} finally {
  await browser.close();
}
