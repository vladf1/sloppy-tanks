import { contentVersion, protocolVersion, serverBuild } from "./content-version.mjs";
import { buildServerSite } from "./server-site.mjs";
import { gameServer } from "./servers.mjs";

/** Does this checkout need a server redeploy? Compares what its builds would stamp
 * with what the live server reports: the protocol and content version decide whether
 * clients built from here can join, and the server build catches server-only changes
 * (crates/server/, bundled dependencies, build settings) that leave clients compatible.
 * `--site` also builds the Wasm engines and the server's copy of the page
 * (scripts/server-site.mjs) and compares its site build, which only a build can tell.
 * Exit 0: nothing to deploy; 1: redeploy needed; 2: the server did not answer.
 * It also reports whether the live page that uses this server can join it; a page that
 * cannot is a warning, since the page deploy may simply not have caught up yet.
 * `--dev` checks the dev site and its server; SLOPPY_SERVER_URL checks another server,
 * such as a local one, and SLOPPY_PAGE_URL the page beside it. */
const HEALTH_TIMEOUT_MS = 10_000;
const PRODUCTION_PAGE_URL = "https://sloppy-tanks.fridman.me/";
const DEV_PAGE_URL = "https://sloppy-tanks-dev.fridman.me/";
const dev = process.argv.includes("--dev");
const compareSite = process.argv.includes("--site");
// The address players use, as they reach it.
const endpoint = process.env.SLOPPY_SERVER_URL ?? gameServer(dev).url;
const pageUrl =
  process.env.SLOPPY_PAGE_URL ??
  (process.env.SLOPPY_SERVER_URL ? undefined : dev ? DEV_PAGE_URL : PRODUCTION_PAGE_URL);

async function readHealth(url) {
  const response = await fetch(url, {
    cache: "no-store",
    signal: AbortSignal.timeout(HEALTH_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

const health = new URL("/health", endpoint.replace(/^ws/, "http"));
const local = {
  protocol: await protocolVersion(),
  contentVersion: await contentVersion(),
  serverBuild: await serverBuild(),
  siteBuild: compareSite ? await buildServerSite({ buildWasm: true }) : undefined,
};
let live;
try {
  live = await readHealth(health);
} catch (error) {
  console.error(`Could not read ${health}: ${error.message}`);
  process.exit(2);
}
const describe = (side) =>
  (side.version ? `v${side.version}, ` : "") +
  `protocol ${side.protocol}, content ${side.contentVersion}, server build ${side.serverBuild}` +
  (side.siteBuild ? `, site ${side.siteBuild}` : "") +
  (side.commit ? `, commit ${side.commit}${side.dirty ? " (local changes)" : ""}` : "") +
  (side.builtAt ? `, built ${side.builtAt}` : "");
console.log(`checkout ${describe(local)}`);
console.log(`live     ${describe(live)}`);
if (pageUrl) {
  const pageHealth = new URL("health/", pageUrl);
  try {
    const page = await readHealth(pageHealth);
    console.log(
      `page     v${page.version}, protocol ${page.protocol}, content ${page.contentVersion}, commit ${page.commit}${page.dirty ? " (local changes)" : ""}, built ${page.builtAt}`,
    );
    if (page.protocol !== live.protocol || page.contentVersion !== live.contentVersion) {
      console.log(`Warning: ${pageUrl} cannot join this server's rooms until both match.`);
    }
  } catch (error) {
    console.log(`page     unknown: could not read ${pageHealth}: ${error.message}`);
  }
}
const compatible = live.protocol === local.protocol && live.contentVersion === local.contentVersion;
if (!compatible) {
  console.log(
    "Redeploy needed: once this is on main and CI has promoted its image, run\n" +
      "`pnpm run server:update` (auto-update does it by itself; `server:deploy` from main\n" +
      "is the SSH fallback). Until then, clients built from here are asked to reload.",
  );
  process.exitCode = 1;
} else if (live.serverBuild !== local.serverBuild) {
  console.log(
    "Redeploy needed for server-only changes: once this is on main and CI has promoted\n" +
      "its image, run `pnpm run server:update` (or let auto-update). Clients built from\n" +
      "here can already join.",
  );
  process.exitCode = 1;
} else if (compareSite && live.siteBuild !== local.siteBuild) {
  console.log(
    "Redeploy needed for the page the server serves: once this is on main and CI has\n" +
      "promoted its image, run `pnpm run server:update` (or let auto-update).",
  );
  process.exitCode = 1;
} else {
  console.log(
    compareSite
      ? "Up to date: no server redeploy needed."
      : "Up to date: no server redeploy needed (--site also compares the page it serves).",
  );
}
