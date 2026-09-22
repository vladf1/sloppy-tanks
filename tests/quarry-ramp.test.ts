import assert from "node:assert/strict";
import test from "node:test";
import { ARENA } from "../src/game/data";
import {
  QUARRY_RAMP,
  quarryRampBoulders,
  quarryRampGeometry,
  quarryRampHeight,
  quarryRampSpoil,
} from "../src/game/quarry-ramp";

test("east haul ramp stays on the apron and blends into the pit floor", () => {
  const { floor, crest, zCrest, zFoot, xBerm, x0, x1, z0, z1 } = QUARRY_RAMP;
  // Beyond the survey stakes and boundary paint, and north of the parked truck bay.
  assert.ok(x0 > ARENA + 5);
  assert.ok(z0 >= 26);
  const geometry = quarryRampGeometry();
  try {
    const positions = geometry.getAttribute("position");
    for (let i = 0; i < positions.count; i++) {
      assert.ok(positions.getY(i) >= floor - 0.25 - 1e-6, "no spikes below the apron");
      assert.ok(positions.getY(i) <= crest + 0.6, "nothing towers above the landing berm");
    }
    // Grid edges are buried, so no open sheet edge shows above the floor.
    for (let x = x0; x <= x1; x += 0.5) {
      assert.ok(quarryRampHeight(x, z0) < floor, `north edge buried at x=${x}`);
      assert.ok(quarryRampHeight(x, z1) < floor, `south edge buried at x=${x}`);
    }
    for (let z = z0; z <= z1; z += 0.5) {
      assert.ok(quarryRampHeight(x0, z) < floor, `outer edge buried at z=${z}`);
      assert.ok(quarryRampHeight(x1, z) < crest - 2, `inner edge under the shelf at z=${z}`);
    }
  } finally {
    geometry.dispose();
  }
  // A drivable haul grade: under 12 degrees anywhere along the wheel path.
  const lane = xBerm + 3;
  assert.ok(Math.abs(quarryRampHeight(lane, zCrest + 1) - crest) < 0.01);
  assert.ok(Math.abs(quarryRampHeight(lane, zFoot) - floor) < 0.01);
  for (let z = zCrest; z < zFoot; z += 0.25) {
    const grade = Math.abs(quarryRampHeight(lane, z + 0.25) - quarryRampHeight(lane, z)) / 0.25;
    assert.ok(grade < Math.tan((12 * Math.PI) / 180), `grade ${grade} at z=${z}`);
  }
  for (const rock of [...quarryRampBoulders(), ...quarryRampSpoil()]) {
    assert.ok(rock.x > x0 && rock.x < x1 && rock.z > z0 && rock.z < z1, "dressing on the ramp");
  }
});
