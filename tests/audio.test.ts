import { test } from "node:test";
import assert from "node:assert/strict";
import { AudioSystem, destructionSound } from "../src/game/audio";
import type { CoverKind } from "../src/game/types";

test("cover destruction uses material sounds while drums defer to their blast", () => {
  for (const kind of ["tree", "timber", "cargo"] as const) {
    assert.equal(destructionSound({ type: "destroy", coverKind: kind, x: 0, z: 0 }), "wood-break");
  }
  for (const kind of ["tower", "rock", "concrete", "house"] as CoverKind[]) {
    assert.equal(
      destructionSound({ type: "destroy", coverKind: kind, x: 0, z: 0 }),
      "rubble-break",
    );
  }
  assert.equal(destructionSound({ type: "destroy", coverKind: "drum", x: 0, z: 0 }), null);
  assert.equal(destructionSound({ type: "death", x: 0, z: 0 }), null);
});

test("scenery sound throttling does not swallow a vehicle explosion", () => {
  // Exercise routing without creating browser audio nodes or loading assets.
  const audio = Object.create(AudioSystem.prototype) as AudioSystem;
  const played: string[] = [];
  audio.enabled = true;
  audio.lastExplosion =
    audio.lastDestruction =
    audio.lastShot =
    audio.lastHit =
    audio.lastLaser =
      -Infinity;
  audio.sounds = Object.fromEntries(
    ["wood-break", "rubble-break", "explosion"].map((name) => [
      name,
      {
        play: () => {
          played.push(name);
          return 1;
        },
        volume: () => {},
        stereo: () => {},
      },
    ]),
  ) as AudioSystem["sounds"];
  const listener = { x: 0, z: 0 };
  audio.event({ type: "destroy", coverKind: "tree", ...listener }, listener);
  audio.event({ type: "death", ...listener }, listener);
  assert.deepEqual(played, ["wood-break", "explosion"]);
  audio.lastExplosion = -Infinity;
  audio.event({ type: "destroy", coverKind: "drum", ...listener }, listener);
  audio.event({ type: "explosion", coverKind: "drum", ...listener }, listener);
  assert.deepEqual(played, ["wood-break", "explosion", "explosion"]);
});
