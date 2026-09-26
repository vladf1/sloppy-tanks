// Destruction as drawn by the real renderer: timber damage stages and breach, rooted
// tree stumps with falling crowns, textured tower rubble, and debris that sinks and
// fades through its TSL shader. Damage, events, particles, navigation and physics are
// covered by tests/*.test.ts (cover-hit-effects, tree-damage, timber-walls,
// destruction-physics, debris-cleanup, game).
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { freezeLoop, gameUrl, launchGame, startRound } from "./browser-helpers.mjs";

const out = "artifacts/performance/destruction";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1440, height: 1000 },
  consoleErrors: true, // Shader compilation errors are only logged.
});
const results = {};
/** Draw one still frame of the game view; the frozen loop leaves HUD overlays stale. */
const draw = () =>
  page.evaluate(() => {
    const { sim, view } = window.sloppy;
    view.render(sim, 1, 0);
  });
try {
  await freezeLoop(page);
  await page.goto(gameUrl);
  await page.locator('input[name="mapMode"][value="village"]').check();
  await startRound(page);
  await page.evaluate(() => {
    document
      .querySelectorAll("#overlay, #hud, #fps, #loading")
      .forEach((element) => (element.style.display = "none"));
    window.sloppy.view.zoom = 20;
  });
  const moveTo = (x, z) =>
    page.evaluate(
      ({ x, z }) => {
        const tank = window.sloppy.sim.human;
        tank.body.setTranslation({ x, y: 0.65, z }, true);
        tank.previous = { x, z };
      },
      { x, z },
    );

  // Timber: each damage stage swaps the wall model; the breach hides only that bay.
  await moveTo(-2, 17);
  results.timber = await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    const wall = s.covers.find((c) => c.kind === "timber" && c.x === -2 && c.z === 13);
    const neighbor = s.covers.find((c) => c.kind === "timber" && c.x === 2 && c.z === 13);
    const stages = [];
    // 80 → 60 → 40 → 15 → 0 shows every damage stage before the breach.
    for (const damage of [20, 20, 25, 15]) {
      v.render(s, 1, 0);
      stages.push(v.coverMeshes.get(wall.id).userData.damageStage ?? 0);
      s.damageCover(wall, damage, s.human.id, s.humanTeam);
    }
    for (const e of s.events.splice(0)) v.event(e);
    v.render(s, 1, 0);
    return {
      stages,
      visible: v.coverMeshes.get(wall.id).visible,
      neighbor: v.coverMeshes.get(neighbor.id).visible,
    };
  });
  assert.deepEqual(results.timber, { stages: [0, 1, 2, 3], visible: false, neighbor: true });
  await page.screenshot({ path: `${out}/timber-breach.png` });

  // Trees: felling keeps the same model and stump meshes, hides the crown, and the
  // falling trunk and crown are visible debris.
  const firstTree = await page.evaluate(() => {
    const tree = window.sloppy.sim.covers.find((c) => c.kind === "tree");
    return { x: tree.x, z: tree.z };
  });
  await moveTo(firstTree.x, firstTree.z + 5);
  await draw();
  await page.screenshot({ path: `${out}/trees.png` });
  results.trees = await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    const trees = s.covers.filter((c) => c.kind === "tree");
    const models = trees.map((c) => v.coverMeshes.get(c.id));
    const stumps = models.map((g) => g.getObjectByName("rooted-stump"));
    const signature = (stump) => stump.children.map((m) => m.geometry.uuid).join();
    const signatures = stumps.map(signature);
    for (const c of trees) s.damageCover(c, 999, s.human.id, s.humanTeam);
    for (const e of s.events.splice(0)) v.event(e);
    v.render(s, 1, 0);
    const falling = s.fragments.filter((f) => f.treeCoverId !== undefined);
    return {
      trees: trees.length,
      sameModel: trees.every((c, i) => v.coverMeshes.get(c.id) === models[i]),
      sameStump: models.every(
        (g, i) =>
          g.getObjectByName("rooted-stump") === stumps[i] && signature(stumps[i]) === signatures[i],
      ),
      onlyStumps: models.every(
        (g) => g.visible && !g.userData.crown.visible && g.userData.cutSurface.visible,
      ),
      falling: falling.length,
      visibleFallingCrowns: falling.every((f) => {
        let meshes = 0;
        v.fragmentMeshes.get(f.id)?.traverse((object) => {
          if (object.isMesh && object.layers.mask !== 0) meshes++;
        });
        return meshes > 0;
      }),
    };
  });
  assert.ok(results.trees.trees > 0 && results.trees.falling > 0, JSON.stringify(results.trees));
  for (const key of ["sameModel", "sameStump", "onlyStumps", "visibleFallingCrowns"]) {
    assert.equal(results.trees[key], true, `trees: ${key}`);
  }
  await page.screenshot({ path: `${out}/stumps.png` });

  // Tower: the intact model disappears and two distinct, textured rubble piles take over.
  const tower = await page.evaluate(() => {
    const tower = window.sloppy.sim.covers.find((c) => c.kind === "tower");
    return { x: tower.x, z: tower.z };
  });
  await moveTo(tower.x, tower.z + 6);
  await page.waitForFunction(() => {
    const { sim: s, view: v } = window.sloppy;
    const tower = s.covers.find((c) => c.kind === "tower");
    return v.coverMeshes
      .get(tower.id)
      .children.every((m) => !m.material.map || m.material.map.image?.complete);
  });
  await draw();
  await page.screenshot({ path: `${out}/tower-intact.png` });
  results.tower = await page.evaluate(async () => {
    const { coverModel } = await import(new URL("src/game/models.ts", location.href).href);
    const { sim: s, view: v } = window.sloppy;
    const tower = s.covers.find((c) => c.kind === "tower");
    const existing = new Set(s.covers.filter((c) => c.kind === "rubble"));
    s.damageCover(tower, 999, s.human.id, s.humanTeam);
    v.render(s, 1, 0);
    const rubble = s.covers.filter((c) => c.kind === "rubble" && !existing.has(c));
    const layout = (c) =>
      JSON.stringify(
        coverModel(c)
          .children.slice(1)
          .map((m) => [
            m.geometry.uuid,
            m.position.toArray(),
            m.rotation.toArray(),
            m.material.color.getHex(),
          ]),
      );
    return {
      towerVisible: v.coverMeshes.get(tower.id).visible,
      piles: rubble.length,
      distinctPiles: layout(rubble[0]) !== layout(rubble[1]),
      stablePiles: rubble.every((c) => layout(c) === layout(c)),
      rubble: rubble.map((c) => ({
        visible: v.coverMeshes.get(c.id).visible,
        textured: v.coverMeshes.get(c.id).children.every((m) => !!m.material.map),
        colorMatches: c.color === tower.color,
        textures: v.coverMeshes.get(c.id).children.map((m) => m.material.map?.image?.src),
      })),
    };
  });
  assert.equal(results.tower.towerVisible, false);
  assert.equal(results.tower.piles, 2);
  assert.equal(results.tower.distinctPiles, true);
  assert.equal(results.tower.stablePiles, true);
  for (const pile of results.tower.rubble) {
    assert.ok(pile.visible && pile.textured && pile.colorMatches, JSON.stringify(pile));
    for (const name of ["weathered-concrete.webp", "siding.webp"])
      assert.ok(
        pile.textures.some((url) => url.endsWith(name)),
        name,
      );
    assert.ok(pile.textures.every((url) => !url.endsWith("shingles.webp")));
  }
  await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    const tower = s.covers.find((c) => c.kind === "tower");
    v.camera.position.set(tower.x + 11, 10, tower.z + 14);
    v.camera.lookAt(tower.x, 3, tower.z);
    v.renderer.render(v.scene, v.camera);
  });
  await page.screenshot({ path: `${out}/tower-rubble.png` });

  // Debris and wrecks sink and fade at full size as their life runs out.
  await page.evaluate(async () => {
    const { renderState } = await import(new URL("src/game/render-state.ts", location.href).href);
    window.sloppy.start(); // A fresh arena, then only the pieces under test.
    const { sim, view } = window.sloppy;
    for (const cover of sim.covers) sim.world.removeRigidBody(cover.body);
    for (const tank of sim.tanks) sim.world.removeRigidBody(tank.body);
    sim.covers = [];
    sim.movableCovers = [];
    sim.coverByCollider.clear();
    sim.tanks = [];
    sim.pickups = [];
    sim.nav.rebuild([]);
    for (const model of view.coverMeshes.values()) model.visible = false;
    for (const [kind, x] of [
      ["cargo", -6],
      ["timber", -2],
      ["tree", 2],
      ["drum", 6],
    ]) {
      const cover = sim.addCover({
        kind,
        x,
        z: -3,
        w: 2,
        h: kind === "tree" ? 6 : 2,
        d: 2,
        hp: 40,
        color: 0x98734f,
      });
      sim.damageCover(cover, 999, 999, 0);
    }
    const tank = sim.addTank(1, false, "balanced");
    tank.body.setTranslation({ x: 0, y: 0.65, z: 5 }, true);
    tank.protection = 0;
    sim.damageTank(tank, 999, 999, 0);
    sim.tanks = [];
    for (let i = 0; i < 210; i++) sim.step();
    sim.events.length = 0;
    // Arrange the fallen pieces in a readable patch, preserving their real resting poses.
    sim.fragments.forEach((piece, i) => {
      const p = piece.body.translation();
      piece.body.setTranslation(
        { x: ((i % 5) - 2) * 4, y: p.y, z: Math.floor(i / 5) * 4 - 4 },
        true,
      );
      piece.body.sleep();
    });
    const camera = view.camera;
    window.drawDebris = (life) => {
      for (const piece of sim.fragments) piece.life = life;
      view.updateFragments(renderState(sim));
      camera.position.set(14, 18, 24);
      camera.lookAt(0, 0, 0);
      camera.updateMatrixWorld();
      view.renderer.render(view.scene, camera);
      const matrices = [];
      for (const mesh of view.debrisMeshes.values()) {
        for (let i = 0; i < mesh.count; i++) {
          const matrix = mesh.matrix.clone();
          mesh.getMatrixAt(i, matrix);
          const position = mesh.position.clone(),
            rotation = mesh.quaternion.clone(),
            scale = mesh.scale.clone();
          matrix.decompose(position, rotation, scale);
          matrices.push({
            y: position.y,
            scale: scale.toArray(),
            opacity: mesh.geometry.getAttribute("debrisOpacity").getX(i),
          });
        }
      }
      return {
        matrices,
        wrecks: [...view.fragmentMeshes.values()].map((group) => ({
          y: group.position.y,
          scale: group.scale.toArray(),
        })),
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
  assert.ok(full.matrices.length >= 9 && full.wrecks.length >= 2, JSON.stringify(full));
  for (let i = 0; i < full.matrices.length; i++) {
    assert.deepEqual(full.matrices[i].scale, half.matrices[i].scale);
    assert.ok(half.matrices[i].y < full.matrices[i].y);
    assert.equal(half.matrices[i].opacity, 0.5);
    assert.equal(gone.matrices[i].opacity, 0);
  }
  for (let i = 0; i < full.wrecks.length; i++) {
    assert.deepEqual(full.wrecks[i].scale, half.wrecks[i].scale);
    assert.ok(half.wrecks[i].y < full.wrecks[i].y);
  }
  results.debris = { pieces: full.matrices.length, wreckParts: full.wrecks.length };
  assert.deepEqual(errors, []);
  writeFileSync(`${out}/results.json`, JSON.stringify({ ...results, errors }, null, 2));
  console.log(JSON.stringify({ ...results, errors }));
} finally {
  await browser.close();
}
