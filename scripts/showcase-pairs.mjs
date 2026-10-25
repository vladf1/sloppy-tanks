// Blind side-by-side pairs of two showcase captures (`showcase-capture.mjs`) for a judge
// who must not know which build is which. For every still both captures share, it writes
// `pair-NN.png` (left | right, labelled only LEFT and RIGHT) and the two full-size halves
// `pair-NN-left.png` / `pair-NN-right.png`; motion strips become 4×2 contact sheets.
// `--order` says per pair which capture goes left: `a` puts --a left, `b` puts --b left
// (cycled when shorter than the pair count). The key file lists which capture each side
// came from and belongs to whoever runs the comparison, not the judge.
//
//   node scripts/showcase-pairs.mjs --a dirA --b dirB --out dirPairs --key key.json \
//     [--order abba...] [--shots village-hero,quarry-action]
import fs from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { createCanvas, loadImage } from "@napi-rs/canvas";

const { values } = parseArgs({
  options: {
    a: { type: "string" },
    b: { type: "string" },
    out: { type: "string" },
    key: { type: "string" },
    order: { type: "string", default: "ab" },
    shots: { type: "string", default: "" },
  },
});
if (!values.a || !values.b || !values.out || !values.key) {
  throw new Error("Usage: --a dir --b dir --out dir --key file [--order ab...] [--shots ...]");
}
const GAP = 16;
const LABEL = 44;
fs.mkdirSync(values.out, { recursive: true });

const stills = (dir) =>
  new Set(
    fs
      .readdirSync(dir)
      .filter((file) => file.endsWith(".png"))
      .map((file) => file.slice(0, -4)),
  );
const inA = stills(values.a);
const inB = stills(values.b);
const wanted = new Set(values.shots.split(",").filter(Boolean));
const keep = (name) =>
  wanted.size === 0 || wanted.has(name) || wanted.has(name.replace(/-strip-\d$/, "-strip"));

// One entry per judged view: a still, or a map's eight strip frames as one sheet.
const views = [];
for (const name of [...inA].sort()) {
  if (!inB.has(name) || !keep(name)) continue;
  const strip = name.match(/^(.*)-strip-(\d)$/);
  if (strip) {
    if (strip[2] === "0") {
      views.push({
        name: `${strip[1]}-strip`,
        frames: [...Array(8).keys()].map((i) => `${strip[1]}-strip-${i}`),
      });
    }
  } else {
    views.push({ name, frames: [name] });
  }
}

async function sheet(dir, frames) {
  const images = await Promise.all(
    frames.map((frame) => loadImage(path.join(dir, `${frame}.png`))),
  );
  if (images.length === 1) return images[0];
  const columns = 4;
  const scale = 0.5;
  const w = images[0].width * scale;
  const h = images[0].height * scale;
  const canvas = createCanvas(w * columns, h * Math.ceil(images.length / columns));
  const context = canvas.getContext("2d");
  images.forEach((image, i) =>
    context.drawImage(image, (i % columns) * w, Math.floor(i / columns) * h, w, h),
  );
  return canvas;
}

const key = [];
for (const [index, view] of views.entries()) {
  const id = `pair-${String(index + 1).padStart(2, "0")}`;
  const aLeft = values.order[index % values.order.length] !== "b";
  const [left, right] = aLeft ? [values.a, values.b] : [values.b, values.a];
  const leftImage = await sheet(left, view.frames);
  const rightImage = await sheet(right, view.frames);
  for (const [side, image] of [
    ["left", leftImage],
    ["right", rightImage],
  ]) {
    const canvas = createCanvas(image.width, image.height);
    canvas.getContext("2d").drawImage(image, 0, 0);
    fs.writeFileSync(path.join(values.out, `${id}-${side}.png`), canvas.toBuffer("image/png"));
  }
  const width = leftImage.width + GAP + rightImage.width;
  const height = Math.max(leftImage.height, rightImage.height) + LABEL;
  const canvas = createCanvas(width, height);
  const context = canvas.getContext("2d");
  context.fillStyle = "#111";
  context.fillRect(0, 0, width, height);
  context.drawImage(leftImage, 0, LABEL);
  context.drawImage(rightImage, leftImage.width + GAP, LABEL);
  context.fillStyle = "#fff";
  context.font = "bold 30px sans-serif";
  context.fillText("LEFT", 12, 32);
  context.fillText("RIGHT", leftImage.width + GAP + 12, 32);
  fs.writeFileSync(path.join(values.out, `${id}.png`), canvas.toBuffer("image/png"));
  key.push({ pair: id, view: view.name, left: aLeft ? "a" : "b", right: aLeft ? "b" : "a" });
}
fs.mkdirSync(path.dirname(path.resolve(values.key)), { recursive: true });
fs.writeFileSync(values.key, JSON.stringify({ a: values.a, b: values.b, pairs: key }, null, 2));
// The judge's index names views but never the builds.
fs.writeFileSync(
  path.join(values.out, "index.json"),
  JSON.stringify(
    key.map(({ pair, view }) => ({ pair, view })),
    null,
    2,
  ),
);
console.log(`Wrote ${key.length} pairs to ${values.out}`);
