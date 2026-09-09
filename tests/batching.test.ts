import { test, mock, after } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { batch, freezeStatic } from "../src/game/batching";
import { arenaLayout } from "../src/game/arena";
import { coverModel, tankModel, wreckModel } from "../src/game/models";

// Geometry tests do not decode external images; retain real textured materials.
mock.method(THREE.TextureLoader.prototype, "load", () => new THREE.Texture());
after(() => mock.restoreAll());

test("cached wrecks preserve assembly bounds and share geometry with independent transforms", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const)
    for (const team of [0, 1] as const)
      for (const part of ["hull", "turret", "turret-barrel", "barrel"] as const) {
        const source = tankModel(kind, team).userData;
        if (part === "turret") source.turret.remove(source.barrel);
        const assembly = part === "turret-barrel" ? source.turret : source[part];
        const pivot = new THREE.Box3().setFromObject(assembly).getCenter(new THREE.Vector3());
        const expected = new THREE.Box3().setFromObject(assembly, true).translate(pivot.negate());
        const a = wreckModel(kind, team, part),
          b = wreckModel(kind, team, part);
        const bounds = new THREE.Box3().setFromObject(a);
        assert.ok(bounds.min.distanceTo(expected.min) < 1e-5, `${kind} ${part} min`);
        assert.ok(bounds.max.distanceTo(expected.max) < 1e-5, `${kind} ${part} max`);
        assert.notEqual(a, b);
        a.position.x = 20;
        assert.equal(b.position.x, 0);
        for (const [i, mesh] of (a.children as THREE.Mesh[]).entries()) {
          assert.equal(mesh.geometry, (b.children[i] as THREE.Mesh).geometry);
          assert.equal(
            mesh.geometry.userData.owned,
            false,
            "round cleanup must retain shared geometry",
          );
        }
      }
});

test("textured batching preserves UVs and paint, and separates different surface maps", () => {
  const group = new THREE.Group(),
    wear = new THREE.Texture();
  const other = new THREE.Texture();
  const base = new THREE.BoxGeometry().toNonIndexed();
  const paints = [0x216ac8, 0xc73120, 0x216ac8, 0x216ac8];
  paints.forEach((color, i) =>
    group.add(
      new THREE.Mesh(
        base,
        new THREE.MeshStandardMaterial({
          color,
          map: i === 2 ? other : wear,
          bumpMap: wear,
          bumpScale: i === 3 ? 0.02 : 0.009,
        }),
      ),
    ),
  );
  batch(group);
  assert.equal(group.children.length, 3);
  const mesh = group.children[0] as THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial>;
  assert.equal(mesh.material.map, wear);
  assert.equal(mesh.material.bumpMap, wear);
  assert.equal(mesh.material.bumpScale, 0.009);
  const uv = mesh.geometry.getAttribute("uv"),
    original = base.getAttribute("uv");
  const colors = mesh.geometry.getAttribute("color");
  assert.equal(uv.count, original.count * 2);
  for (let i = 0; i < uv.count; i++) {
    assert.equal(uv.getX(i), original.getX(i % original.count));
    assert.equal(uv.getY(i), original.getY(i % original.count));
    const paint = new THREE.Color(paints[Math.floor(i / original.count)]);
    assert.ok(Math.abs(colors.getX(i) - paint.r) < 1e-7);
    assert.ok(Math.abs(colors.getY(i) - paint.g) < 1e-7);
    assert.ok(Math.abs(colors.getZ(i) - paint.b) < 1e-7);
  }
});

test("batching preserves triangle positions and linear colors while combining compatible paints", () => {
  const group = new THREE.Group();
  const paints = [0xc73120, 0x216ac8];
  for (const [i, color] of paints.entries()) {
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(1, 2, 3),
      new THREE.MeshStandardMaterial({ color }),
    );
    mesh.position.set(i * 5, 2, -3);
    mesh.rotation.y = 0.4;
    group.add(mesh);
  }
  const before = new THREE.Box3().setFromObject(group);
  batch(group);
  assert.equal(group.children.length, 1);
  const mesh = group.children[0] as THREE.Mesh;
  const positions = mesh.geometry.getAttribute("position"),
    colors = mesh.geometry.getAttribute("color");
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
    const mesh = hull.children.find((c) => c instanceof THREE.Mesh)!;
    const before = mesh.matrixWorld.clone();
    hull.rotation.y = Math.PI / 2;
    tank.position.x = 10;
    tank.updateMatrixWorld(true);
    assert.notDeepEqual(mesh.matrixWorld.elements, before.elements);
    assert.ok(
      mesh.matrixWorld.equals(new THREE.Matrix4().multiplyMatrices(hull.matrixWorld, mesh.matrix)),
    );
  }
  const cover = coverModel({ kind: "house", x: 15, z: -10, w: 8, d: 7, h: 6, color: 0xac7856 });
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
  for (const definition of arenaLayout().filter((c) => c.kind === "house")) {
    const house = coverModel(definition);
    batch(house);
    house.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return;
      triangles +=
        (object.geometry.index?.count ?? object.geometry.getAttribute("position").count) / 3;
      if (object.geometry.userData.owned) object.geometry.dispose();
    });
  }
  // Tiny bevels previously pushed these houses above 112k triangles.
  assert.ok(triangles <= 20_000, `House geometry exceeded its budget: ${triangles}`);
});

test("tree and fence detail stays within scenery budgets", () => {
  const triangles = (model: THREE.Group) => {
    let count = 0;
    model.traverse((object) => {
      if (object instanceof THREE.Mesh)
        count +=
          (object.geometry.index?.count ?? object.geometry.getAttribute("position").count) / 3;
    });
    return count;
  };
  for (const [kind, budget] of [
    ["tree", 9_000],
    ["fence", 3_500],
  ] as const) {
    const count = arenaLayout()
      .filter((c) => c.kind === kind)
      .reduce((sum, c) => sum + triangles(coverModel(c)), 0);
    assert.ok(count <= budget, `${kind}: ${count} triangles exceeds ${budget}`);
  }
  const tree = arenaLayout().find((c) => c.kind === "tree")!;
  assert.ok(triangles(coverModel(tree, "background")) <= 200);
});
