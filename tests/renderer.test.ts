import { test } from "node:test";
import assert from "node:assert/strict";
import { NodeMaterial, BackSide } from "three/webgpu";
import { vec4 } from "three/tsl";
import { GameRenderer, ShadowMaterials } from "../src/game/renderer";

test("failed renderer startup releases the backend without restarting initialization", () => {
  const renderer = Object.create(GameRenderer.prototype) as GameRenderer;
  let releases = 0;
  Object.defineProperty(renderer, "initialized", { value: false });
  Object.defineProperty(renderer, "backend", { value: { dispose: () => releases++ } });
  renderer.init = () => {
    assert.fail("failure cleanup must not retry GPU initialization");
  };
  renderer.dispose();
  assert.equal(releases, 1);
});

test("alternating solid and cutout shadows keeps both shader versions stable", () => {
  const cache = new ShadowMaterials();
  const source = new NodeMaterial();
  source.side = BackSide;
  source.colorNode = vec4(0, 0, 0, 1);
  const solid = cache.get(source, 0);
  const foliage = cache.get(source, 0.35);
  const versions = [solid.version, foliage.version];
  for (let i = 0; i < 20; i++) {
    source.alphaTest = i % 2 ? 0.35 : 0;
    const material = cache.get(source, source.alphaTest);
    material.alphaTest = source.alphaTest;
    assert.equal(material, i % 2 ? foliage : solid);
    assert.equal(material.side, BackSide);
    assert.equal((material as NodeMaterial).colorNode, source.colorNode);
  }
  assert.deepEqual([solid.version, foliage.version], versions);
  assert.notEqual(cache.get(source, 0.6), foliage, "distinct cutoffs retain their thresholds");
  source.dispose();
});

test("shadow variants are isolated per light template and disposed with their owner", () => {
  const cache = new ShadowMaterials();
  const first = new NodeMaterial();
  const second = new NodeMaterial();
  const solid = cache.get(first, 0);
  const foliage = cache.get(first, 0.35);
  const otherLight = cache.get(second, 0);
  let disposed = 0;
  for (const variant of [solid, foliage, otherLight]) {
    variant.addEventListener("dispose", () => disposed++);
  }
  assert.notEqual(solid, otherLight);
  first.dispose();
  assert.equal(disposed, 2);
  first.dispose();
  assert.equal(disposed, 2, "owner cleanup is idempotent");
  second.dispose();
  assert.equal(disposed, 3);
});
