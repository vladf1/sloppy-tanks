import { Howl, Howler } from "howler";
import type { SimEvent, Vec2 } from "./types";
export class AudioSystem {
  sounds = {
    shot: new Howl({
      src: [`${import.meta.env.BASE_URL}audio/shot.mp3`],
      format: ["mp3"],
      volume: 0.3,
      pool: 16,
    }),
    explosion: new Howl({
      src: [`${import.meta.env.BASE_URL}audio/explosion.mp3`],
      format: ["mp3"],
      volume: 0.5,
      pool: 12,
    }),
    impact: new Howl({
      src: [`${import.meta.env.BASE_URL}audio/impact.mp3`],
      format: ["mp3"],
      volume: 0.12,
      pool: 12,
    }),
    pickup: new Howl({
      src: [`${import.meta.env.BASE_URL}audio/pickup.mp3`],
      format: ["mp3"],
      volume: 0.4,
    }),
  };
  enabled = false;
  lastExplosion = 0;
  lastShot = 0;
  start() {
    this.enabled = true;
    void Howler.ctx?.resume();
  }
  volume(v: number) {
    Howler.volume(v);
  }
  event(e: SimEvent, listener: Vec2) {
    if (!this.enabled) return;
    const d = Math.hypot(e.x - listener.x, e.z - listener.z);
    if (d > 38) return;
    const now = performance.now();
    if (e.type === "shot" && now - this.lastShot < 35) return;
    if (
      ["explosion", "death", "destroy"].includes(e.type) &&
      now - this.lastExplosion < 70
    )
      return;
    const key =
      e.type === "shot"
        ? "shot"
        : ["explosion", "death", "destroy"].includes(e.type)
          ? "explosion"
          : e.type === "pickup"
            ? "pickup"
            : e.type === "impact" || e.type === "ricochet"
              ? "impact"
              : null;
    if (!key) return;
    if (key === "shot") this.lastShot = now;
    if (key === "explosion") this.lastExplosion = now;
    const sound = this.sounds[key],
      id = sound.play();
    sound.volume(
      (key === "explosion" ? 0.45 : key === "shot" ? 0.25 : 0.18) *
        Math.max(0.05, 1 - d / 40),
      id,
    );
    sound.stereo(Math.max(-1, Math.min(1, (e.x - listener.x) / 25)), id);
  }
}
