import { test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { tankModel } from "../src/game/tank-model";
import { wreckModel } from "../src/game/wreck-model";

test("detached hulls expose a recessed turret socket while live hull decks stay intact", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    for (const team of [0, 1] as const) {
      const live = tankModel(kind, team).userData.hull;
      const center = new THREE.Box3().setFromObject(live).getCenter(new THREE.Vector3());
      const wreck = wreckModel(kind, team, "hull");
      wreck.updateMatrixWorld(true);
      const down = new THREE.Vector3(0, -1, 0);
      const liveHit = new THREE.Raycaster(new THREE.Vector3(0, 3, -0.12), down).intersectObject(
        live,
      )[0];
      const floor = new THREE.Raycaster(
        new THREE.Vector3(-center.x, 3, -0.12 - center.z),
        down,
      ).intersectObject(wreck)[0];
      assert.ok(liveHit && floor);
      assert.ok(
        liveHit.point.y - (floor.point.y + center.y) > 0.2,
        `${kind}: socket must be an actual recess`,
      );
      const radius = kind === "scout" ? 0.5 : 0.65;
      const rim = new THREE.Raycaster(
        new THREE.Vector3(radius + 0.04 - center.x, 3, -0.12 - center.z),
        down,
      ).intersectObject(wreck)[0];
      assert.ok(rim);
      assert.ok(
        Math.abs(rim.point.y + center.y - liveHit.point.y) < 1e-5,
        "rim stays flush with deck",
      );
      const wall = new THREE.Raycaster(
        new THREE.Vector3(-center.x, floor.point.y + 0.08, -0.12 - center.z),
        new THREE.Vector3(1, 0, 0),
      ).intersectObject(wreck)[0];
      assert.ok(wall && Math.abs(wall.distance - radius) < 1e-5, "socket has inward-facing walls");
      const socket = wall.object as THREE.Mesh<THREE.BufferGeometry, THREE.Material>;
      assert.equal(socket.material.side, THREE.FrontSide, "the wall shares front-side paint");
      assert.ok(
        wall.face!.normal.x < 0 && socket.geometry.getAttribute("normal").getX(wall.face!.a) < 0,
        "its triangles and normals face into the socket",
      );
    }
  }
});
