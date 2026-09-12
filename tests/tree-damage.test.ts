import { after, mock, test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { freezeStatic } from "../src/game/batching";
import { TreeDebris } from "../src/game/tree-debris";
import { setTreeDamage, setTreeDestroyed, treeModel, TREE_FAMILIES } from "../src/game/tree-models";

mock.method(THREE.TextureLoader.prototype, "load", () => new THREE.Texture());
after(() => mock.restoreAll());
function families() {
  const trees = new Map<string, THREE.Group>();
  for (let i = 0; i < 100 && trees.size < TREE_FAMILIES.length; i++) {
    const tree = treeModel({ x: i * 3.1, z: 27, w: 2.6, d: 2.6, h: 6 });
    trees.set(tree.userData.family, tree);
  }
  assert.equal(trees.size, TREE_FAMILIES.length);
  return trees.values();
}

test("all tree families shed stable branches once per damage stage and retain the rooted stump", () => {
  for (const tree of families()) {
    const branches = tree.userData.branches as THREE.Group[];
    const stump = tree.getObjectByName("rooted-stump");
    const dropped: THREE.Group[] = [];
    const shed = (branch: THREE.Group) => dropped.push(branch);
    assert.equal(branches.length, 4);
    setTreeDamage(tree, 1, shed);
    assert.equal(dropped.length, 0);
    setTreeDamage(tree, 0.99, shed);
    assert.equal(dropped.length, 2);
    setTreeDamage(tree, 0.5, shed);
    assert.equal(dropped.length, 2, "repeated damage in the same stage must not duplicate debris");
    setTreeDamage(tree, 0.35, shed);
    assert.equal(dropped.length, 4);
    assert.equal(new Set(dropped).size, 4);
    assert.ok(branches.every((branch) => !branch.visible));
    assert.equal(tree.getObjectByName("rooted-stump"), stump);
    setTreeDestroyed(tree, true);
    assert.equal(tree.userData.crown.visible, false);
    assert.equal(tree.userData.cutSurface.visible, true);
    setTreeDamage(tree, 1);
    assert.ok(
      branches.every((branch) => branch.visible),
      "a restored tree regains its own branches",
    );
  }
});

test("falling branches start at the exact frozen tree pose, retain foliage, and settle on the ground", () => {
  for (const tree of families()) {
    freezeStatic(tree);
    const debris = new TreeDebris();
    const source = tree.userData.branches[0] as THREE.Group;
    const before = new THREE.Box3().setFromObject(source);
    assert.ok(
      before.getCenter(new THREE.Vector3()).distanceTo(tree.position) < 8,
      "branch recentering must preserve its location even far from the origin",
    );
    debris.shed(source);
    const branch = debris.branches[0];
    const after = new THREE.Box3().setFromObject(branch.model);
    assert.ok(before.min.distanceTo(after.min) < 1e-5);
    assert.ok(before.max.distanceTo(after.max) < 1e-5);
    const meshes = source.children as THREE.Mesh[];
    for (const [i, child] of (branch.model.children as THREE.Mesh[]).entries()) {
      assert.equal(child.geometry, meshes[i].geometry, "falling copies borrow existing geometry");
      assert.equal(child.material, meshes[i].material);
    }
    const initialY = branch.model.position.y;
    debris.update(0.1);
    assert.ok(branch.model.position.y < initialY);
    for (let i = 0; i < 180; i++) debris.update(1 / 60);
    assert.equal(branch.landed, true);
    assert.equal(branch.model.position.y, 0.2);
    assert.ok(branch.model.scale.y < branch.scale.y);
    debris.update(6);
    assert.equal(debris.branches.length, 0);
    assert.equal(debris.group.children.length, 0);
  }
});

test("branch effects remain bounded and round reset clears borrowed models", () => {
  const tree = treeModel({ x: 36, z: 45, w: 2.6, d: 2.6, h: 6 });
  const debris = new TreeDebris();
  for (let i = 0; i < 50; i++) debris.shed(tree.userData.branches[0]);
  assert.equal(debris.branches.length, 32);
  assert.equal(debris.group.children.length, 32);
  debris.reset();
  assert.equal(debris.branches.length, 0);
  assert.equal(debris.group.children.length, 0);
});

test("background trees never allocate detachable boughs", () => {
  const tree = treeModel({ x: 36, z: 45, w: 4.4, d: 4.4, h: 9 }, "background");
  const debris = new TreeDebris();
  setTreeDamage(tree, 0.2, debris.shed);
  assert.equal(tree.userData.branches.length, 0);
  assert.equal(debris.branches.length, 0);
});
