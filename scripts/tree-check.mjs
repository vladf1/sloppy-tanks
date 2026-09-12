import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() => {
    window.requestAnimationFrame = () => 1;
  });
  await page.goto(process.env.SLOPPY_URL ?? "http://127.0.0.1:5174/sloppy-tanks/");
  await page.waitForFunction(() => !!window.sloppy);
  const result = await page.evaluate(async () => {
    const { treeModel, setTreeDestroyed, TREE_FAMILIES } =
      await import("/sloppy-tanks/src/game/tree-models.ts");
    const THREE = await import("/sloppy-tanks/node_modules/three/build/three.module.js");
    const { sim: s, view: v } = window.sloppy;
    s.mapMode = "village";
    window.sloppy.start();
    document.querySelector("#overlay").style.display = "none";
    const trees = s.covers.filter((c) => c.kind === "tree");
    const first = trees[0];
    s.human.body.setTranslation({ x: first.x, y: 0.65, z: first.z + 5 }, true);
    s.human.previous = { x: first.x, z: first.z + 5 };
    v.zoom = 23;
    const before = trees.map((c) => v.coverMeshes.get(c.id));
    const stumps = before.map((g) => g.getObjectByName("rooted-stump"));
    const signatures = stumps.map((g) => g.children.map((m) => m.geometry.uuid).join());
    v.render(s, 1, 0);
    const all = new Map();
    for (let i = 0; i < 160 && all.size < 6; i++) {
      const c = { x: i * 3.1, z: 27, w: 2.6, d: 2.6, h: 6 };
      const tree = treeModel(c);
      if (!all.has(tree.userData.family)) all.set(tree.userData.family, c);
    }
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0xaac8cf);
    scene.add(new THREE.HemisphereLight(0xf4f5de, 0x516044, 2.3));
    const sun = new THREE.DirectionalLight(0xffebc5, 3);
    sun.position.set(-8, 20, 12);
    scene.add(sun);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(42, 24),
      new THREE.MeshStandardMaterial({ color: 0x9da77c, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -0.015;
    scene.add(ground);
    const specimens = [];
    for (const [i, family] of TREE_FAMILIES.entries()) {
      const c = all.get(family);
      const tree = treeModel(c),
        stump = treeModel(c);
      setTreeDestroyed(stump, true);
      tree.position.set((i - 2.5) * 5.3, 0, -2.2);
      stump.position.set((i - 2.5) * 5.3, 0, 3.7);
      scene.add(tree, stump);
      specimens.push(tree, stump);
      const label = document.createElement("div");
      label.textContent = family;
      label.style.cssText = `position:fixed;left:${((i + 0.5) / 6) * 100}%;bottom:105px;transform:translateX(-50%);font:600 20px system-ui;color:#172c26;pointer-events:none`;
      label.className = "tree-label";
      document.body.append(label);
    }
    const camera = new THREE.PerspectiveCamera(40, 1.6, 0.1, 100);
    camera.position.set(0, 15, 28);
    camera.lookAt(0, 2.1, 0);
    window.treeCheck = { scene, camera, specimens, trees, before, stumps, signatures };
    return { families: [...all.keys()] };
  });
  assert.equal(result.families.length, 6);
  await page.waitForFunction(() => {
    let ready = true;
    window.treeCheck.scene.traverse((m) => {
      if (m.material?.map) ready &&= !!m.material.map.image?.complete;
    });
    return ready;
  });
  await page.evaluate(() => {
    document.querySelectorAll("#hud, #fps").forEach((el) => (el.style.display = "none"));
    const { scene, camera } = window.treeCheck;
    window.sloppy.view.renderer.render(scene, camera);
  });
  await page.screenshot({ path: "artifacts/tree-families.png" });
  await page.evaluate(() => {
    document.querySelectorAll(".tree-label").forEach((el) => el.remove());
    const { sim: s, view: v } = window.sloppy;
    v.render(s, 1, 0);
  });
  await page.screenshot({ path: "artifacts/trees-game.png" });
  const destroyed = await page.evaluate(() => {
    const { sim: s, view: v } = window.sloppy;
    const { trees, before, stumps, signatures } = window.treeCheck;
    const fragments = s.fragments.length,
      particles = v.particles.length;
    s.events.length = 0;
    for (const c of trees) s.damageCover(c, 999, s.human.id, s.humanTeam);
    for (const e of s.events) v.event(e);
    v.render(s, 1, 0);
    return {
      sameTree: trees.every((c, i) => v.coverMeshes.get(c.id) === before[i]),
      sameStump: before.every(
        (g, i) =>
          g.getObjectByName("rooted-stump") === stumps[i] &&
          stumps[i].children.map((m) => m.geometry.uuid).join() === signatures[i],
      ),
      onlyStumps: before.every(
        (g) => g.visible && !g.userData.crown.visible && g.userData.cutSurface.visible,
      ),
      hasBurst:
        s.fragments.length > fragments &&
        v.particles.length > particles &&
        v.particles.some((p) => p.shape === "leaf") &&
        v.particles.some((p) => p.shape === "splinter"),
      longerDebris:
        s.fragments.every((f) => f.shape === "wood" && f.life >= 4.8 && f.life <= 7.8) &&
        v.particles.every((p) => p.life >= 2.55 && p.life <= 5.25),
      physicsCleared: trees.every((c) => !s.coverByCollider.has(c.collider.handle)),
    };
  });
  await page.screenshot({ path: "artifacts/stumps-game.png" });
  assert.ok(Object.values(destroyed).every(Boolean), JSON.stringify(destroyed));
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ ...result, ...destroyed, errors }));
} finally {
  await browser.close();
}
