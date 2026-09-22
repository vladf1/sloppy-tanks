import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { chromium } from "playwright";

const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const output = "artifacts/performance/pickup-atlas";
mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
const errors = [];
const requests = [];
try {
  const page = await browser.newPage({
    viewport: { width: 1000, height: 700 },
    deviceScaleFactor: 1,
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("request", (request) => {
    if (request.url().includes("/textures/pickups/")) requests.push(request.url());
  });
  await page.addInitScript(() => {
    const raf = requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => (callback.name === "loop" ? 1 : raf(callback));
  });
  await page.goto(url);
  await page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
  );
  assert.equal(requests.length, 1, "one early atlas download, reused by TextureLoader");
  assert.ok(requests[0].endsWith("/textures/pickups/atlas.webp"));
  const resources = await page.evaluate(async () => {
    const d = window.sloppy;
    const { PICKUP_ATLAS_TILES, pickupAtlasUV } = await import(
      new URL("src/game/pickup-atlas.ts", location.href).href
    );
    const scene = new d.view.scene.constructor();
    scene.background = d.view.scene.background.clone();
    for (const light of d.view.scene.children.filter((object) => object.isLight))
      scene.add(light.clone());
    const camera = d.view.camera.clone();
    camera.aspect = 1000 / 700;
    camera.updateProjectionMatrix();
    const bodies = [];
    const kinds = Object.keys(PICKUP_ATLAS_TILES);
    for (const [i, kind] of kinds.entries()) {
      const pickup = d.sim.pickups.find((pickup) => pickup.kind === kind);
      const gem = d.view.pickupMeshes.get(pickup.id).userData.gem.clone(true);
      gem.position.set(((i % 3) - 1) * 3, 0, (Math.floor(i / 3) - 1) * 3);
      gem.rotation.y = 0.2;
      gem.visible = true;
      scene.add(gem);
      gem.traverse((mesh) => {
        if (mesh.isMesh && mesh.material.emissiveMap) bodies.push({ kind, mesh });
      });
    }
    const sharedMaterials = new Set(bodies.map(({ mesh }) => mesh.material)).size;
    const sharedTextures = new Set(bodies.map(({ mesh }) => mesh.material.map)).size;
    // Only the reference comparison fetches individual icons, after verifying
    // that the real game's complete startup downloaded the atlas exactly once.
    for (const body of bodies) {
      const { mesh, kind } = body;
      body.atlasMaterial = mesh.material;
      body.atlasGeometry = mesh.geometry;
      const image = new Image();
      image.crossOrigin = "anonymous";
      image.src = new URL(`textures/pickups/${kind}.webp`, location.href).href;
      await image.decode();
      // Texture.clone shares its Source; a new texture keeps the atlas image intact.
      const texture = new mesh.material.map.constructor(image);
      texture.colorSpace = mesh.material.map.colorSpace;
      texture.anisotropy = mesh.material.map.anisotropy;
      texture.needsUpdate = true;
      body.referenceMaterial = mesh.material.clone();
      body.referenceMaterial.map = body.referenceMaterial.emissiveMap = texture;
      body.referenceGeometry = mesh.geometry.clone();
      const uv = body.referenceGeometry.getAttribute("uv");
      const [u0, v0] = pickupAtlasUV(kind, 0, 0);
      const [u1, v1] = pickupAtlasUV(kind, 1, 1);
      for (let j = 0; j < uv.count; j++)
        uv.setXY(j, (uv.getX(j) - u0) / (u1 - u0), (uv.getY(j) - v0) / (v1 - v0));
    }
    const renderer = d.view.renderer;
    document.body.replaceChildren(renderer.domElement);
    document.body.style.margin = "0";
    renderer.setPixelRatio(1);
    renderer.setSize(1000, 700);
    window.drawPickupAtlas = async (atlas, distance) => {
      for (const body of bodies) {
        body.mesh.material = atlas ? body.atlasMaterial : body.referenceMaterial;
        body.mesh.geometry = atlas ? body.atlasGeometry : body.referenceGeometry;
      }
      camera.position.set(distance * 0.3, distance * 0.8, distance);
      camera.lookAt(0, 0, 0);
      await renderer.compileAsync(scene, camera);
      for (let i = 0; i < 3; i++) {
        renderer.render(scene, camera);
        await renderer.waitForPipelineCompilation();
      }
    };
    return { kinds, sharedMaterials, sharedTextures };
  });
  assert.equal(resources.kinds.length, 9);
  assert.equal(resources.sharedMaterials, 1);
  assert.equal(resources.sharedTextures, 1);
  const comparisons = [];
  for (const distance of [10, 35]) {
    const images = [];
    for (const atlas of [true, false]) {
      await page.evaluate(({ atlas, distance }) => window.drawPickupAtlas(atlas, distance), {
        atlas,
        distance,
      });
      const path = `${output}/${atlas ? "atlas" : "reference"}-${distance}.png`;
      await page.locator("canvas").screenshot({ path });
      const image = await loadImage(path);
      const context = createCanvas(image.width, image.height).getContext("2d");
      context.drawImage(image, 0, 0);
      images.push(context.getImageData(0, 0, image.width, image.height).data);
    }
    let difference = 0,
      changed = 0;
    for (let i = 0; i < images[0].length; i += 4) {
      let pixel = 0;
      for (let channel = 0; channel < 3; channel++)
        pixel += Math.abs(images[0][i + channel] - images[1][i + channel]);
      difference += pixel;
      if (pixel > 24) changed++;
    }
    const pixels = images[0].length / 4;
    const result = {
      distance,
      meanChannelDifference: difference / (pixels * 3),
      changedFraction: changed / pixels,
    };
    comparisons.push(result);
    assert.ok(result.meanChannelDifference < 1, "atlas preserves icon appearance");
    assert.ok(result.changedFraction < 0.01, "no tile swaps, flips or visible edge bleed");
  }
  assert.deepEqual(errors, []);
  writeFileSync(
    `${output}/checks.json`,
    JSON.stringify({ ...resources, comparisons, errors }, null, 2),
  );
  console.log(JSON.stringify({ ...resources, comparisons, errors }));
} finally {
  await browser.close();
}
