import { mkdir, writeFile } from "node:fs/promises";
import { Random } from "./asset-data";
import { encodePixelsWebp } from "./encode-webp";
import { fbm, lattice, mix, smooth } from "./texture-noise";

// The timber wall atlas (`timber_model.rs` `GRAIN_ROWS`, `END_CELLS`): three rows of
// flat-sawn plank faces, each tileable left to right so a long beam repeats it along
// its grain, over a row of four end-grain cells (rings around an off-centre pith,
// drying checks, saw marks). Colours are the wood's own; members tint them lightly.
// The plank faces are weathered like a yard's timber: sun-silvered patches, damp
// stains, drying checks along the grain, dents and fresh chips showing pale wood,
// and nail holes bleeding rust. The image also serves as the members' bump map.
const width = 1024;
const height = 1024;
const row = height / 4;
const data = new Uint8ClampedArray(width * height * 4);
const rng = new Random(90211);

const noise = lattice(1);
const fibre = lattice(2);
const blotch = lattice(3);
const weather = lattice(4);

// Silvered, sun-bleached wood, rust and the pale raw wood under a fresh chip.
const SILVER = [150, 141, 126];
const RUST = [112, 62, 34];
const RAW = [226, 196, 150];

// Earlywood, latewood and knot colours of seasoned pine (sRGB 0-255).
const EARLY = [208, 172, 128];
const LATE = [158, 112, 72];
const KNOT = [96, 62, 38];

function put(x: number, y: number, rgb: number[]) {
  const i = (y * width + x) * 4;
  data[i] = rgb[0];
  data[i + 1] = rgb[1];
  data[i + 2] = rgb[2];
  data[i + 3] = 255;
}

/** Ring brightness for a ring coordinate: soft earlywood, a sharper latewood band. */
function ring(r: number) {
  const f = r - Math.floor(r);
  return smooth(0.5, 0.8, f) * (1 - smooth(0.9, 1.0, f));
}

// Plank faces. A board sawn past the pith shows rings as long stripes that close
// into cathedral arches where the saw cut deeper; x wraps across the image.
for (let band = 0; band < 3; band++) {
  const top = band * row;
  const depth = rng.range(1.2, 2.2);
  const swing = rng.range(0.35, 0.7);
  // The pith lies just outside the board, so rings never close into bullseyes.
  const pith = rng.next() < 0.5 ? rng.range(-0.8, -0.25) : rng.range(1.25, 1.8);
  const rings = rng.range(11, 16);
  const phase = rng.range(0, Math.PI * 2);
  const knots = Array.from({ length: 1 + Math.floor(rng.next() * 2) }, () => ({
    x: rng.range(0, width),
    y: top + rng.range(row * 0.25, row * 0.75),
    size: rng.range(5, 9),
  }));
  const tint = rng.range(0.94, 1.04);
  // Long drying checks along the grain, dents and chips, and nail holes.
  const checks = Array.from({ length: 3 + Math.floor(rng.next() * 3) }, () => ({
    x: rng.range(0, width),
    y: top + rng.range(row * 0.12, row * 0.88),
    length: rng.range(60, 260),
    bend: rng.range(-6, 6),
  }));
  const dents = Array.from({ length: 14 }, () => ({
    x: rng.range(0, width),
    y: top + rng.range(8, row - 8),
    rx: rng.range(4, 16),
    ry: rng.range(2.5, 7),
    chip: rng.next() < 0.45,
  }));
  const nails = Array.from({ length: 6 }, (_, i) => ({
    x: (i + rng.range(0.2, 0.8)) * (width / 6),
    y: top + (rng.next() < 0.5 ? row * 0.3 : row * 0.7) + rng.range(-6, 6),
  }));
  const wrapped = (dx: number) => {
    const d = Math.abs(dx);
    return Math.min(d, width - d);
  };
  for (let py = 0; py < row; py++) {
    for (let x = 0; x < width; x++) {
      const y = top + py;
      const u = (x / width) * Math.PI * 2;
      // Across the board (0-1) and along it (wraps every image width).
      const across = py / row;
      const along = x / width;
      const warp = fbm(noise, along * 6, across * 3 + band * 7, 6) - 0.5;
      const cut = depth + swing * Math.sin(u + phase) + 0.2 * Math.sin(2 * u + phase * 1.7);
      const offset = across - pith + warp * 0.06;
      let r = Math.sqrt(offset * offset + cut * cut * 0.18) * rings;
      // Rings bend around each knot and darken toward its heart.
      let knotShade = 0;
      for (const k of knots) {
        let dx = Math.abs(x - k.x);
        dx = Math.min(dx, width - dx);
        const dy = y - k.y;
        const d = Math.sqrt(dx * dx * 0.18 + dy * dy) / k.size;
        r += 2.2 * Math.exp(-d * d * 0.35);
        knotShade = Math.max(knotShade, 1 - smooth(0.6, 1.25, d));
      }
      const late = ring(r);
      // Fine fibres run with the grain; broad blotches age the surface.
      const fibres =
        fibre(along * 260, across * 40 + band * 31, 260) -
        0.5 +
        (fibre(along * 40, across * 90 + band * 17, 40) - 0.5) * 0.8;
      const age = blotch(along * 5, across * 2 + band * 13, 5) - 0.5;
      const shade = tint * (1 + fibres * 0.13 + age * 0.12);
      // Sun-silvered patches, streaked with the grain, and darker damp stains.
      const silver = smooth(0.5, 0.72, fbm(weather, along * 16, across * 2 + band * 5, 16));
      const damp = smooth(0.58, 0.78, fbm(weather, along * 4 + 9, across * 1.5 + band * 3, 4));
      let check = 0;
      for (const c of checks) {
        const dx = wrapped(x - c.x);
        if (dx > c.length / 2) continue;
        const t = dx / (c.length / 2);
        const centre = c.y + c.bend * (1 - t * t);
        const half = 1.4 * (1 - t);
        check = Math.max(check, 1 - smooth(half * 0.3, half + 0.7, Math.abs(y - centre)));
      }
      let dent = 0;
      let chip = 0;
      for (const d of dents) {
        const ex = wrapped(x - d.x) / d.rx;
        const ey = (y - d.y) / d.ry;
        const r = Math.sqrt(ex * ex + ey * ey + (noise(x * 0.3, y * 0.3) - 0.5) * 0.5);
        if (r > 1.3) continue;
        if (d.chip) chip = Math.max(chip, 1 - smooth(0.8, 1.0, r));
        else dent = Math.max(dent, 1 - smooth(0.5, 1.1, r));
      }
      let nail = 0;
      let rust = 0;
      for (const n of nails) {
        const r = Math.hypot(wrapped(x - n.x), y - n.y);
        nail = Math.max(nail, 1 - smooth(1.6, 2.6, r));
        // Rust bleeds a little way along the grain.
        const bleed = Math.hypot(wrapped(x - n.x) * 0.35, y - n.y);
        rust = Math.max(rust, (1 - smooth(2.0, 9.0, bleed)) * 0.6);
      }
      const rgb = [0, 1, 2].map((c) => {
        const base = mix(EARLY[c], LATE[c], late * 0.55);
        let value = mix(base, KNOT[c], knotShade * 0.9) * shade;
        value = mix(value, SILVER[c] * (0.9 + late * 0.16), silver * 0.45);
        value *= 1 - damp * 0.16;
        value = mix(value, RUST[c], rust);
        value = mix(value, RAW[c], chip * 0.85);
        value *= 1 - dent * 0.18;
        value = mix(value, 46, Math.max(check * 0.8, nail * 0.9));
        return value;
      });
      put(x, y, rgb);
    }
  }
}

