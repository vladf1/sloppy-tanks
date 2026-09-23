import { chromium } from "playwright";
import { startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";

const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
    const requestFrame = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) =>
      callback.name === "loop" ? 1 : requestFrame(callback);
  });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/");
  await startRound(page);
  await page.evaluate(async () => {
    const { sim, view } = window.sloppy;
    const { renderState } = await import("/sloppy-tanks/src/game/render-state.ts");
    const THREE = await import("/sloppy-tanks/node_modules/three/build/three.module.js");
    window.sloppy.start();
    for (const cover of sim.covers) sim.world.removeRigidBody(cover.body);
    for (const tank of sim.tanks) sim.world.removeRigidBody(tank.body);
    sim.covers = [];
    sim.movableCovers = [];
    sim.coverByCollider.clear();
    sim.tanks = [];
    sim.pickups = [];
    sim.nav.rebuild([]);
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
    // Arrange already fallen pieces in a readable patch, preserving their real resting poses.
    sim.fragments.forEach((piece, i) => {
      const p = piece.body.translation();
      piece.body.setTranslation(
        { x: ((i % 5) - 2) * 4, y: p.y, z: Math.floor(i / 5) * 4 - 4 },
        true,
      );
      piece.body.sleep();
      piece.life = 2;
    });
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0xaac8cf);
    scene.add(new THREE.HemisphereLight(0xf4f5de, 0x516044, 2.3));
    const sun = new THREE.DirectionalLight(0xffebc5, 3);
    sun.position.set(-8, 20, 12);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    Object.assign(sun.shadow.camera, { left: -20, right: 20, top: 20, bottom: -20 });
    sun.shadow.camera.updateProjectionMatrix();
    scene.add(sun);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(80, 80),
      new THREE.MeshStandardMaterial({ color: 0xa6a181, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    scene.add(ground);
    const camera = new THREE.PerspectiveCamera(40, 1.44, 0.1, 100);
    camera.position.set(14, 18, 24);
    camera.lookAt(0, 0, 0);
    for (const mesh of view.debrisMeshes.values()) scene.add(mesh);
    document
      .querySelectorAll("#overlay, #hud, #fps, #loading")
      .forEach((el) => (el.style.display = "none"));
    window.debrisCheck = {
      scene,
      camera,
      THREE,
      draw(life) {
        for (const piece of sim.fragments) piece.life = life;
        view.updateFragments(renderState(sim));
        for (const group of view.fragmentMeshes.values()) scene.add(group);
        view.renderer.render(scene, camera);
        const matrices = [];
        for (const mesh of view.debrisMeshes.values()) {
          for (let i = 0; i < mesh.count; i++) {
            const matrix = new THREE.Matrix4();
            mesh.getMatrixAt(i, matrix);
            const position = new THREE.Vector3(),
              rotation = new THREE.Quaternion(),
              scale = new THREE.Vector3();
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
      },
    };
  });
  const full = await page.evaluate(() => window.debrisCheck.draw(1));
  await page.screenshot({ path: "artifacts/debris-full.png" });
  const half = await page.evaluate(() => window.debrisCheck.draw(0.5));
  await page.screenshot({ path: "artifacts/debris-sinking.png" });
  const gone = await page.evaluate(() => window.debrisCheck.draw(0));
  await page.screenshot({ path: "artifacts/debris-gone.png" });
  // Timber members fall as separate pieces, so instanced debris is cargo, splinters and drums.
  assert.ok(full.matrices.length >= 9 && full.wrecks.length >= 2);
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
  assert.deepEqual(errors, []);
  console.log(
    `Verified ${full.matrices.length} debris pieces and ${full.wrecks.length} wreck parts: full size, sink, fade, no browser/shader errors.`,
  );
} finally {
  await browser.close();
}
