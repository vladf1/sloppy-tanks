import { contentVersion } from "./content-version.mjs";
import { PROTOCOL_VERSION } from "../src/net/protocol.ts";
import { VPS_MULTIPLAYER_URL } from "./vps-host.mjs";

/** Does this checkout need a server redeploy? Compares the version and content hash its
 * builds would stamp with what the live server reports. Exit 0: clients built from
 * here can play on it; 1: redeploy needed; 2: the server did not answer. */
const HEALTH_TIMEOUT_MS = 10_000;
const health = new URL("/health", VPS_MULTIPLAYER_URL.replace(/^ws/, "http"));
const local = { version: PROTOCOL_VERSION, contentVersion: await contentVersion() };
let live;
try {
  const response = await fetch(health, {
    cache: "no-store",
    signal: AbortSignal.timeout(HEALTH_TIMEOUT_MS),
  });
  live = await response.json();
} catch (error) {
  console.error(`Could not read ${health}: ${error.message}`);
  process.exit(2);
}
console.log(`checkout protocol ${local.version}, content ${local.contentVersion}`);
console.log(`live     protocol ${live.version}, content ${live.contentVersion}`);
if (live.version === local.version && live.contentVersion === local.contentVersion) {
  console.log("Up to date: no server redeploy needed.");
} else {
  console.log(
    "Redeploy needed: once this is on main, run `pnpm run server:deploy` from main.\n" +
      "Until then, clients built from here are asked to reload and cannot join.",
  );
  process.exitCode = 1;
}
