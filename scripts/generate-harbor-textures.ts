import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, mix, smooth } from "./texture-noise";

// Offline only: writes Harbor Havoc's container paint wear, a tile the engine
// multiplies by each container's color (`harbor_models.rs`). One tile is
// `CONTAINER_WEAR_TILE` (3 m) of wall across the full container height; the
// corrugation itself is modelled, so the tile carries only weathering: sun-faded
// paint, touched-up patches, dents catching the light, rust blooming along the
// roof and floor rails and running down from chips, grime climbing from the floor,
// and bare-metal scuffs.
// Tiles horizontally; rows run from the roof (top) to the floor (bottom).
//
//   node --import tsx scripts/generate-harbor-textures.ts

const SIZE = 512;
const output = fileURLToPath(new URL("../public/textures/harbor/", import.meta.url));

function encodeLossy(name: string, pixels: Uint8ClampedArray, quality: number) {
  const dir = mkdtempSync(join(tmpdir(), "harbor-textures-"));
  try {
    const pam = join(dir, "texture.pam");
    const header = `P7\nWIDTH ${SIZE}\nHEIGHT ${SIZE}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n`;
    writeFileSync(pam, Buffer.concat([Buffer.from(header, "ascii"), Buffer.from(pixels.buffer)]));
    const target = join(output, name);
    execFileSync("cwebp", [
      "-quiet",
      "-q",
      String(quality),
      "-m",
      "6",
      "-metadata",
      "none",
      pam,
      "-o",
      target,
    ]);
    console.log(target);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const RUST: [number, number, number] = [0.42, 0.2, 0.09];
const RUST_DARK: [number, number, number] = [0.2, 0.1, 0.05];

function containerWear() {
  const random = new Random(707);
  const coarse = lattice(11, true);
  const fine = lattice(12, true);
  const rustNoise = lattice(13, true);
  const grimeNoise = lattice(14, true);
  const pixels = new Float32Array(SIZE * SIZE * 3);
  const index = (x: number, y: number) =>
    ((((y % SIZE) + SIZE) % SIZE) * SIZE + (((x % SIZE) + SIZE) % SIZE)) * 3;
  const blend = (i: number, color: [number, number, number], weight: number) => {
    for (let c = 0; c < 3; c++)
      pixels[i + c] = mix(pixels[i + c], color[c], Math.min(1, Math.max(0, weight)));
  };
  for (let y = 0; y < SIZE; y++) {
    const v = y / SIZE; // 0 at the roof, 1 at the floor
    for (let x = 0; x < SIZE; x++) {
      const u = x / SIZE;
      const mottle = fbm(coarse, u * 4, v * 4, 4, 5);
      const speck = fine(u * 128, v * 128, 128);
      // Sun-faded towards the roof, paint mottled by years of weather.
      let shade = 0.82 * mix(0.86, 1.08, mottle) * mix(0.95, 1.04, speck) * mix(1.07, 0.97, v);
      // Grime rising from the floor rail, ragged at its edge, and dirt washed down
      // in vertical bands from the roof.
      const grimeEdge = 0.74 + 0.12 * fbm(grimeNoise, u * 8, 0.5, 8, 3);
      const grime = smooth(grimeEdge - 0.08, grimeEdge + 0.16, v);
      const wash =
        smooth(0.45, 0.8, fbm(grimeNoise, u * 16, v * 2 + 3, 16, 3)) * smooth(0.0, 0.5, v) * 0.35;
      const dirt = Math.min(1, grime * 0.8 + wash);
      let r = shade * mix(1, 0.62, dirt);
      let g = shade * mix(1, 0.57, dirt);
      let b = shade * mix(1, 0.5, dirt);
      // Rust blooms along the roof and floor rails.
      const edge = Math.max(smooth(0.07, 0.0, v), smooth(0.9, 1.0, v));
      const bloom = edge * smooth(0.4, 0.66, fbm(rustNoise, u * 12, v * 12, 12, 4));
      // Rust creeping up from the floor rail where water stands.
      const creep =
        smooth(0.8, 0.98, v) * smooth(0.5, 0.75, fbm(rustNoise, u * 20, v * 6 + 9, 20, 3));
      const rust = Math.min(1, bloom * 0.9 + creep * 0.7);
      r = mix(r, RUST[0], rust);
      g = mix(g, RUST[1], rust);
      b = mix(b, RUST[2], rust);
      const i = (y * SIZE + x) * 3;
      pixels[i] = r;
      pixels[i + 1] = g;
      pixels[i + 2] = b;
    }
  }
  // Touched-up patches: rectangles repainted a shade off the original.
  for (let p = 0; p < 7; p++) {
    const w = random.range(30, 120);
    const h = random.range(25, 110);
    const x0 = random.next() * SIZE;
    const y0 = random.range(0.1, 0.8) * SIZE;
    const tone = random.range(0.86, 1.12);
    for (let y = Math.floor(y0); y < Math.min(SIZE, y0 + h); y++) {
      for (let x = Math.floor(x0); x < x0 + w; x++) {
        const i = index(x, y);
        for (let c = 0; c < 3; c++) pixels[i + c] *= tone;
      }
    }
  }
  // Dents from forklift tines and twistlocks: a soft hollow, lit along its upper
  // rim and shaded along its lower one.
  for (let d = 0; d < 9; d++) {
    const cx = random.next() * SIZE;
    const cy = random.range(0.25, 0.92) * SIZE;
    const rx = random.range(14, 46);
    const ry = random.range(8, 26);
    const depth = random.range(0.07, 0.16);
    for (let dy = -Math.ceil(ry); dy <= Math.ceil(ry); dy++) {
      for (let dx = -Math.ceil(rx); dx <= Math.ceil(rx); dx++) {
        const r = Math.hypot(dx / rx, dy / ry);
        if (r > 1) continue;
        const falloff = smooth(1, 0.35, r);
        const tone = 1 + (dy < 0 ? 0.6 : -1) * depth * falloff * (Math.abs(dy) / ry);
        const i = index(Math.round(cx + dx), Math.round(cy + dy));
        for (let c = 0; c < 3; c++) pixels[i + c] *= tone;
      }
    }
  }
  // Chips through the paint that have rusted, each running down the wall.
  for (let s = 0; s < 44; s++) {
    // The first sixteen run down from the roof rail; the rest from chips in the wall.
    const fromRail = s < 16;
    const cx = random.next() * SIZE;
    const cy = (fromRail ? random.range(0.0, 0.05) : random.range(0.05, 0.8)) * SIZE;
    const radius = fromRail ? 0 : random.range(2, 7);
    const strength = random.range(0.5, 0.95);
    for (let dy = -Math.ceil(radius); dy <= Math.ceil(radius); dy++) {
      for (let dx = -Math.ceil(radius); dx <= Math.ceil(radius); dx++) {
        const d = Math.hypot(dx, dy) / radius;
        if (d > 1) continue;
        const ragged = rustNoise((cx + dx) * 0.3, (cy + dy) * 0.3, 0) * 0.5;
        const weight = smooth(1, 0.4, d + ragged * 0.4) * strength;
        blend(index(Math.round(cx + dx), Math.round(cy + dy)), d < 0.4 ? RUST_DARK : RUST, weight);
      }
    }
    const length = random.range(0.08, 0.55) * SIZE;
    const width = random.range(1.5, 5);
    for (let y = Math.floor(cy); y < Math.min(SIZE, cy + length); y++) {
      const along = (y - cy) / length;
      const fade = (1 - along) ** 1.2 * strength * 0.62 * (0.6 + 0.4 * fine(cx * 0.1, y * 0.05, 0));
      const wobble = Math.sin(y * 0.09 + s) * 1.2;
      for (let dx = -Math.ceil(width * 2); dx <= Math.ceil(width * 2); dx++) {
        const weight = fade * Math.exp(-((dx / width) ** 2));
        blend(index(Math.round(cx + wobble) + dx, y), RUST, weight);
      }
    }
  }
  // Scuffs: short pale scratches through the paint, mostly low on the wall where
  // forklifts and twistlocks rub.
  for (let s = 0; s < 55; s++) {
    const x0 = random.next() * SIZE;
    const y0 = (0.3 + 0.7 * Math.sqrt(random.next())) * SIZE;
    const angle = random.range(-0.4, 0.4) + (random.next() < 0.7 ? 0 : Math.PI / 2);
    const length = random.range(5, 34);
    const strength = random.range(0.12, 0.35);
    for (let t = 0; t < length; t++) {
      const x = Math.round(x0 + Math.cos(angle) * t);
      const y = Math.round(y0 + Math.sin(angle) * t);
      if (y < 0 || y >= SIZE) continue;
      blend(index(x, y), [1.05, 1.03, 1.0], strength);
      blend(index(x, y + 1), [0.35, 0.3, 0.27], strength * 0.4);
    }
  }
  const bytes = new Uint8ClampedArray(SIZE * SIZE * 4);
  for (let p = 0; p < SIZE * SIZE; p++) {
    for (let c = 0; c < 3; c++) {
      // Linear multiplier encoded as sRGB.
      const linear = Math.min(1, Math.max(0, pixels[p * 3 + c]));
      bytes[p * 4 + c] = Math.round(255 * linear ** (1 / 2.2));
    }
    bytes[p * 4 + 3] = 255;
  }
  encodeLossy("container.webp", bytes, 86);
}

containerWear();
