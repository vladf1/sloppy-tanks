// Destruction as drawn by the real renderer: timber damage stages and breach, rooted
// tree stumps with falling crowns, tower rubble, and debris that sinks and fades.
// Damage, events, particles, navigation, physics and rubble materials are covered by
// the engine's tests (cover_hit_effects, tree damage, timber_walls,
// destruction_physics, debris_cleanup, tower_rubble_wears_concrete_and_timber); this
// check reads what each view shows (`Game.debug_view_json`) and keeps screenshots of
// every stage.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import {
  chooseMap,
  freezeLoop,
  gameUrl,
  launchGame,
  seedGame,
  startRound,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/destruction";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1440, height: 1000 },
  consoleErrors: true, // Shader compilation errors are only logged.
});
const results = {};
/** Draw one still frame of the game view; the frozen loop leaves HUD overlays stale. */
const draw = (camera = []) => page.evaluate((camera) => window.engine.draw(camera), camera);
try {
  await freezeLoop(page);
  // The seed keeps the first round's breach and tree debris deterministic; the wreck
  // section reseeds before it adds its tank.
  await seedGame(page, 12345);
  await page.goto(gameUrl);
  await chooseMap(page, "village");
  await startRound(page);
  await page.evaluate(() => {
    document
      .querySelectorAll("#overlay, #hud")
      .forEach((element) => (element.style.display = "none"));
    window.sloppy.autoplay(false);
    window.sloppy.zoom(20);
  });
  const moveTo = (x, z) =>
    page.evaluate(
      ({ x, z }) => {
        const { sloppy } = window;
        sloppy.game.debug_place_tank(sloppy.sim.human.id, x, z, NaN);
        // Settle the camera on the new position.
        for (let i = 0; i < 60; i++) sloppy.game.debug_render(1, 1 / 60, false, new Float32Array());
      },
      { x, z },
    );

  // Timber: each damage stage swaps the wall model; the breach hides only that bay.
  await moveTo(-2, 17);
  results.timber = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const covers = engine.covers();
    const wall = covers.find((c) => c.kind === "timber" && c.x === -2 && c.z === 13);
    const neighbor = covers.find((c) => c.kind === "timber" && c.x === 2 && c.z === 13);
    const view = (id) => engine.view().covers.find((cover) => cover.id === id);
    const stages = [];
    // 80 → 60 → 40 → 15 → 0 shows every damage stage before the breach.
    for (const damage of [20, 20, 25, 15]) {
      engine.draw();
      stages.push(view(wall.id).stage);
      sloppy.game.debug_damage_cover(wall.id, damage);
    }
    engine.draw();
    return { stages, visible: view(wall.id).shown, neighbor: view(neighbor.id).shown };
  });
  assert.deepEqual(results.timber, { stages: [0, 1, 2, 3], visible: false, neighbor: true });
  // Each stage replaced the wall's model; the drawn frame freed the superseded meshes
  // instead of keeping them until the round resets.
  results.unusedMeshes = await page.evaluate(() => window.sloppy.stats().unusedMeshes);
  assert.equal(results.unusedMeshes, 0, "superseded cover meshes are freed");

  // The breached bay's loose members scar where a shell strikes them, like the wall did.
  results.debrisScar = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const game = sloppy.game;
    for (let i = 0; i < 120; i++) {
      game.debug_step(1, 0, 0);
      game.debug_render(1, 1 / 60, false, new Float32Array());
    }
    const pieces = () => engine.view().fragments.filter((f) => Number.isInteger(f.timberMarks));
    // The tallest piece stands high enough for a shell at combat height to strike it.
    const target = pieces().sort((a, b) => b.position[1] - a.position[1])[0];
    const [x, , z] = target.position;
    game.debug_add_shot(
      JSON.stringify({
        x,
        z: z + 3,
        vx: 0,
        vz: -40,
        team: 1,
        owner: 9999,
        weapon: "standard",
        damage: 10,
        life: 1,
      }),
    );
    for (let i = 0; i < 5; i++) {
      game.debug_step(1, 0, 0);
      game.debug_render(1, 1 / 60, false, new Float32Array());
    }
    const after = pieces().find((f) => f.id === target.id);
    return { pieces: pieces().length, before: target.timberMarks, after: after?.timberMarks };
  });
  assert.ok(results.debrisScar.pieces > 0, JSON.stringify(results.debrisScar));
  assert.equal(results.debrisScar.after, results.debrisScar.before + 1, "a loose member scars");
  await page.screenshot({ path: `${out}/timber-breach.png` });

  // Trees: felling keeps the same model with its stump, hides the crown, and the
  // falling crowns are visible debris.
  const firstTree = await page.evaluate(() =>
    window.engine.covers().find((c) => c.kind === "tree"),
  );
  await moveTo(firstTree.x, firstTree.z + 5);
  await draw();
  await page.screenshot({ path: `${out}/trees.png` });
  results.trees = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const trees = engine.covers().filter((c) => c.kind === "tree");
    const ids = new Set(trees.map((tree) => tree.id));
    const views = () => engine.view().covers.filter((cover) => ids.has(cover.id));
    const before = views();
    const owned = new Set(
      engine
        .view()
        .fragments.filter((f) => f.look === "owned")
        .map((f) => f.id),
    );
    for (const tree of trees) sloppy.game.debug_damage_cover(tree.id, 999);
    engine.draw();
    const after = views();
    const falling = engine.view().fragments.filter((f) => f.look === "owned" && !owned.has(f.id));
    return {
      trees: trees.length,
      sameModel: after.every((view, i) => view.modelKey === before[i].modelKey),
      onlyStumps: after.every((view) => view.shown && view.crown === false && view.cut === true),
      falling: falling.length,
      visibleFallingCrowns: falling.every((f) => f.shown && f.opacity > 0),
    };
  });
  assert.ok(results.trees.trees > 0 && results.trees.falling > 0, JSON.stringify(results.trees));
  for (const key of ["sameModel", "onlyStumps", "visibleFallingCrowns"]) {
    assert.equal(results.trees[key], true, `trees: ${key}`);
  }
  await page.screenshot({ path: `${out}/stumps.png` });

  // Tower: the intact model disappears and two distinct rubble piles take over.
  const tower = await page.evaluate(() => window.engine.covers().find((c) => c.kind === "tower"));
  await moveTo(tower.x, tower.z + 6);
  await draw();
  await page.screenshot({ path: `${out}/tower-intact.png` });
  results.tower = await page.evaluate((tower) => {
    const { sloppy, engine } = window;
    const existing = new Set(
      engine
        .covers()
        .filter((c) => c.kind === "rubble")
        .map((c) => c.id),
    );
    sloppy.game.debug_damage_cover(tower.id, 999);
    engine.draw();
    const views = new Map(engine.view().covers.map((view) => [view.id, view]));
    const rubble = engine.covers().filter((c) => c.kind === "rubble" && !existing.has(c.id));
    return {
      towerVisible: views.get(tower.id).shown,
      piles: rubble.length,
      distinctPiles: new Set(rubble.map((c) => views.get(c.id).modelKey)).size === rubble.length,
      rubble: rubble.map((c) => ({
        visible: views.get(c.id).shown,
        colorMatches: c.color === tower.color,
      })),
    };
  }, tower);
  assert.equal(results.tower.towerVisible, false);
  assert.equal(results.tower.piles, 2);
  assert.equal(results.tower.distinctPiles, true);
  for (const pile of results.tower.rubble) {
    assert.ok(pile.visible && pile.colorMatches, JSON.stringify(pile));
  }
  await draw([tower.x + 11, 10, tower.z + 14, tower.x, 3, tower.z]);
  await page.screenshot({ path: `${out}/tower-rubble.png` });

  // Debris and wrecks sink and fade at full size as their life runs out.
  await page.evaluate(() => {
    const { sloppy, engine } = window;
    const game = sloppy.game;
    // Wreck trajectories and debris-budget pruning use the seeded stream.
    game.debug_configure(4242, 12, 0);
    sloppy.start(); // A fresh arena, then only the pieces under test.
    game.debug_clear_arena(new Uint32Array());
    game.debug_place_tank(sloppy.sim.human.id, 0, 40, NaN);
    for (const [kind, x] of [
      ["cargo", -6],
      ["timber", -2],
      ["tree", 2],
      ["drum", 6],
    ]) {
      const id = game.debug_add_cover(
        JSON.stringify({
          kind,
          x,
          z: -3,
          w: 2,
          h: kind === "tree" ? 6 : 2,
          d: 2,
          hp: 40,
          color: 0x98734f,
        }),
      );
      game.debug_damage_cover(id, 999);
    }
    const tank = game.debug_add_tank(1, "balanced");
    game.debug_place_tank(tank, 0, 5, NaN);
    engine.setTank(tank, { protection: 0, shield: 0 });
    game.debug_damage_tank(tank, 999, 999999, 0);
    game.debug_step(210, 0, 0);
    engine.draw();
    window.drawDebris = (life) => {
      game.debug_set_fragment_life(life);
      engine.draw([14, 18, 24, 0, 0, 0]);
      const fragments = engine.view().fragments;
      return {
        pieces: fragments.filter((f) => f.look === "piece" && f.shown),
        wrecks: fragments.filter((f) => f.look === "wreck"),
      };
    };
  });
  const full = await page.evaluate(() => window.drawDebris(1));
  await page.screenshot({ path: `${out}/debris-full.png` });
  const half = await page.evaluate(() => window.drawDebris(0.5));
  await page.screenshot({ path: `${out}/debris-sinking.png` });
  const gone = await page.evaluate(() => window.drawDebris(0));
  await page.screenshot({ path: `${out}/debris-gone.png` });
  // Timber members fall as separate pieces, so instanced debris is cargo, splinters and drums.
  assert.ok(
    full.pieces.length >= 9 && full.wrecks.length >= 2,
    JSON.stringify({ pieces: full.pieces.length, wrecks: full.wrecks.length }),
  );
  const close = (a, b) => a.every((value, i) => Math.abs(value - b[i]) < 1e-5);
  for (let i = 0; i < full.pieces.length; i++) {
    assert.ok(close(full.pieces[i].scale, half.pieces[i].scale), "pieces keep their size");
    assert.ok(half.pieces[i].position[1] < full.pieces[i].position[1], "pieces sink");
    assert.ok(Math.abs(half.pieces[i].opacity - 0.5) < 1e-6, "pieces fade");
    assert.equal(gone.pieces[i].opacity, 0);
  }
  for (let i = 0; i < full.wrecks.length; i++) {
    assert.ok(close(full.wrecks[i].scale, half.wrecks[i].scale), "wrecks keep their size");
    assert.ok(half.wrecks[i].position[1] < full.wrecks[i].position[1], "wrecks sink");
  }
  results.debris = { pieces: full.pieces.length, wreckParts: full.wrecks.length };
  assert.equal(await page.evaluate(() => window.sloppy.error()), null, "no GPU error");
  assert.deepEqual(errors, []);
  writeFileSync(`${out}/results.json`, JSON.stringify({ ...results, errors }, null, 2));
  console.log(JSON.stringify({ ...results, errors }));
} finally {
  await browser.close();
}
