import { mkdir, stat } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Offline only: preserve the original sound envelopes with reproducible noise.
function wave(
  frequency: number | readonly number[],
  duration: number,
  noise: number,
  seed: number,
) {
  let state = seed;
  const random = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296;
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
    let tone: number;
    if (typeof frequency === "number") tone = Math.sin(t * frequency * Math.PI * 2 * (1 - t * 0.6));
    else {
      // A short rising three-note promotion chime, baked into the saved MP3.
      const noteLength = duration / frequency.length,
        note = Math.min(frequency.length - 1, Math.floor(t / noteLength));
      const local = t - note * noteLength;
      tone =
        Math.sin(local * frequency[note] * Math.PI * 2) *
        Math.min(1, local / 0.005) *
        (1 - local / noteLength);
    }
    const sample = (tone * (1 - noise) + (random() * 2 - 1) * noise) * env * 0.7;
    v.setInt16(44 + i * 2, sample * 32767, true);
  }
  return new Uint8Array(buffer);
}

const output = new URL("../public/audio/", import.meta.url);
await mkdir(output, { recursive: true });
for (const [name, frequency, duration, noise, seed] of [
  ["shot", 150, 0.14, 0.35, 150],
  ["explosion", 65, 0.7, 0.75, 65],
  ["impact", 600, 0.08, 0.55, 600],
  ["pickup", 700, 0.3, 0.05, 700],
  // Distinct pitch, envelope and noise give each munition its own attack.
  ["shot-spread", 310, 0.11, 0.8, 310],
  ["shot-rocket", 72, 0.38, 0.48, 72],
  ["shot-ricochet", 880, 0.18, 0.2, 880],
  ["shot-piercing", 1250, 0.075, 0.6, 1250],
  ["hit", 1800, 0.045, 0.15, 1800],
  ["laser", 1600, 0.075, 0.05, 1600],
  ["promotion", [660, 830, 990], 0.48, 0, 660],
] as const) {
  const data = wave(frequency, duration, noise, seed);
  const target = new URL(`${name}.mp3`, output);
  const result = spawnSync(
    "ffmpeg",
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "wav",
      "-i",
      "pipe:0",
      "-c:a",
      "libmp3lame",
      "-q:a",
      "2",
      "-map_metadata",
      "-1",
      "-id3v2_version",
      "0",
      "-write_id3v1",
      "0",
      fileURLToPath(target),
    ],
    { input: data },
  );
  if (result.error || result.status !== 0)
    throw new Error(
      `Offline audio encoding requires FFmpeg with libmp3lame: ${result.error?.message ?? result.stderr.toString()}`,
    );
  console.log(`${name}.mp3: ${(await stat(target)).size} bytes`);
}
