import { chromium } from "playwright";
import assert from "node:assert/strict";

const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.addInitScript(() => { window.requestAnimationFrame = () => 1; });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5174/sloppy-tanks/");
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    s.mapMode = "classic"; s.reset(); v.reset(s); window.sloppy.start();
    // The paused animation loop does not run the HUD's normal overlay update.
    document.querySelector("#overlay").style.display = "none";
    const tower = s.covers.find(c => c.kind === "tower");
    s.human.body.setTranslation({ x: tower.x, y: 0.65, z: tower.z + 6 }, true);
    s.human.previous = { x: tower.x, z: tower.z + 6 };
    v.zoom = 23;
  });
  await page.waitForFunction(() => {
    const { sim: s, view: v } = window.sloppy;
    const tower = s.covers.find(c => c.kind === "tower");
    return v.coverMeshes.get(tower.id).children.every(m =>
      !m.material.map || m.material.map.image?.complete);
  });
  for (const destroyed of [false, true]) {
    const result = await page.evaluate(async destroyed => {
      const { coverModel } = await import("/sloppy-tanks/src/game/models.ts");
      const { sim: s, view: v } = window.sloppy;
      const tower = s.covers.find(c => c.kind === "tower");
      if (destroyed) s.damageCover(tower, 999, s.human.id, s.humanTeam);
      v.render(s, 1, 0);
      const rubble = s.covers.filter(c => c.kind === "rubble");
      const layout = c => JSON.stringify(coverModel(c).children.slice(1).map(m => [
        m.geometry.uuid, m.position.toArray(), m.rotation.toArray(), m.material.color.getHex(),
      ]));
      return {
        towerVisible: v.coverMeshes.get(tower.id).visible,
        centerOpen: !s.nav.blocked[s.nav.index(tower)],
        woodFragments: s.fragments.filter(f => f.shape === "wood").length,
        distinctPiles: rubble.length === 2 && layout(rubble[0]) !== layout(rubble[1]),
        stablePiles: rubble.every(c => layout(c) === layout(c)),
        rubble: rubble.map(c => ({
          textured: v.coverMeshes.get(c.id).children.every(m => !!m.material.map),
          colorMatches: c.color === tower.color,
          textures: v.coverMeshes.get(c.id).children.map(m => m.material.map?.image?.src),
        })),
      };
    }, destroyed);
    const state = destroyed ? "destroyed" : "intact";
    await page.screenshot({ path: `artifacts/tower-${state}-game.png` });
    await page.evaluate(() => {
      const { sim: s, view: v } = window.sloppy;
      const tower = s.covers.find(c => c.kind === "tower");
      v.camera.position.set(tower.x + 11, 10, tower.z + 14);
      v.camera.lookAt(tower.x, 3, tower.z);
      v.renderer.render(v.scene, v.camera);
    });
    await page.screenshot({ path: `artifacts/tower-${state}-detail.png` });
    assert.equal(result.towerVisible, !destroyed);
    if (destroyed) {
      assert.equal(result.centerOpen, true);
      assert.equal(result.rubble.length, 2);
      assert.equal(result.woodFragments, 10);
      assert.equal(result.distinctPiles, true);
      assert.equal(result.stablePiles, true);
      assert.ok(result.rubble.every(c => c.textured && c.colorMatches));
      for (const c of result.rubble) {
        for (const name of ["weathered-concrete.webp", "siding.png"])
          assert.ok(c.textures.some(url => url.endsWith(name)));
        assert.ok(c.textures.every(url => !url.endsWith("shingles.png")));
      }
    }
    console.log(state, JSON.stringify(result));
  }
  assert.deepEqual(errors, []);
} finally { await browser.close(); }
