import { test, mock, after } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { batch, freezeStatic } from "../src/game/batching";
import { arenaLayout } from "../src/game/arena";
import { coverModel, tankModel } from "../src/game/models";

// Geometry tests do not decode external images; retain real textured materials.
mock.method(THREE.TextureLoader.prototype, "load", () => new THREE.Texture());
after(() => mock.restoreAll());

test("batching preserves triangle positions and linear colors while combining compatible paints", () => {
  const group = new THREE.Group();
  const paints = [0xc73120, 0x216ac8];
  for (const [i, color] of paints.entries()) {
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(1, 2, 3), new THREE.MeshStandardMaterial({color}));
    mesh.position.set(i * 5, 2, -3);
    mesh.rotation.y = 0.4;
    group.add(mesh);
  }
  const before = new THREE.Box3().setFromObject(group);
  batch(group);
  assert.equal(group.children.length, 1);
  const mesh = group.children[0] as THREE.Mesh;
  const positions = mesh.geometry.getAttribute("position"), colors = mesh.geometry.getAttribute("color");
  assert.equal(positions.count, 72);
  for (let i = 0; i < colors.count; i++) {
    const expected = new THREE.Color(paints[Math.floor(i / 36)]);
    assert.ok(Math.abs(colors.getX(i) - expected.r) < 1e-7);
    assert.ok(Math.abs(colors.getY(i) - expected.g) < 1e-7);
    assert.ok(Math.abs(colors.getZ(i) - expected.b) < 1e-7);
  }
  const after = new THREE.Box3().setFromObject(group);
  assert.ok(before.min.distanceTo(after.min) < 1e-6);
  assert.ok(before.max.distanceTo(after.max) < 1e-6);
  assert.ok(mesh.castShadow && mesh.receiveShadow);
});

test("batching keeps metallic, emissive and tone-mapping responses separate", () => {
  const group = new THREE.Group();
  const materials = [
    new THREE.MeshStandardMaterial({ color: 0xff0000 }),
    new THREE.MeshStandardMaterial({ color: 0x00ff00, metalness: 0.8 }),
    new THREE.MeshStandardMaterial({ color: 0x0000ff, emissive: 0x0000ff }),
    new THREE.MeshStandardMaterial({ color: 0x00ffff, toneMapped: false }),
  ];
  for (const mat of materials) group.add(new THREE.Mesh(new THREE.BoxGeometry(), mat));
  batch(group);
  assert.equal(group.children.length, 4);
  assert.equal((group.children[2] as THREE.Mesh).material, materials[2]);
  assert.equal(materials[0].color.getHex(), 0xff0000);
  assert.equal(materials[0].vertexColors, false);
});

test("batched tank assemblies still follow their moving parents and static cover keeps its pose", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    const tank = tankModel(kind, 0);
    const hull = tank.userData.hull as THREE.Group;
    batch(hull);
    tank.updateMatrixWorld(true);
    const mesh = hull.children.find(c => c instanceof THREE.Mesh)!;
    const before = mesh.matrixWorld.clone();
    hull.rotation.y = Math.PI / 2;
    tank.position.x = 10;
    tank.updateMatrixWorld(true);
    assert.notDeepEqual(mesh.matrixWorld.elements, before.elements);
    assert.ok(mesh.matrixWorld.equals(new THREE.Matrix4().multiplyMatrices(hull.matrixWorld, mesh.matrix)));
  }
  const cover = coverModel({kind:"house", x:15, z:-10, w:8, d:7, h:6, color:0xac7856});
  batch(cover);
  const before = new THREE.Box3().setFromObject(cover);
  freezeStatic(cover);
  cover.visible = false;
  cover.updateMatrixWorld(true);
  cover.visible = true;
  const after = new THREE.Box3().setFromObject(cover);
  assert.ok(before.equals(after));
});


test("detailed arena houses stay within the scenery polygon budget", () => {
  let triangles = 0;
  for (const definition of arenaLayout().filter(c => c.kind === "house")) {
    const house = coverModel(definition);
    batch(house);
    house.traverse(object => {
      if (!(object instanceof THREE.Mesh)) return;
      triangles += (object.geometry.index?.count ?? object.geometry.getAttribute("position").count) / 3;
      if (object.geometry.userData.owned) object.geometry.dispose();
    });
  }
  // Tiny bevels previously pushed these houses above 112k triangles.
  assert.ok(triangles <= 20_000, `House geometry exceeded its budget: ${triangles}`);
});

test("tree and fence detail stays within scenery budgets", () => {
  const triangles = (model: THREE.Group) => {
    let count = 0;
    model.traverse(object => {
      if (object instanceof THREE.Mesh)
        count += (object.geometry.index?.count ?? object.geometry.getAttribute("position").count) / 3;
    });
    return count;
  };
  for (const [kind, budget] of [["tree", 9_000], ["fence", 3_500]] as const) {
    const count = arenaLayout().filter(c => c.kind === kind)
      .reduce((sum, c) => sum + triangles(coverModel(c)), 0);
    assert.ok(count <= budget, `${kind}: ${count} triangles exceeds ${budget}`);
  }
  const tree = arenaLayout().find(c => c.kind === "tree")!;
  assert.ok(triangles(coverModel(tree, "background")) <= 200);
});
