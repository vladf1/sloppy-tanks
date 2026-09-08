import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage();
  const errors = []; page.on("pageerror", e => errors.push(e.message));
  await page.addInitScript(() => { window.requestAnimationFrame = () => 1; });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5174/sloppy-tanks/");
  await page.waitForFunction(() => !!window.sloppy);
  const results = await page.evaluate(async () => {
    const { stepProjectiles } = await import("/sloppy-tanks/src/game/weapons.ts");
    const { sim: s, view: v } = window.sloppy;
    s.mapMode = "classic"; window.sloppy.start();
    const covers = [s.covers.find(c => c.kind === "tree"), s.covers.find(c => c.kind === "timber"),
      s.addCover({ kind: "fence", x: 0, z: 30, w: 4, d: 0.9, h: 1.5, hp: 80, color: 0xb47a49 })];
    s.world.step();
    return covers.map(c => {
      function shoot(damage) {
        s.shots.length = 0; s.events.length = 0; v.particles.length = 0;
        s.shots.push({ id: s.nextId++, x: c.x, z: c.z - c.d / 2 - 0.5, vx: 0, vz: 40,
          owner: s.human.id, team: s.humanTeam, damage, bounces: 0, life: 1, piercing: 0, weapon: "standard" });
        stepProjectiles(s, 0.05);
        for (const e of s.events) v.event(e);
        return { alive: c.alive, hp: c.hp,
          impacts: s.events.filter(e => e.type === "impact" && e.coverKind === c.kind).length,
          destroys: s.events.filter(e => e.type === "destroy").length,
          leaves: v.particles.filter(p => p.shape === "leaf").length,
          chips: v.particles.filter(p => p.shape === "splinter").length };
      }
      const hp = c.hp, hit = shoot(20), destroyed = shoot(999);
      return { kind: c.kind, hp, hit, destroyed };
    });
  });
  console.log(JSON.stringify(results));
  for (const { kind, hp, hit, destroyed } of results) {
    assert.equal(hit.alive, true); assert.equal(hit.hp, hp - 20);
    assert.equal(hit.impacts, 1); assert.equal(hit.destroys, 0);
    assert.ok(hit.chips > 0); assert.equal(hit.leaves > 0, kind === "tree");
    assert.equal(destroyed.alive, false); assert.equal(destroyed.destroys, 1);
    assert.equal(destroyed.impacts, 0, "fatal impacts must not double the destruction burst");
    assert.ok(destroyed.leaves + destroyed.chips > hit.leaves + hit.chips);
  }
  assert.deepEqual(errors, []); console.log(JSON.stringify({ results, errors }));
} finally { await browser.close(); }
