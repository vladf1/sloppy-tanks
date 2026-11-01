// Run the functional browser checks one after another against a running page server
// (`bun run dev`, or `bun run preview` of a build; pass its URL in SLOPPY_URL).
// This is a manual gate for startup, menu, input and rendering changes, not CI: it needs
// Chrome, Playwright's WebKit and a GPU, and the performance workloads in
// scripts/README.md stay separate. SLOPPY_WEBGL=1 plays every check on the WebGL2 build.
// The room checks start their own game server, which `bun scripts/build-server.mjs`
// builds; without it they are skipped, with a note.
import { spawnSync } from "node:child_process";
import { hasServerBinary, serverBinary } from "./browser-helpers.mjs";

const checks = [
  { script: "shell-check.mjs" },
  { script: "round-check.mjs" },
  { script: "geometry-cache-check.mjs" },
  { script: "first-person-check.mjs" },
  { script: "input-check.mjs" },
  { script: "touch-check.mjs" },
  { script: "phone-check.mjs" },
  { script: "webgl-check.mjs" },
  { script: "firefox-check.mjs" },
  { script: "text-fields-check.mjs" },
  { script: "fixtures-check.mjs" },
  { script: "multiplayer-check.mjs", server: true },
  { script: "lifecycle-check.mjs", server: true },
  { script: "webkit-check.mjs" },
];

const failed = [];
const skipped = [];
for (const { script, server } of checks) {
  console.log(`\n=== ${script}`);
  if (server && !hasServerBinary()) {
    console.log(`skipped: no game server at ${serverBinary} (node scripts/build-server.mjs)`);
    skipped.push(script);
    continue;
  }
  const run = spawnSync(process.execPath, [`scripts/${script}`], { stdio: "inherit" });
  if (run.status !== 0) {
    failed.push(script);
  }
}
const ran = checks.length - skipped.length;
const skips = skipped.length ? ` (${skipped.length} skipped: ${skipped.join(", ")})` : "";
if (failed.length) {
  console.error(`\n${failed.length} of ${ran} browser checks failed: ${failed.join(", ")}${skips}`);
  process.exit(1);
}
console.log(`\nAll ${ran} browser checks passed${skips}.`);
