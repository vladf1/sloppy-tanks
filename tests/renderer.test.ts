import { test } from "node:test";
import assert from "node:assert/strict";
import { NodeMaterial, BackSide, MeshStandardMaterial, Texture } from "three/webgpu";
import { vec4 } from "three/tsl";
import {
  GameRenderer,
  ShadowMaterials,
  numberUniformsInOrder,
  shareTexturedShadowNodes,
} from "../src/game/renderer";

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

test("equivalent shaders get equal uniform names whatever the other stage declared", () => {
  // A part batch's vertex-stage matrices take the first number, shifting every
  // fragment uniform by one compared with the same material on a single mesh.
  const batched = [
    "@binding( 1 ) @group( 1 ) var nodeUniform14_sampler : sampler;",
    "@binding( 2 ) @group( 1 ) var nodeUniform14 : texture_2d<f32>;",
    "struct objectStruct { nodeUniform1 : vec3<f32>, nodeUniform10 : f32 };",
    "DiffuseColor = vec4<f32>( object.nodeUniform1, object.nodeUniform10 );",
    "Output = textureSample( nodeUniform14, nodeUniform14_sampler, uv ) * DiffuseColor;",
  ].join("\n");
  const single = batched.replace(/nodeUniform(\d+)/g, (_, n: string) => `nodeUniform${+n - 1}`);
  assert.notEqual(single, batched);
  const renamed = numberUniformsInOrder(batched);
  assert.equal(renamed, numberUniformsInOrder(single));
  assert.match(renamed, /var nodeUniform0_sampler : sampler;\n.*var nodeUniform0 : texture_2d/);
  assert.match(renamed, /object\.nodeUniform1, object\.nodeUniform2 \)/);
  assert.equal(
    numberUniformsInOrder("nodeUniform3 nodeUniform30 nodeUniform0 nodeUniform3"),
    "nodeUniform0 nodeUniform1 nodeUniform2 nodeUniform0",
    "distinct uniforms keep distinct names",
  );
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

test("fading copies of a textured material share its shadow nodes", () => {
  let derived = 0;
  const renderer: Parameters<typeof shareTexturedShadowNodes>[0] = {
    _getShadowNodes: () => ({ colorNode: ++derived, depthNode: null, positionNode: null }),
  };
  shareTexturedShadowNodes(renderer);
  const bark = new Texture();
  const source = new MeshStandardMaterial({ map: bark });
  const nodes = renderer._getShadowNodes(source);
  assert.equal(renderer._getShadowNodes(source.clone()), nodes, "a copy reuses the nodes");
  assert.equal(renderer._getShadowNodes(source.clone()), nodes);
  const other = new MeshStandardMaterial({ map: new Texture() });
  assert.notEqual(renderer._getShadowNodes(other), nodes, "another map needs its own alpha");
  const plain = new MeshStandardMaterial();
  renderer._getShadowNodes(plain);
  renderer._getShadowNodes(plain.clone());
  const custom = new NodeMaterial();
  renderer._getShadowNodes(custom);
  renderer._getShadowNodes(custom);
  assert.equal(derived, 6, "untextured and node materials keep Three's own derivation");
});
