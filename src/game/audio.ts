import { Howl, Howler } from "howler";
import type { SimEvent, Vec2 } from "./types";
function wave(frequency: number, duration: number, noise: number) {
  const rate = 22050,
    n = Math.floor(rate * duration),
    buffer = new ArrayBuffer(44 + n * 2),
    v = new DataView(buffer);
  const str = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i));
  };
  str(0, "RIFF");
  v.setUint32(4, 36 + n * 2, true);
  str(8, "WAVE");
  str(12, "fmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, rate, true);
  v.setUint32(28, rate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  str(36, "data");
  v.setUint32(40, n * 2, true);
  for (let i = 0; i < n; i++) {
    const t = i / rate,
      env = Math.pow(1 - i / n, 2);
    const sample =
      (Math.sin(t * frequency * Math.PI * 2 * (1 - t * 0.6)) * (1 - noise) +
        (Math.random() * 2 - 1) * noise) *
      env *
      0.7;
    v.setInt16(44 + i * 2, sample * 32767, true);
  }
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return "data:audio/wav;base64," + btoa(binary);
}
export class AudioSystem {
  sounds = {
    shot: new Howl({
      src: [wave(150, 0.14, 0.35)],
      format: ["wav"],
      volume: 0.3,
      pool: 16,
    }),
    explosion: new Howl({
      src: [wave(65, 0.7, 0.75)],
      format: ["wav"],
      volume: 0.5,
      pool: 12,
    }),
    impact: new Howl({
      src: [wave(600, 0.08, 0.55)],
      format: ["wav"],
      volume: 0.12,
      pool: 12,
    }),
    pickup: new Howl({
      src: [wave(700, 0.3, 0.05)],
      format: ["wav"],
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
