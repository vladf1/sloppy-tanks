import { Howl, Howler } from "howler";
import type { EventBatch, SoundName } from "./engine-api";

const sound = (name: SoundName, pool = 12) =>
  new Howl({
    src: [`${import.meta.env.BASE_URL}audio/${name}.mp3`],
    format: ["mp3"],
    pool,
    volume: 0.25,
  });

/** Plays the sounds the engine plans for each event batch (`crates/web/src/sounds.rs`:
 * which sound, how loud and where, rate-limited on the frame clock). */
export class AudioSystem {
  sounds = {
    shot: sound("shot", 16),
    "shot-spread": sound("shot-spread", 16),
    "shot-rocket": sound("shot-rocket", 16),
    "shot-ricochet": sound("shot-ricochet", 16),
    "shot-piercing": sound("shot-piercing", 16),
    explosion: sound("explosion"),
    "wood-break": sound("wood-break"),
    "rubble-break": sound("rubble-break"),
    impact: sound("impact"),
    pickup: sound("pickup"),
    hit: sound("hit"),
    laser: sound("laser"),
    promotion: sound("promotion", 2),
  } satisfies Record<SoundName, Howl>;
  enabled = false;
  start(): void {
    this.enabled = true;
    void Howler.ctx?.resume();
  }
  volume(v: number): void {
    Howler.volume(v);
  }
  /** Start one frame's planned sounds; nothing plays before `start`. */
  play(batch: EventBatch): void {
    if (!this.enabled) {
      return;
    }
    for (const { sound, gain, pan } of batch.sounds) {
      const effect = this.sounds[sound];
      const id = effect.play();
      effect.volume(gain, id);
      effect.stereo(pan, id);
    }
  }
}
