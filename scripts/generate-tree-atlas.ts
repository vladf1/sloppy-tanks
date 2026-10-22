import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

// One texture for every tree surface, so a tree's limbs, foliage and cut wood draw
// in one material: the runtime tree surfaces side by side, each in its own column
// (`TREE_ATLAS` in crates/core/src/models/tree_models.rs). The 256 px bark, birch
// and growth rings stack twice to fill their 512 px tall columns; the 512 px
// foliage cards keep their size. Pixels are copied raw, so the colour the foliage
// keeps under its transparent texels survives (a canvas would premultiply it
// away). Run after any of the source surfaces changes.
const trees = new URL("../public/textures/trees/", import.meta.url);
const columns: [string, number][] = [
  ["bark", 256],
  ["birch", 256],
  ["rings", 256],
  ["leaf-sprigs", 512],
  ["conifer-spray", 512],
];
const height = 512;
const width = columns.reduce((total, [, w]) => total + w, 0);
const atlas = new Uint8ClampedArray(width * height * 4);

/** A WebP's raw RGBA pixels, through `dwebp`'s PAM output. */
function decode(url: URL): { pixels: Buffer; width: number; height: number } {
  const pam = execFileSync("dwebp", ["-quiet", "-pam", fileURLToPath(url), "-o", "-"], {
    maxBuffer: 32 * 1024 * 1024,
  });
  const end = pam.indexOf("ENDHDR\n") + 7;
  const header = pam.subarray(0, end).toString("ascii");
  const size = (key: string) => Number(new RegExp(`${key} (\\d+)`).exec(header)![1]);
  return { pixels: pam.subarray(end), width: size("WIDTH"), height: size("HEIGHT") };
}

let left = 0;
for (const [name, w] of columns) {
  const image = decode(new URL(`${name}.webp`, trees));
  if (image.width !== w) throw new Error(`${name}.webp is ${image.width} px wide, not ${w}`);
  for (let y = 0; y < height; y++) {
    const row = (y % image.height) * image.width * 4;
    atlas.set(image.pixels.subarray(row, row + w * 4), (y * width + left) * 4);
  }
  left += w;
}
// Lossy like the bark it copies (the sources are already lossy, so a lossless copy
// would only store their artefacts at twice the size); `-exact` keeps the colour
// under the cut-out texels.
const header = `P7\nWIDTH ${width}\nHEIGHT ${height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n`;
const webp = execFileSync(
  "cwebp",
  ["-quiet", "-q", "86", "-m", "6", "-exact", "-alpha_q", "100", "-o", "-", "--", "-"],
  {
    input: Buffer.concat([Buffer.from(header, "ascii"), Buffer.from(atlas.buffer)]),
    maxBuffer: 32 * 1024 * 1024,
  },
);
await writeFile(new URL("atlas.webp", trees), webp);
