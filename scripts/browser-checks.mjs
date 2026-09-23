// Run the functional browser checks one after another against a running dev server
// (`npm run dev`; pass its URL in SLOPPY_URL). This is a manual gate for startup,
// menu, input and rendering changes, not CI: it needs Chrome and a GPU, and the
// performance workloads in scripts/README.md stay separate.
import { spawnSync } from "node:child_process";

const checks = [
  ["browser-check.mjs"],
  ["startup-check.mjs"],
  ["map-start-check.mjs"],
  ["driving-check.mjs"],
  ["ammunition-check.mjs", "--visual-only"],
  ["combat-feedback-check.mjs"],
  ["veterancy-check.mjs"],
  ["laser-defense-check.mjs"],
  ["projectile-visual-check.mjs"],
  ["touch-controls-check.mjs"],
  ["round-recap-check.mjs"],
  ["solo-survival-check.mjs"],
  ["bot-movement-browser.mjs"],
  ["render-bundles-check.mjs"],
  ["pickup-atlas-check.mjs"],
  ["cover-hit-check.mjs"],
  ["tree-check.mjs"],
  ["tower-check.mjs"],
  ["timber-walls-check.mjs"],
  ["debris-cleanup-check.mjs"],
  ["multiplayer-simulation-check.mjs"],
];

const failed = [];
for (const [script, ...args] of checks) {
  console.log(`\n=== ${script} ${args.join(" ")}`);
  const run = spawnSync(process.execPath, [`scripts/${script}`, ...args], { stdio: "inherit" });
  if (run.status !== 0) {
    failed.push(script);
  }
}
if (failed.length) {
  console.error(
    `\n${failed.length} of ${checks.length} browser checks failed: ${failed.join(", ")}`,
  );
  process.exit(1);
}
console.log(`\nAll ${checks.length} browser checks passed.`);
