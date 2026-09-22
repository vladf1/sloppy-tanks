import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three/webgpu";
import { spawnPositions } from "../src/game/arena";
import { Flags } from "../src/game/flags";

test("every flag has a visible pole and cloth at its team spawn", () => {
  const flags = new Flags();
  const poles = flags.group.children.find(
    (child) => child.name === "flag-pole",
  ) as THREE.InstancedMesh;
  const cloth = flags.group.children.filter(
    (child) => child.name === "flag-cloth",
  ) as THREE.InstancedMesh[];
  const expected = [spawnPositions(0), spawnPositions(1)];
  assert.equal(poles.count, expected[0].length + expected[1].length);
  assert.equal(cloth.length, 2);
  const matrix = new THREE.Matrix4();
  let poleIndex = 0;
  for (const team of [0, 1] as const) {
    assert.equal(cloth[team].count, expected[team].length);
    for (const [i, spawn] of expected[team].entries()) {
      poles.getMatrixAt(poleIndex++, matrix);
      assert.deepEqual(
        new THREE.Vector3().setFromMatrixPosition(matrix).toArray(),
        [team === 0 ? -62 : 62, 2.4, spawn.z].map(Math.fround),
      );
      cloth[team].getMatrixAt(i, matrix);
      assert.deepEqual(
        new THREE.Vector3().setFromMatrixPosition(matrix).toArray(),
        [team === 0 ? -62 : 62, 4.6, spawn.z].map(Math.fround),
      );
      assert.ok(cloth[team].castShadow && cloth[team].receiveShadow);
    }
  }
});

test("wind animation retains geometry, instance buffers, and conservative bounds", () => {
  const flags = new Flags();
  const cloth = flags.group.children.filter(
    (child) => child.name === "flag-cloth",
  ) as THREE.InstancedMesh[];
  const before = cloth.map((mesh) => ({
    position: (mesh.geometry.attributes.position as THREE.BufferAttribute).version,
    normal: (mesh.geometry.attributes.normal as THREE.BufferAttribute).version,
    instances: mesh.instanceMatrix.version,
    bounds: mesh.boundingSphere!.clone(),
  }));
  for (let i = 0; i < 1200; i++) flags.update(i / 60);
  for (const [i, mesh] of cloth.entries()) {
    assert.equal(
      (mesh.geometry.attributes.position as THREE.BufferAttribute).version,
      before[i].position,
    );
    assert.equal(
      (mesh.geometry.attributes.normal as THREE.BufferAttribute).version,
      before[i].normal,
    );
    assert.equal(mesh.instanceMatrix.version, before[i].instances);
    assert.ok(mesh.boundingSphere!.equals(before[i].bounds));
    assert.equal(mesh.geometry.boundingSphere!.radius, 2);
  }
});
