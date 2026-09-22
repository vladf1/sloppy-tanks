import { test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three/webgpu";
import Info from "three/src/renderers/common/Info.js";
import {
  RenderResources,
  trackInterleavedBuffers,
  type DrawResources,
} from "../src/game/renderer-resources";

test("removed objects release draw bindings without disposing borrowed resources", () => {
  const root = new THREE.Group();
  const geometry = new THREE.BoxGeometry();
  const material = new THREE.MeshBasicMaterial();
  const mesh = new THREE.Mesh(geometry, material);
  root.add(mesh);
  let bindings = 0,
    shared = 0;
  geometry.addEventListener("dispose", () => shared++);
  material.addEventListener("dispose", () => shared++);
  const resources = new RenderResources(() => shared++);
  const draw = (): DrawResources => ({
    object: mesh,
    geometry,
    getAttributes: () => [geometry.getAttribute("position")],
    onDispose: () => bindings++,
    dispose() {
      this.onDispose();
    },
  });
  const main = draw(),
    shadow = draw();
  resources.track(main);
  resources.track(main);
  resources.track(shadow);
  main.dispose();
  resources.release(root);
  resources.release(root);
  assert.equal(bindings, 2, "each pass releases once, including material-driven disposal");
  assert.equal(shared, 0);
});

test("owned geometry releases the union of shadow and color-pass attributes", () => {
  const geometry = new THREE.BoxGeometry();
  geometry.userData.owned = true;
  const object = new THREE.Mesh(geometry);
  const removed = new Set<THREE.BufferAttribute | THREE.InterleavedBufferAttribute>();
  const resources = new RenderResources((attribute) => removed.add(attribute));
  for (const names of [["position"], ["position", "normal", "uv"]]) {
    resources.track({
      object,
      geometry,
      getAttributes: () => names.map((name) => geometry.getAttribute(name)),
      onDispose() {},
      dispose() {},
    });
  }
  geometry.dispose();
  assert.equal(removed.size, 3);
});

test("shader rebuilds count a shared interleaved allocation once and release its accounting", () => {
  const info = new Info();
  trackInterleavedBuffers(info);
  const data = new THREE.InstancedInterleavedBuffer(new Float32Array(128 * 16), 16);
  const views = Array.from(
    { length: 12 },
    (_, i) => new THREE.InterleavedBufferAttribute(data, 4, (i % 4) * 4),
  );
  // Three's declaration omits the interleaved attributes accepted at runtime.
  for (const view of views) {
    info.createAttribute(view as unknown as THREE.BufferAttribute);
  }
  assert.equal(info.memory.attributes, 1);
  assert.equal(info.memory.attributesSize, data.array.byteLength);
  for (const view of [...views].reverse()) {
    info.destroyAttribute(view as unknown as THREE.BufferAttribute);
  }
  assert.equal(info.memory.attributes, 0);
  assert.equal(info.memory.attributesSize, 0);
  info.createAttribute(views[0] as unknown as THREE.BufferAttribute);
  assert.equal(info.memory.attributes, 1, "reallocation can be counted again");
});
