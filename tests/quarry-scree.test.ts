import assert from "node:assert/strict";
import test from "node:test";
import { quarryScreeSpots } from "../src/game/quarry-benches";
import { quarryScreeGeometry, quarryScreeRubble } from "../src/game/quarry-scree";

test("scree piles are closed solids with buried toes and sides", () => {
  for (const spot of quarryScreeSpots()) {
    const geometry = quarryScreeGeometry(spot);
    try {
      const positions = geometry.getAttribute("position");
      const indices = geometry.getIndex()!;
      const edges = new Map<string, { count: number; direction: number }>();
      const topEdges = new Map<string, { count: number; a: number; b: number }>();
      for (let i = 0; i < indices.count; i += 3) {
        const va = indices.getX(i);
        const vb = indices.getX(i + 1);
        const vc = indices.getX(i + 2);
        const upward =
          (positions.getZ(vb) - positions.getZ(va)) * (positions.getX(vc) - positions.getX(va)) -
          (positions.getX(vb) - positions.getX(va)) * (positions.getZ(vc) - positions.getZ(va));
        for (let j = 0; j < 3; j++) {
          const a = indices.getX(i + j);
          const b = indices.getX(i + ((j + 1) % 3));
          const key = `${Math.min(a, b)}/${Math.max(a, b)}`;
          const edge = edges.get(key) ?? { count: 0, direction: 0 };
          edge.count++;
          edge.direction += a < b ? 1 : -1;
          edges.set(key, edge);
          if (upward > 0.0001) {
            const topEdge = topEdges.get(key) ?? { count: 0, a, b };
            topEdge.count++;
            topEdges.set(key, topEdge);
          }
        }
      }
      for (const edge of edges.values()) {
        assert.equal(edge.count, 2, "no open sheet edges");
        assert.equal(edge.direction, 0, "adjacent faces have consistent winding");
      }
      for (const edge of topEdges.values()) {
        if (edge.count !== 1) continue;
        for (const index of [edge.a, edge.b]) {
          if (positions.getZ(index) < spot.depth - 0.001) {
            assert.ok(positions.getY(index) < 0, "exposed rim sinks into the ground");
          }
        }
      }
      let tallVertices = 0;
      let minToe = Infinity;
      let maxToe = -Infinity;
      for (let i = 0; i < positions.count; i++) {
        const x = positions.getX(i);
        const y = positions.getY(i);
        const z = positions.getZ(i);
        if (Math.abs(y + 0.16) < 0.001 && Math.abs(x) < spot.length * 0.4) {
          minToe = Math.min(minToe, z);
          maxToe = Math.max(maxToe, z);
        }
        if (y > spot.height / 2) tallVertices++;
      }
      assert.ok(tallVertices > 10, "the pile retains substantial relief");
      assert.ok(maxToe - minToe > 0.8, "the toe is irregular, not a straight cut");
    } finally {
      geometry.dispose();
    }
  }
});

test("actual sediment and rubble stay outside gameplay and reach their quarry cut", () => {
  for (const spot of quarryScreeSpots()) {
    const mound = quarryScreeGeometry(spot);
    const rubble = quarryScreeRubble(spot);
    try {
      for (const geometry of [mound, rubble]) {
        geometry.rotateY(spot.rotY);
        geometry.translate(spot.x, 0, spot.z);
        const positions = geometry.getAttribute("position");
        let outermost = 0;
        for (let i = 0; i < positions.count; i++) {
          const extent = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i)));
          assert.ok(extent > 68, "even loose fragments leave the arena and haul lane clear");
          assert.ok(extent < 85, "scenery remains confined to the first terrace");
          outermost = Math.max(outermost, extent);
        }
        assert.ok(outermost > 80, "the pile intersects the cut instead of ending in midair");
      }
      assert.ok(
        rubble.getAttribute("position").count / 3 < 7500,
        "rubble triangle count is bounded",
      );
    } finally {
      mound.dispose();
      rubble.dispose();
    }
  }
});
