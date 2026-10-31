// The rendering budget of a showcase capture (`showcase-capture.mjs`) against a baseline
// capture of the same shots: every still both reports share may draw at most `--draws`
// times the baseline's main-pass draw calls and `--triangles` times its triangles (each
// with a small absolute allowance, so a near-empty view is not held to a ratio of a few
// draws). Phones and the Tesla in-car browser are the audience's weak end; a polish pass
// must stay inside this. Exits non-zero when any still is over.
//
//   bun scripts/showcase-budget.mjs --candidate dir/report.json \
//     --baseline artifacts/aaa/baseline/report.json [--baseline more/report.json]
import fs from "node:fs";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    candidate: { type: "string" },
    baseline: { type: "string", multiple: true, default: [] },
    draws: { type: "string", default: "1.15" },
    triangles: { type: "string", default: "1.3" },
  },
});
if (!values.candidate || values.baseline.length === 0) {
  throw new Error("Usage: --candidate report.json --baseline report.json [--baseline ...]");
}
const DRAW_RATIO = Number(values.draws);
const TRIANGLE_RATIO = Number(values.triangles);
const DRAW_ALLOWANCE = 10;
const TRIANGLE_ALLOWANCE = 20_000;

const stats = (file) =>
  new Map(
    JSON.parse(fs.readFileSync(file, "utf8"))
      .shots.filter((shot) => shot.stats)
      .map((shot) => [`${shot.map}-${shot.shot}`, shot.stats]),
  );
const baseline = new Map(values.baseline.flatMap((file) => [...stats(file)]));
const candidate = stats(values.candidate);

const rows = [];
for (const [view, now] of candidate) {
  const before = baseline.get(view);
  if (!before) continue;
  const drawLimit = before.drawCalls * DRAW_RATIO + DRAW_ALLOWANCE;
  const triangleLimit = before.triangles * TRIANGLE_RATIO + TRIANGLE_ALLOWANCE;
  rows.push({
    view,
    draws: `${before.drawCalls} -> ${now.drawCalls}`,
    triangles: `${Math.round(before.triangles / 1000)}k -> ${Math.round(now.triangles / 1000)}k`,
    over: now.drawCalls > drawLimit || now.triangles > triangleLimit,
  });
}
const over = rows.filter((row) => row.over);
for (const row of rows) {
  console.log(
    `${row.over ? "OVER" : "ok  "} ${row.view.padEnd(34)} draws ${row.draws.padEnd(12)} triangles ${row.triangles}`,
  );
}
console.log(
  `${over.length} of ${rows.length} stills over budget (draws ≤ ${DRAW_RATIO}× + ${DRAW_ALLOWANCE}, triangles ≤ ${TRIANGLE_RATIO}× + ${TRIANGLE_ALLOWANCE / 1000}k)`,
);
if (over.length) process.exitCode = 1;
