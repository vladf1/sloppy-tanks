import { test, mock, after } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { villageLandmarks } from "../src/game/village-landmarks";
import { VillageLandscape, creekDistance, valleyHeight } from "../src/game/village-landscape";
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
  const point = new THREE.Vector3();
  river.updateWorldMatrix(true, false);
  for (let i = 0; i < p.count; i++) {
    point.fromBufferAttribute(p, i).applyMatrix4(river.matrixWorld);
    assert.ok(Math.abs(point.x) > 63 || Math.abs(point.z) > 63, "creek crosses the arena wall");
    assert.ok(Math.abs(point.y + 2.65) < 0.0001, "creek must stay on its horizontal mirror plane");
  }
  for (let x = -60; x <= 60; x += 10)
    for (let z = -60; z <= 60; z += 10)
      assert.ok(valleyHeight(x, z) < -0.5, "landscape protrudes through the playable ground");
});

test("creek distance matches the sampled creek polyline", () => {
  const creek = new THREE.CatmullRomCurve3(
    [
      [150, -107],
      [85, -86],
      [15, -82],
      [-57, -85],
      [-81, -64],
      [-81, -12],
      [-86, 48],
      [-110, 145],
    ].map(([x, z]) => new THREE.Vector3(x, -2.65, z)),
  );
  const points = creek.getPoints(160);
  const segment = new THREE.Line3();
  const closest = new THREE.Vector3();
  for (let x = -170; x <= 170; x += 7.3) {
    for (let z = -170; z <= 170; z += 6.1) {
      const query = new THREE.Vector3(x, 0, z);
      let expected = Infinity;
      for (let i = 1; i < points.length; i++) {
        segment.start.set(points[i - 1].x, 0, points[i - 1].z);
        segment.end.set(points[i].x, 0, points[i].z);
        expected = Math.min(
          expected,
          segment.closestPointToPoint(query, true, closest).distanceTo(query),
        );
      }
      assert.ok(Math.abs(creekDistance(x, z) - expected) < 1e-9, `creek distance at ${x}, ${z}`);
    }
  }
});

test("chimney smoke follows permanent cottages and reuses buffers across map resets", () => {
  const smoke = new VillageAtmosphere();
  const covers = arenaLayout()
    .filter((c) => c.kind === "house")
    .map((c) => ({ ...c, destructible: Number.isFinite(c.hp) })) as Cover[];
  const geometry = smoke.mesh.geometry,
    positions = geometry.getAttribute("smokeOrigin");
  smoke.setCovers(covers);
  assert.equal(geometry.instanceCount, (covers.filter((c) => !c.destructible).length + 1) * 8);
  smoke.setCovers([]);
  assert.equal(geometry.instanceCount, 8, "only the mill remains without village cottages");
  smoke.setCovers(covers);
  assert.equal(
    geometry.getAttribute("smokeOrigin"),
    positions,
    "reset must reuse the particle buffer",
  );
});