// End grain: four cells of rings around a pith outside or near the cell's corner,
// with radial drying checks and faint diagonal saw marks.
const cell = row;
for (let index = 0; index < 4; index++) {
  const left = index * cell;
  const top = 3 * row;
  const cx = rng.range(-0.25, 1.25) * cell;
  const cy = rng.range(-0.25, 1.25) * cell;
  const rings = rng.range(14, 20) / cell;
  const checks = Array.from({ length: 2 + Math.floor(rng.next() * 3) }, () => ({
    angle: rng.range(0, Math.PI * 2),
    reach: rng.range(0.25, 0.6) * cell,
  }));
  for (let py = 0; py < cell; py++) {
    for (let px = 0; px < cell; px++) {
      const dx = px - cx;
      const dy = py - cy;
      const angle = Math.atan2(dy, dx);
      const wobble =
        (fbm(noise, Math.cos(angle) * 2 + index * 5, Math.sin(angle) * 2, 0, 3) - 0.5) * 2.2;
      const r = Math.sqrt(dx * dx + dy * dy) * rings + wobble;
      const late = ring(r);
      let crack = 0;
      for (const check of checks) {
        // Distance to a radial line from the pith, shortened past its reach.
        const along = dx * Math.cos(check.angle) + dy * Math.sin(check.angle);
        const side = Math.abs(-dx * Math.sin(check.angle) + dy * Math.cos(check.angle));
        if (along > 0 && along < check.reach) {
          const widthAt = 1.6 * (1 - along / check.reach);
          crack = Math.max(crack, 1 - smooth(widthAt * 0.4, widthAt + 0.6, side));
        }
      }
      const saw = Math.sin((px + py) * 0.9 + noise(px * 0.05, py * 0.05) * 3) * 0.04;
      const grain = (fibre(px * 0.6 + index * 50, py * 0.6) - 0.5) * 0.12;
      const shade = 0.82 * (1 + saw + grain);
      const rgb = [0, 1, 2].map((c) =>
        mix(mix(EARLY[c], LATE[c], late * 0.7) * shade, 40, crack * 0.85),
      );
      put(left + px, top + py, rgb);
    }
  }
}

const folder = new URL("../assets/texture-sources/wood/", import.meta.url);
await mkdir(folder, { recursive: true });
await writeFile(new URL("timber.webp", folder), encodePixelsWebp(data, width, height));
