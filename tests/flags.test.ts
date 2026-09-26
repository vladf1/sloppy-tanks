import assert from "node:assert/strict";
import test from "node:test";
import type * as THREE from "three/webgpu";
import { Flags } from "../src/game/flags";

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
