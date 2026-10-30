// Run the functional browser checks one after another against a running page server
// (`pnpm run dev`, or `pnpm exec vite preview` of a build; pass its URL in SLOPPY_URL).
// This is a manual gate for startup, menu, input and rendering changes, not CI: it needs
// Chrome, Playwright's WebKit and a GPU, and the performance workloads in
// scripts/README.md stay separate. SLOPPY_WEBGL=1 plays every check on the WebGL2 build.
import { spawnSync } from "node:child_process";

const checks = [
  "shell-check.mjs",
  "round-check.mjs",
  "first-person-check.mjs",
  "input-check.mjs",
  "touch-check.mjs",
  "phone-check.mjs",
  "webgl-check.mjs",
  "fixtures-check.mjs",
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
