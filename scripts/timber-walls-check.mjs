import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    window.requestAnimationFrame = () => 1;
  });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5174/sloppy-tanks/");
  await page.waitForFunction(() => !!window.sloppy);
  const result = await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    s.mapMode = "village";
    s.reset();
    v.reset(s);
    window.sloppy.start();
    s.human.body.setTranslation({ x: -2, y: 0.65, z: 17 }, true);
    s.human.previous = { x: -2, z: 17 };
    v.zoom = 20;
    const wall = s.covers.find((c) => c.kind === "timber" && c.x === -2 && c.z === 13);
    const neighbor = s.covers.find((c) => c.kind === "timber" && c.x === 2 && c.z === 13);

    const stages = [];
    for (let i = 0; i < 3; i++) {
      v.render(s, 1, 0);
      stages.push(v.coverMeshes.get(wall.id).userData.damageStage ?? 0);
      s.damageCover(wall, 40, s.human.id, s.humanTeam);
    }
    for (const e of s.events) v.event(e);
    v.render(s, 1, 0);
    return {
      stages,
      visible: v.coverMeshes.get(wall.id).visible,
      neighbor: v.coverMeshes.get(neighbor.id).visible,
    };
  });
  assert.deepEqual(result, { stages: [0, 1, 2], visible: false, neighbor: true });
  await page.screenshot({ path: "artifacts/timber-breach.png" });
  await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    s.reset();
    v.reset(s);
    window.sloppy.start();
    s.human.body.setTranslation({ x: -2, y: 0.65, z: 17 }, true);
    s.human.previous = { x: -2, z: 17 };
    v.zoom = 20;
    v.render(s, 1, 0);
  });
  await page.screenshot({ path: "artifacts/timber-intact.png" });
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ ...result, errors }));
} finally {
  await browser.close();
}
