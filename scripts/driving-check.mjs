import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";

const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const out = "artifacts/performance/driving";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
const errors = [], checks = [];
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  page.on("pageerror", e => errors.push(e.message));
  // Real keyboard events and the application loop, with deterministic frame timing.
  await page.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = callback => { frame = callback; return 1; };
    window.advance = ticks => {
      for (let i = 0; i < ticks; i++) {
        now = (now ?? performance.now()) + 1000 / 60;
        frame(now);
      }
    };
  });
  await page.goto(url);
  await page.waitForFunction(() => !!window.sloppy);
  assert.match(await page.title(), /^Sloppy Tanks/);
  const advance = ticks => page.evaluate(ticks => window.advance(ticks), ticks);
  const pose = () => page.evaluate(() => {
    const t = window.sloppy.sim.human;
    return { heading: t.heading, position: { ...t.body.translation() },
      velocity: { ...t.body.linvel() }, aim: t.aim };
  });
  for (const [up, down, right] of [["w", "s", "d"], ["ArrowUp", "ArrowDown", "ArrowRight"]]) {
    await page.evaluate(() => {
      const d = window.sloppy; d.start(); d.autoplay(false);
      const s = d.sim, t = s.human;
      for (const c of s.covers) s.world.removeRigidBody(c.body);
      for (const other of s.tanks) if (other !== t) s.world.removeRigidBody(other.body);
      s.covers = []; s.coverByCollider.clear(); s.tanks = [t]; s.pickups = [];
      t.heading = Math.PI;
      t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
      t.body.setLinvel({ x: 0, y: 0, z: 0 }, true); t.previous = { x: 0, z: 0 };
      s.world.step(); d.view.reset(s); window.advance(2);
    });
    await page.mouse.move(1200, 350);
    await page.keyboard.down(up); await advance(30); await page.keyboard.up(up);
    const forward = await pose();
    assert.ok(forward.velocity.z < -8);
    await page.keyboard.down(down); await advance(30); await page.keyboard.up(down);
    const reverse = await pose();
    assert.ok(reverse.velocity.z > 6);
    assert.ok(Math.abs(reverse.heading - forward.heading) < 1e-8);
    await page.keyboard.down(up); await advance(30); await page.keyboard.up(up);
    await page.keyboard.down(right); await advance(12);
    const turning = await pose();
    assert.ok(turning.heading > Math.PI / 2 + 0.7 && turning.heading < Math.PI);
    assert.ok(Math.hypot(turning.velocity.x, turning.velocity.z) < 5);
    await page.screenshot({ path: `${out}/${up}-turning.png` });
    await advance(18); await page.keyboard.up(right);
    const aligned = await pose();
    assert.ok(Math.abs(aligned.heading - Math.PI / 2) < 1e-8);
    assert.ok(aligned.velocity.x > 8);
    await advance(12);
    const stopped = await pose();
    assert.ok(Math.hypot(stopped.velocity.x, stopped.velocity.z) < 0.01);
    await page.keyboard.press("Escape"); await advance(1);
    assert.equal(await page.evaluate(() => window.sloppy.sim.match.phase), "paused");
    checks.push({ keys: [up, down, right], forward, reverse, turning, aligned, stopped });
  }
  assert.deepEqual(errors, []);
  writeFileSync(`${out}/results.json`, JSON.stringify({ checks, errors }, null, 2));
  console.log(JSON.stringify({ checks, errors }));
} finally { await browser.close(); }
