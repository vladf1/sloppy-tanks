import { Howl, Howler } from "howler";
import type { SimEvent, Vec2, Weapon } from "./types";

const AUDIO = {
  hitIntervalMs: 80,
  botShotIntervalMs: 35,
  explosionIntervalMs: 70,
  laserIntervalMs: 50,
  audibleDistance: 38,
  fadeDistance: 40,
  stereoDistance: 25,
} as const;

const sound = (name: string, pool = 12) =>
  new Howl({
    src: [`${import.meta.env.BASE_URL}audio/${name}.mp3`],
    format: ["mp3"],
    pool,
    volume: 0.25,
  });
const shotSounds = {
  standard: "shot",
  spread: "shot-spread",
  rocket: "shot-rocket",
  ricochet: "shot-ricochet",
  piercing: "shot-piercing",
} as const satisfies Record<Weapon, string>;

export class AudioSystem {
  sounds = {
    shot: sound("shot", 16),
    "shot-spread": sound("shot-spread", 16),
    "shot-rocket": sound("shot-rocket", 16),
    "shot-ricochet": sound("shot-ricochet", 16),
    "shot-piercing": sound("shot-piercing", 16),
    explosion: sound("explosion"),
    impact: sound("impact"),
    pickup: sound("pickup"),
    hit: sound("hit"),
    laser: sound("laser"),
    promotion: sound("promotion", 2),
  };
  enabled = false;
  lastExplosion = -Infinity;
  lastShot = -Infinity;
  lastHit = -Infinity;
  lastLaser = -Infinity;
  start(): void {
    this.enabled = true;
    void Howler.ctx?.resume();
  }
  volume(v: number): void {
    Howler.volume(v);
  }
  event(event: SimEvent, listener: Vec2, playerHit = false, playerEvent = false): void {
    if (!this.enabled) {
      return;
    }
    const now = performance.now();
    // One quiet, centered tick even when several spread pellets land together.
    if (playerHit && now - this.lastHit >= AUDIO.hitIntervalMs) {
      this.lastHit = now;
      const id = this.sounds.hit.play();
      this.sounds.hit.volume(0.14, id);
      this.sounds.hit.stereo(0, id);
    }
    const d = Math.hypot(event.x - listener.x, event.z - listener.z);
    if (d > AUDIO.audibleDistance) {
      return;
    }
    const shot = event.type === "shot";
    const explosion = ["explosion", "death", "destroy"].includes(event.type);
    if (shot && !playerEvent && now - this.lastShot < AUDIO.botShotIntervalMs) {
      return;
    }
    if (explosion && now - this.lastExplosion < AUDIO.explosionIntervalMs) {
      return;
    }
    if (event.type === "laser" && now - this.lastLaser < AUDIO.laserIntervalMs) {
      return;
    }
    const key = shot
      ? shotSounds[event.weapon ?? "standard"]
      : explosion
        ? "explosion"
        : event.type === "pickup"
          ? "pickup"
          : event.type === "promotion" && playerEvent
            ? "promotion"
            : event.type === "laser"
              ? "laser"
              : event.type === "impact" || event.type === "ricochet"
                ? "impact"
                : null;
    if (!key) {
      return;
    }
    if (shot) {
      this.lastShot = now;
    }
    if (explosion) {
      this.lastExplosion = now;
    }
    if (event.type === "laser") {
      this.lastLaser = now;
    }
    const effect = this.sounds[key];
    const id = effect.play();
    effect.volume(
      (explosion ? 0.45 : shot ? 0.25 : event.type === "laser" ? 0.12 : 0.18) *
        Math.max(0.05, 1 - d / AUDIO.fadeDistance),
      id,
    );
    effect.stereo(Math.max(-1, Math.min(1, (event.x - listener.x) / AUDIO.stereoDistance)), id);
  }
}
