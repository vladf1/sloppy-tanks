import { contentVersion, serverBuild } from "./content-version.mjs";
import { readFile } from "node:fs/promises";
import { VPS_MULTIPLAYER_URL } from "./vps-host.mjs";

/** Does this checkout need a server redeploy? Compares what its builds would stamp
 * with what the live server reports: the protocol and content version decide whether
 * clients built from here can join, and the server build catches server-only changes
 * (server/, bundled dependencies, build settings) that leave clients compatible.
 * Exit 0: nothing to deploy; 1: redeploy needed; 2: the server did not answer.
 * SLOPPY_SERVER_URL checks another server, such as a local one. */
const HEALTH_TIMEOUT_MS = 10_000;
const endpoint = process.env.SLOPPY_SERVER_URL ?? VPS_MULTIPLAYER_URL;
const health = new URL("/health", endpoint.replace(/^ws/, "http"));
// The protocol version is a constant of the shared Rust core both builds compile.
const protocolSource = await readFile(
  new URL("../crates/core/src/net/protocol.rs", import.meta.url),
  "utf8",
);
const PROTOCOL_VERSION = Number(
  /pub const PROTOCOL_VERSION: u32 = (\d+);/.exec(protocolSource)?.[1],
);
if (!Number.isInteger(PROTOCOL_VERSION))
  throw new Error("PROTOCOL_VERSION not found in protocol.rs");
const local = {
  version: PROTOCOL_VERSION,
  contentVersion: await contentVersion(),
  serverBuild: await serverBuild(),
};
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
const describe = (side) =>
  `protocol ${side.version}, content ${side.contentVersion}, server build ${side.serverBuild ?? "unknown"}`;
console.log(`checkout ${describe(local)}`);
console.log(`live     ${describe(live)}`);
const compatible = live.version === local.version && live.contentVersion === local.contentVersion;
if (!compatible) {
  console.log(
    "Redeploy needed: once this is on main, run `pnpm run server:deploy` from main.\n" +
      "Until then, clients built from here are asked to reload and cannot join.",
  );
  process.exitCode = 1;
} else if (live.serverBuild !== local.serverBuild) {
  console.log(
    "Redeploy needed for server-only changes: once this is on main, run\n" +
      "`pnpm run server:deploy` from main. Clients built from here can already join.",
  );
  process.exitCode = 1;
} else {
  console.log("Up to date: no server redeploy needed.");
}
