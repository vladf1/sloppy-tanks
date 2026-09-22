import { test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three/webgpu";
import { PartBatches } from "../src/game/part-batches";
import { restoreBatchedLayers } from "../src/game/render-resources";

function fixture(count = 2) {
  const scene = new THREE.Scene();
  const root = new THREE.Group();
  const geometry = new THREE.PlaneGeometry(2, 3);
  const material = new THREE.MeshStandardMaterial({
    color: 0x41723a,
    alphaTest: 0.35,
    alphaToCoverage: true,
    side: THREE.DoubleSide,
  });
  const meshes = Array.from({ length: count }, (_, i) => {
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(i * 3, i, -i);
    mesh.rotation.set(i * 0.1, i * 0.3, i * 0.05);
    mesh.scale.set(1, 2, 0.7);
    mesh.castShadow = true;
    root.add(mesh);
    return mesh;
  });
  scene.add(root);
  const batches = new PartBatches();
  scene.add(batches.group);
  batches.rebuild([root]);
  const update = () => {
    scene.updateMatrixWorld(true);
    batches.update();
  };
  update();
  return { scene, root, geometry, material, meshes, batches, update };
}

test("GPU pose batches retain vertex positions, paint and cutout thresholds", () => {
  const f = fixture();
  f.root.position.set(3, 7, -4);
  f.root.rotation.y = 0.6;
  f.batches.group.position.set(5, 2, 1);
  f.update();
  assert.equal(f.batches.batches.length, 1);
  const batch = f.batches.batches[0];
  assert.equal(batch.mesh.material.alphaTest, 0.35);
  assert.equal(batch.mesh.material.alphaToCoverage, true);
  assert.equal(batch.mesh.material.side, THREE.DoubleSide);
  assert.equal((batch.mesh.material as THREE.MeshStandardNodeMaterial).color.getHex(), 0x41723a);
  const position = batch.mesh.geometry.getAttribute("position");
  const ids = batch.mesh.geometry.getAttribute("partIndex");
  for (let i = 0; i < position.count; i++) {
    const source = f.meshes[ids.getX(i)];
    const transform = new THREE.Matrix4().fromArray(batch.transforms, ids.getX(i) * 16);
    const actual = new THREE.Vector3()
      .fromBufferAttribute(position, i)
      .applyMatrix4(transform)
      .applyMatrix4(batch.mesh.matrixWorld);
    const expected = new THREE.Vector3()
      .fromBufferAttribute(f.geometry.getAttribute("position"), i % 4)
      .applyMatrix4(source.matrixWorld);
    assert.ok(actual.distanceTo(expected) < 1e-5);
  }
  assert.equal(
    f.geometry.getAttribute("partIndex"),
    undefined,
    "source geometry is borrowed unchanged",
  );
  f.batches.dispose();
});

test("poses and ancestor visibility update in place, and detached pieces render independently", () => {
  const f = fixture();
  const batch = f.batches.batches[0];
  const transforms = batch.transforms;
  const before = Array.from(transforms);
  f.root.rotation.x = 0.25;
  f.meshes[0].position.z -= 0.4;
  f.update();
  assert.equal(batch.transforms, transforms);
  assert.notDeepEqual(Array.from(transforms), before);
  f.root.visible = false;
  f.update();
  assert.equal(transforms[13], -1e6);
  f.root.visible = true;
  f.update();
  assert.notEqual(transforms[13], -1e6);
  const detached = f.root.clone(true);
  restoreBatchedLayers(detached);
  assert.equal(detached.children[0].layers.mask, 1);
  assert.equal(f.meshes[0].layers.mask, 0, "source is still drawn by its batch");
  assert.equal(f.root.visible, true);
  f.batches.dispose();
  assert.equal(f.meshes[0].layers.mask, 1);
  assert.equal(f.meshes[0].userData.batchedLayers, undefined);
});

test("native storage poses do not split at the old uniform-array limit", () => {
  const f = fixture(300);
  assert.equal(f.batches.batches.length, 1);
  assert.equal(f.batches.batches[0].parts.length, 300);
  assert.equal(f.batches.batches[0].transforms.length, 300 * 16);
  f.batches.dispose();
});

test("material batches share one pose allocation and release it exactly once", () => {
  const f = fixture(4);
  f.batches.dispose();
  const second = f.material.clone();
  f.meshes[2].material = second;
  f.meshes[3].material = second;
  const released: THREE.StorageBufferAttribute[] = [];
  const batches = new PartBatches((attribute) => released.push(attribute));
  batches.rebuild([f.root]);
  batches.update();
  assert.equal(batches.batches.length, 2);
  const [a, b] = batches.batches;
  assert.equal(a.transforms.buffer, b.transforms.buffer);
  assert.equal(a.mesh.geometry.getAttribute("partIndex").getX(0), 0);
  assert.equal(b.mesh.geometry.getAttribute("partIndex").getX(0), 2);
  batches.dispose();
  batches.dispose();
  assert.equal(released.length, 1);
  assert.equal(released[0].count, 4);
});

test("rebuilds dispose owned batches without disposing shared source resources", () => {
  const f = fixture();
  let owned = 0,
    shared = 0;
  f.geometry.addEventListener("dispose", () => shared++);
  f.material.addEventListener("dispose", () => shared++);
  for (let i = 0; i < 5; i++) {
    const batch = f.batches.batches[0];
    batch.mesh.geometry.addEventListener("dispose", () => owned++);
    batch.mesh.material.addEventListener("dispose", () => owned++);
    f.batches.rebuild([f.root]);
    f.update();
    assert.equal(f.batches.group.children.length, 1);
    assert.equal(f.root.children.length, 2);
  }
  assert.equal(owned, 10);
  assert.equal(shared, 0);
  f.batches.dispose();
  f.batches.dispose();
  assert.equal(shared, 0);
});

test("transparent and partial-range meshes keep their independent draws", () => {
  const f = fixture();
  f.batches.dispose();
  f.meshes[0].material = new THREE.MeshStandardMaterial({ transparent: true });
  f.meshes[1].geometry = f.geometry.clone();
  f.meshes[1].geometry.setDrawRange(0, 3);
  f.batches.rebuild([f.root]);
  assert.equal(f.batches.batches.length, 0);
  assert.ok(f.meshes.every((mesh) => mesh.layers.mask === 1));
  f.batches.dispose();
});
