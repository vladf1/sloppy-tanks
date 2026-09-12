import { test, mock, after } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { villageLandmarks } from "../src/game/village-landmarks";
import { VillageLandscape, valleyHeight } from "../src/game/village-landscape";
import { VillageAtmosphere } from "../src/game/village-atmosphere";
import { arenaLayout } from "../src/game/arena";
import type { Cover } from "../src/game/types";

mock.method(THREE.TextureLoader.prototype, "load", () => new THREE.Texture());
after(() => mock.restoreAll());

test("village landmarks and flowing creek stay outside the combat arena", () => {
  const { group, wheel } = villageLandmarks();
  for (const child of group.children) {
    const bounds = new THREE.Box3().setFromObject(child);
    assert.ok(
      bounds.max.x < -60 || bounds.min.x > 60 || bounds.max.z < -60 || bounds.min.z > 60,
      `${child.name} intrudes into combat`,
    );
  }
  group.updateMatrixWorld(true);
  const before = wheel.children[0].matrixWorld.clone();
  wheel.rotation.z = 0.3;
  group.updateMatrixWorld(true);
  assert.ok(
    !wheel.children[0].matrixWorld.equals(before),
    "batched wheel still turns with its assembly",
  );
  const landscape = new VillageLandscape(new THREE.MeshStandardMaterial({ vertexColors: true }));
  const river = landscape.group.getObjectByName("village-creek") as THREE.Mesh;
  const p = river.geometry.getAttribute("position");
  for (let i = 0; i < p.count; i++)
    assert.ok(Math.abs(p.getX(i)) > 63 || Math.abs(p.getZ(i)) > 63, "creek crosses the arena wall");
  for (let x = -60; x <= 60; x += 10)
    for (let z = -60; z <= 60; z += 10)
      assert.ok(valleyHeight(x, z) < -0.5, "landscape protrudes through the playable ground");
});

test("chimney smoke follows permanent cottages and reuses buffers across map resets", () => {
  const smoke = new VillageAtmosphere();
  const covers = arenaLayout()
    .filter((c) => c.kind === "house")
    .map((c) => ({ ...c, destructible: Number.isFinite(c.hp) })) as Cover[];
  const geometry = smoke.mesh.geometry,
    positions = geometry.getAttribute("position");
  smoke.setCovers(covers);
  assert.equal(geometry.drawRange.count, (covers.filter((c) => !c.destructible).length + 1) * 8);
  smoke.setCovers([]);
  assert.equal(geometry.drawRange.count, 8, "only the mill remains without village cottages");
  smoke.setCovers(covers);
  assert.equal(
    geometry.getAttribute("position"),
    positions,
    "reset must reuse the particle buffer",
  );
});
