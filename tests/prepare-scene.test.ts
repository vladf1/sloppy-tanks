import assert from "node:assert/strict";
import { test } from "node:test";
import * as THREE from "three/webgpu";
import { exposeWarmupObjects } from "../src/game/prepare-scene";

test("warm-up includes empty pools and bundles then restores presentation state", () => {
  const scene = new THREE.Scene();
  const bundle = new THREE.BundleGroup();
  const geometry = new THREE.BoxGeometry();
  const material = new THREE.MeshBasicMaterial();
  const pool = new THREE.InstancedMesh(geometry, material, 4);
  pool.count = 0;
  pool.visible = false;
  pool.layers.mask = 0;
  bundle.add(pool);
  scene.add(bundle);
  const version = bundle.version;
  const restore = exposeWarmupObjects(scene);
  try {
    assert.equal(bundle.isBundleGroup, false);
    assert.equal(pool.visible, true);
    assert.equal(pool.count, 1);
    assert.equal(pool.frustumCulled, false);
    assert.equal(pool.layers.mask, 0, "batched source meshes must stay excluded");
  } finally {
    restore();
  }
  assert.equal(bundle.isBundleGroup, true);
  assert.equal(bundle.version, version + 1);
  assert.equal(pool.visible, false);
  assert.equal(pool.count, 0);
  assert.equal(pool.frustumCulled, true);
  pool.dispose();
  geometry.dispose();
  material.dispose();
});
