// Run the functional browser checks one after another against a running dev server
// (`pnpm run dev`; pass its URL in SLOPPY_URL). This is a manual gate for startup,
// menu, input and rendering changes, not CI: it needs Chrome, Playwright's WebKit and a GPU, and the
// performance workloads in scripts/README.md stay separate.
import { spawnSync } from "node:child_process";

const checks = [
  "browser-check.mjs",
  "first-person-check.mjs",
  "startup-check.mjs",
  "map-start-check.mjs",
  "hud-feedback-check.mjs",
  "touch-controls-check.mjs",
  "phone-check.mjs",
  "round-recap-check.mjs",
  "render-cameras-check.mjs",
  "destruction-check.mjs",
  "fixtures-check.mjs",
  "multiplayer-simulation-check.mjs",
  "webgl-check.mjs",
  "webkit-startup-check.mjs",
];

const failed = [];
for (const script of checks) {
  console.log(`\n=== ${script}`);
  const run = spawnSync(process.execPath, [`scripts/${script}`], { stdio: "inherit" });
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
