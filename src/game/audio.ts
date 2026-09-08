import { Howl, Howler } from "howler";
import type { SimEvent, Vec2, Weapon } from "./types";

const sound = (name: string, pool = 12) => new Howl({
  src: [`${import.meta.env.BASE_URL}audio/${name}.mp3`], format: ["mp3"], pool, volume: 0.25,
});
const shotSounds = {
  standard: "shot", spread: "shot-spread", rocket: "shot-rocket",
  ricochet: "shot-ricochet", piercing: "shot-piercing",
} as const satisfies Record<Weapon, string>;

export class AudioSystem {
  sounds = {
    shot: sound("shot", 16),
    "shot-spread": sound("shot-spread", 16),
    "shot-rocket": sound("shot-rocket", 16),
    "shot-ricochet": sound("shot-ricochet", 16),
    "shot-piercing": sound("shot-piercing", 16),
    explosion: sound("explosion"), impact: sound("impact"),
    pickup: sound("pickup"), hit: sound("hit"), laser: sound("laser"), promotion: sound("promotion", 2),
  };
  enabled = false;
  lastExplosion = -Infinity;
  lastShot = -Infinity;
  lastHit = -Infinity;
  lastLaser = -Infinity;
  start() {
    this.enabled = true;
    void Howler.ctx?.resume();
  }
  volume(v: number) {
    Howler.volume(v);
  }
  event(e: SimEvent, listener: Vec2, playerHit = false, playerEvent = false) {
    if (!this.enabled) return;
    const now = performance.now();
    // One quiet, centered tick even when several spread pellets land together.
    if (playerHit && now - this.lastHit >= 80) {
      this.lastHit = now;
      const id = this.sounds.hit.play();
      this.sounds.hit.volume(0.14, id);
      this.sounds.hit.stereo(0, id);
    }
    const d = Math.hypot(e.x - listener.x, e.z - listener.z);
    if (d > 38) return;
    const shot = e.type === "shot";
    const explosion = ["explosion", "death", "destroy"].includes(e.type);
    if (shot && !playerEvent && now - this.lastShot < 35) return;
    if (explosion && now - this.lastExplosion < 70) return;
    if (e.type === "laser" && now - this.lastLaser < 50) return;
    const key = shot ? shotSounds[e.weapon ?? "standard"]
      : explosion ? "explosion"
      : e.type === "pickup" ? "pickup"
      : e.type === "promotion" && playerEvent ? "promotion"
      : e.type === "laser" ? "laser"
      : e.type === "impact" || e.type === "ricochet" ? "impact" : null;
    if (!key) return;
    if (shot) this.lastShot = now;
    if (explosion) this.lastExplosion = now;
    if (e.type === "laser") this.lastLaser = now;
    const effect = this.sounds[key], id = effect.play();
    effect.volume((explosion ? 0.45 : shot ? 0.25 : e.type === "laser" ? 0.12 : 0.18) * Math.max(0.05, 1 - d / 40), id);
    effect.stereo(Math.max(-1, Math.min(1, (e.x - listener.x) / 25)), id);
  }
}
