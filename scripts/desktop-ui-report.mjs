import assert from "node:assert/strict";
import { createHash, randomInt } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, relative, resolve } from "node:path";
import { createCanvas, loadImage } from "@napi-rs/canvas";

const [native, reference, destination] = process.argv.slice(2).map((path) => resolve(path));
assert.ok(destination, "Usage: node scripts/desktop-ui-report.mjs <native> <reference> <report>");
await mkdir(destination, { recursive: true });
const escape = (text) => text.replace(/[&<>"']/g, (character) => `&#${character.charCodeAt(0)};`);
const capture = JSON.parse(await readFile(resolve(reference, "capture.json"), "utf8"));
const pairs = [];
for (const name of (await readdir(native)).filter((name) => name.endsWith(".png")).sort()) {
  const webPath = resolve(reference, name);
  const nativePath = resolve(native, name);
  const [web, app] = await Promise.all([loadImage(webPath), loadImage(nativePath)]);
  assert.equal(web.width, app.width, `${name}: matching pixel widths`);
  assert.equal(web.height, app.height, `${name}: matching pixel heights`);
  const order = randomInt(2) ? [web, app] : [app, web];
  const width = Math.round(web.width / 2);
  const height = Math.round(web.height / 2);
  const canvas = createCanvas(width * 2, height + 36);
  const context = canvas.getContext("2d");
  context.fillStyle = "#091326";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = "#fff";
  context.font = "18px sans-serif";
  context.fillText("A", 12, 25);
  context.fillText("B", width + 12, 25);
  order.forEach((image, index) => context.drawImage(image, index * width, 36, width, height));
  const contact = `${basename(name, ".png")}-ab.png`;
  await writeFile(resolve(destination, contact), canvas.toBuffer("image/png"));
  const hash = async (path) =>
    createHash("sha256")
      .update(await readFile(path))
      .digest("hex");
  pairs.push({
    name: basename(name, ".png"),
    contact,
    a: order[0] === web ? "reference" : "native",
    native: relative(destination, nativePath),
    reference: relative(destination, webPath),
    nativeHash: await hash(nativePath),
    referenceHash: await hash(webPath),
  });
}
await writeFile(
  resolve(destination, "mapping.json"),
  JSON.stringify({ capture: capture.health, pairs }, null, 2),
);
await writeFile(
  resolve(destination, "index.html"),
  `<!doctype html><meta charset="utf-8">
<title>Desktop UI comparison</title>
<style>body{margin:24px;background:#091326;color:#e6eef8;font:16px system-ui}header{position:sticky;top:0;background:#091326;padding:12px 0}a{color:#8ed4ff}img{width:100%;height:auto}details{margin:20px 0;border-top:1px solid #425466}summary{cursor:pointer;padding:16px}p{max-width:90ch}</style>
<header><b>Desktop UI comparison · ${escape(capture.health.version)} / ${escape(capture.health.commit)}</b>
<p>Randomized A/B pairs. Judge similarity to the production reference; inspect layout, typography, colors, controls and content. Arena backgrounds, animation phases and native-only Fullscreen/Quit controls are documented platform differences. No numeric score certifies completion.</p></header>
${pairs.map((pair) => `<details open><summary>${escape(pair.name)}</summary><img src="${escape(pair.contact)}" alt="Unlabelled A/B comparison for ${escape(pair.name)}"><p><a href="${escape(pair.reference)}">Production</a> · <a href="${escape(pair.native)}">Native</a></p></details>`).join("\n")}`,
);
console.log(`Wrote ${pairs.length} A/B comparisons to ${destination}/index.html`);
