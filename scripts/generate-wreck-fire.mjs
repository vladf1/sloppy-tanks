// Bake two slices of rising hot gas into a seamless six-second flipbook.
// Red and green hold the back and front temperatures. Blue holds the soot
// carried by the front slice. Runtime blends the two slices through the smoke.
import { createCanvas } from "@napi-rs/canvas";
import { writeFileSync } from "node:fs";

const width = 96,
  height = 144;
const columns = 12,
  rows = 12,
  frames = columns * rows;
const cells = width * height;
let randomState = 731;
function random() {
  randomState = (Math.imul(randomState, 1664525) + 1013904223) >>> 0;
  return randomState / 4294967296;
}
function smooth(x) {
  const t = Math.min(1, Math.max(0, x));
  return t * t * (3 - 2 * t);
}

function bakeSlice() {
  let heat = new Float32Array(cells),
    next = new Float32Array(cells);
  let soot = new Float32Array(cells),
    nextSoot = new Float32Array(cells);
  const output = [],
    smoke = [];
  // Each release entrains cooler air on its way up. A pair of counter-rotating
  // eddies rolls its sides over, travels with it and broadens as it climbs.
  const pulses = [];
  for (let at = 0; at < frames; at += 1.8 + 4.1 * random()) {
    pulses.push({
      at,
      x: width * (0.32 + 0.36 * random()),
      radius: 2.2 + 5.0 * random(),
      duration: 3 + 10 * random(),
      heat: 0.74 + 0.74 * random(),
      lift: 2.1 + 1.5 * random(),
      spin: 2.0 + 3.8 * random(),
      asymmetry: 0.3 + 1.2 * random(),
      skew: random() - 0.5,
      drift: 1.6 * random() - 0.8,
    });
  }
  // Smaller eddies in the shear layer stretch and tear the edge of a release
  // while the larger ones carry its hot centre up the column.
  const shear = Array.from({ length: 245 }, () => ({
    at: random() * frames,
    x: width * (0.22 + 0.56 * random()),
    y: 2 + 75 * random() ** 1.8,
    radius: 1.8 + 3.9 * random(),
    speed: 1.5 + 3 * random(),
    spin: (random() < 0.5 ? -1 : 1) * (2.6 + 4.0 * random()),
    drift: random() * 1.6 - 0.8,
  }));
  // Warm up for one period so the first stored frame carries the eddies and
  // smoke released just before the last. No cross-fade at the wrap.
  for (let frame = -frames; frame < frames; frame++) {
    const wind = new Float32Array(cells);
    const up = new Float32Array(cells).fill(2.45);
    const sources = new Float32Array(width);
    for (const pulse of pulses) {
      const age = (((frame - pulse.at) % frames) + frames) % frames;
      if (age > 65) continue;
      if (age < pulse.duration) {
        const draft =
          0.96 +
          0.18 * Math.sin(((frame * 2 * Math.PI) / frames) * 2) +
          0.14 * Math.cos(((frame * 2 * Math.PI) / frames) * 5);
        const amount = draft * pulse.heat * Math.sin((Math.PI * age) / pulse.duration);
        for (let x = 0; x < width; x++) {
          const d = (x - pulse.x) / pulse.radius;
          sources[x] = Math.max(sources[x], amount * Math.exp(-d * d * 0.9));
        }
      }
      const cy = 3 + age * pulse.lift,
        r = pulse.radius + age * 0.09;
      const cx = pulse.x + age * pulse.drift;
      const inv = 1 / r,
        extent = 3.4 * r;
      const strength = pulse.spin * smooth(age / 4) * (1 - smooth((age - 22) / 40));
      for (let y = Math.max(0, Math.floor(cy - extent)); y < Math.min(height, cy + extent); y++) {
        const dy = (y - cy) * inv;
        if (Math.abs(dy) > 2.4) continue;
        for (let x = Math.max(0, Math.floor(cx - extent)); x < Math.min(width, cx + extent); x++) {
          const dx = (x - cx) * inv;
          const left = Math.exp(-((dx + 0.95) ** 2 + dy * dy)),
            right = pulse.asymmetry * Math.exp(-((dx - 0.95) ** 2 + (dy - pulse.skew) ** 2));
          const i = y * width + x;
          wind[i] += strength * dy * (right - left);
          up[i] += strength * ((dx + 0.95) * left - (dx - 0.95) * right);
        }
      }
    }
    for (const eddy of shear) {
      const age = (((frame - eddy.at) % frames) + frames) % frames;
      if (age > 24) continue;
      const cx = eddy.x + eddy.drift * age,
        cy = eddy.y + eddy.speed * age,
        r = eddy.radius;
      const spin = eddy.spin * Math.sin((Math.PI * age) / 24);
      for (let y = Math.max(0, Math.floor(cy - 2 * r)); y < Math.min(height, cy + 2 * r); y++) {
        for (let x = Math.max(0, Math.floor(cx - 2 * r)); x < Math.min(width, cx + 2 * r); x++) {
          const dx = (x - cx) / r,
            dy = (y - cy) / r,
            swirl = spin * Math.exp(-dx * dx - dy * dy);
          wind[y * width + x] -= dy * swirl;
          up[y * width + x] += dx * swirl;
        }
      }
    }
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const i = y * width + x;
        const sx = Math.max(0, Math.min(width - 1.001, x - wind[i]));
        const sy = Math.max(0, Math.min(height - 1.001, y - up[i] - heat[i] * 1.65));
        const x0 = Math.floor(sx),
          y0 = Math.floor(sy),
          dx = sx - x0,
          dy = sy - y0;
        const j = y0 * width + x0;
        function carried(field) {
          return (
            (1 - dy) * ((1 - dx) * field[j] + dx * field[j + 1]) +
            dy * ((1 - dx) * field[j + width] + dx * field[j + width + 1])
          );
        }
        // Air entering through the lower boundary carries fresh fuel only
        // where the source ignites. Clamping hot gas at row zero leaves
        // vertical bands burning long after a release has stopped.
        const temp = sy > 0 ? carried(heat) : 0;
        const gas = sy > 0 ? carried(soot) : 0;
        const base = 0.53 * Math.exp(-((x - width * 0.5) ** 2) / 178);
        const fuel = y < 7 ? Math.max(base, sources[x]) : 0;
        next[i] = Math.max(fuel, temp * 0.993 - 0.0095, 0);
        nextSoot[i] = Math.max(fuel * 0.65, gas * 0.979, 0);
      }
    }
    [heat, next] = [next, heat];
    [soot, nextSoot] = [nextSoot, soot];
    if (frame >= 0) {
      output.push(Uint8Array.from(heat, (v) => Math.round(Math.min(v / 1.25, 1) * 255)));
      smoke.push(Uint8Array.from(soot, (v) => Math.round(Math.min(v, 1) * 255)));
    }
  }
  return { heat: output, soot: smoke };
}

const back = bakeSlice(),
  front = bakeSlice();
const canvas = createCanvas(width * columns, height * rows);
const context = canvas.getContext("2d");
for (let f = 0; f < frames; f++) {
  const cell = context.createImageData(width, height);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const i = y * width + x,
        dest = ((height - 1 - y) * width + x) * 4;
      cell.data.set([back.heat[f][i], front.heat[f][i], front.soot[f][i], 255], dest);
    }
  context.putImageData(cell, (f % columns) * width, Math.floor(f / columns) * height);
}
writeFileSync(
  new URL("../public/textures/effects/wreck-fire.webp", import.meta.url),
  canvas.toBuffer("image/webp", 89),
);
