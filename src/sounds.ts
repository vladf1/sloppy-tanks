/**
 * The game's thirteen sounds. Each is a formula, not a recording: a falling sine (or
 * the three rising notes of the promotion chime) mixed with seeded noise under a
 * squared fade-out, at 22,050 samples a second. The page makes them when it starts
 * (`index.html`) instead of downloading thirteen files, and hands the shell the same
 * 16-bit WAV files that `scripts/generate-audio.ts` encodes as the MP3s the native apps
 * embed, so both play the same samples (`tests/sounds.test.ts` pins them).
 */
const RATE = 22050;

/** A sound: its name, frequency in Hz (the chime's notes), duration in seconds, the
 * noise's share of the mix, and the noise's seed. */
type Sound = readonly [
  name: string,
  frequency: number | readonly number[],
  duration: number,
  noise: number,
  seed: number,
];

/** Distinct pitch, envelope and noise give each munition its own attack. */
export const SOUNDS: readonly Sound[] = [
  ["shot", 150, 0.14, 0.35, 150],
  ["explosion", 65, 0.7, 0.75, 65],
  ["impact", 600, 0.08, 0.55, 600],
  ["wood-break", 340, 0.28, 0.82, 341],
  ["rubble-break", 190, 0.4, 0.94, 191],
  ["pickup", 700, 0.3, 0.05, 700],
  ["shot-spread", 310, 0.11, 0.8, 310],
  ["shot-rocket", 72, 0.38, 0.48, 72],
  ["shot-ricochet", 880, 0.18, 0.2, 880],
  ["shot-piercing", 1250, 0.075, 0.6, 1250],
  ["hit", 1800, 0.045, 0.15, 1800],
  ["laser", 1600, 0.075, 0.05, 1600],
  ["promotion", [660, 830, 990], 0.48, 0, 660],
];

/** `sound` as a mono 16-bit WAV file. */
export function soundFile([, frequency, duration, noise, seed]: Sound): Uint8Array {
  let state = seed;
  const random = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296;
  const n = Math.floor(RATE * duration);
  const buffer = new ArrayBuffer(44 + n * 2);
  const v = new DataView(buffer);
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
  v.setUint32(24, RATE, true);
  v.setUint32(28, RATE * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  str(36, "data");
  v.setUint32(40, n * 2, true);
  for (let i = 0; i < n; i++) {
    const t = i / RATE;
    const env = Math.pow(1 - i / n, 2);
    let tone: number;
    if (typeof frequency === "number") {
      tone = Math.sin(t * frequency * Math.PI * 2 * (1 - t * 0.6));
    } else {
      // A short rising three-note chime.
      const noteLength = duration / frequency.length;
      const note = Math.min(frequency.length - 1, Math.floor(t / noteLength));
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
