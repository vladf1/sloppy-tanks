import { test } from "node:test";
import assert from "node:assert/strict";
import { AudioSystem } from "../src/game/audio";
import type { EventBatch } from "../src/game/engine-api";

// The engine chooses, rate-limits and places the sounds (`crates/web/src/sounds.rs`
// tests); the page only starts them.
test("the page plays the engine's planned sounds in order, with their gain and pan, once started", () => {
  // Exercise playback without creating browser audio nodes or loading assets.
  const audio = Object.create(AudioSystem.prototype) as AudioSystem;
  const played: [string, number, number][] = [];
  audio.enabled = false;
  audio.sounds = Object.fromEntries(
    ["hit", "explosion"].map((name) => {
      let id = 0;
      const voices = new Map<number, [string, number, number]>();
      return [
        name,
        {
          play: () => {
            const voice: [string, number, number] = [name, NaN, NaN];
            voices.set(++id, voice);
            played.push(voice);
            return id;
          },
          volume: (gain: number, voice: number) => {
            voices.get(voice)![1] = gain;
          },
          stereo: (pan: number, voice: number) => {
            voices.get(voice)![2] = pan;
          },
        },
      ];
    }),
  ) as unknown as AudioSystem["sounds"];
  const batch: EventBatch = {
    listener: { x: 0, z: 0 },
    listenerRight: { x: 1, z: 0 },
    events: [],
    sounds: [
      { sound: "hit", gain: 0.14, pan: 0 },
      { sound: "explosion", gain: 0.3, pan: -0.5 },
    ],
  };
  audio.play(batch);
  assert.deepEqual(played, [], "silent until the first gesture starts audio");
  audio.enabled = true;
  audio.play(batch);
  assert.deepEqual(played, [
    ["hit", 0.14, 0],
    ["explosion", 0.3, -0.5],
  ]);
});
