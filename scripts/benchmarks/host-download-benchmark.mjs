import { execFile, execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const RUNS = 13;
const MAX_TIME_SECONDS = 60;
const ACCEPT_ENCODING = "br, gzip";
/** Every curl request's transport: HTTP/2 over IPv4, failing loudly. */
const CURL = ["--fail", "--silent", "--show-error", "--ipv4", "--http2"];
const deployments = [
  {
    id: "github-pages",
    name: "GitHub Pages",
    origin: "https://sloppy-tanks.fridman.me",
  },
  {
    id: "cloudflare-workers",
    name: "Cloudflare Workers (dev site)",
    origin: "https://sloppy-tanks-dev.fridman.me",
  },
];

/** The textures the page fetches for the village, the default map; the sounds, tank
 * previews and fonts are inside the shell's Wasm. */
const texturePaths = [
  "textures/barrels/painted-drum.webp",
  "textures/ground/dry-grass.webp",
  "textures/ground/packed-dirt.webp",
  "textures/ground/village-ground.webp",
  "textures/houses/brick.webp",
  "textures/houses/clapboard.webp",
  "textures/houses/shingles.webp",
  "textures/houses/siding.webp",
  "textures/houses/stone.webp",
  "textures/pickups/atlas.webp",
  "textures/tanks/armor-camo.webp",
  "textures/tanks/armor-scorch.webp",
  "textures/tanks/armor-wear.webp",
  "textures/trees/atlas.webp",
  "textures/trees/bark.webp",
  "textures/trees/birch.webp",
  "textures/trees/rings.webp",
  "textures/walls/weathered-concrete.webp",
  "textures/water/normals.webp",
  "textures/wood/timber.webp",
];
/** The page's two builds of the egui shell: `index.html` imports one by the browser's
 * graphics, and its glue fetches its Wasm. A production build names them from the site's
 * base (`assets/…`), the dev site's build from the importing file (`./…`). */
const backends = [
  { id: "webgpu", glue: /(?:\.\/|assets\/)sloppy-tanks-(?!webgl-)[\w-]+\.js/, wasm: /(?:\.\/|assets\/)sloppy-tanks_bg-[\w-]+\.wasm/ },
  { id: "webgl", glue: /(?:\.\/|assets\/)sloppy-tanks-webgl-[\w-]+\.js/, wasm: /(?:\.\/|assets\/)sloppy-tanks-webgl_bg-[\w-]+\.wasm/ },
];

function curlText(url) {
  return execFileSync(
    "curl",
    [...CURL, "--compressed", "--max-time", "20", url],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
  );
}

function urlFor(deployment, path) {
  return `${deployment.origin}/${path}`;
}

/** The first match of `pattern` in the file at `from`, as a path from the site's root;
 * a `./` path is relative to `from`. */
function found(text, pattern, what, deployment, from) {
  const match = text.match(pattern);
  if (!match) {
    throw new Error(`Could not find ${what} in ${urlFor(deployment, from)}`);
  }
  return match[0].startsWith("./")
    ? new URL(match[0], urlFor(deployment, from)).pathname.slice(1)
    : match[0].replace(/^\/+/, "");
}

/** What one page load fetches on each backend: the HTML, its module, the backend's
 * shell glue and Wasm, the favicon and the village's textures. */
function discover(deployment) {
  const entry = urlFor(deployment, "");
  const html = curlText(entry);
  const main = found(html, /[^"']*assets\/main-[\w-]+\.js/, "the page's module", deployment, "");
  const mainText = curlText(urlFor(deployment, main));
  const shared = [
    { path: "", type: "html", label: "HTML entry" },
    { path: main, type: "code", label: "Page module" },
    { path: "favicon.svg", type: "startup", label: "Favicon" },
    ...texturePaths.map((path) => ({ path, type: "texture", label: path })),
  ];
  const loads = Object.fromEntries(
    backends.map((backend) => {
      const glue = found(mainText, backend.glue, `the ${backend.id} shell glue`, deployment, main);
      const glueText = curlText(urlFor(deployment, glue));
      const wasm = found(glueText, backend.wasm, `the ${backend.id} Wasm`, deployment, glue);
      return [
        backend.id,
        [
          ...shared,
          { path: glue, type: "code", label: `Shell glue (${backend.id})` },
          { path: wasm, type: "wasm", label: `Shell Wasm (${backend.id})` },
        ],
      ];
    }),
  );
  return { htmlBytes: Buffer.byteLength(html), loads };
}

function requestResource(url, resource) {
  return new Promise((resolve) => {
    const started = performance.now();
    const args = [
      ...CURL,
      "--raw",
      "--location",
      "--max-time",
      String(MAX_TIME_SECONDS),
      "--header",
      `Accept-Encoding: ${ACCEPT_ENCODING}`,
      "--output",
      "/dev/null",
      "--write-out",
      '{"http_code":"%{http_code}","http_version":"%{http_version}","remote_ip":"%{remote_ip}","time_namelookup":%{time_namelookup},"time_connect":%{time_connect},"time_appconnect":%{time_appconnect},"time_starttransfer":%{time_starttransfer},"time_total":%{time_total},"size_download":%{size_download},"speed_download":%{speed_download},"num_connects":%{num_connects},"num_redirects":%{num_redirects}}',
      url,
    ];
    execFile("curl", args, { maxBuffer: 1024 * 1024 }, (error, stdout, stderr) => {
      const base = { ...resource, url, processWallMs: performance.now() - started };
      if (error) {
        resolve({ ...base, ok: false, error: error.message, stderr: stderr.trim() });
        return;
      }
      try {
        const metrics = JSON.parse(stdout.trim().split("\n").at(-1));
        resolve({ ...base, ok: metrics.http_code === "200", ...metrics });
      } catch (parseError) {
        resolve({
          ...base,
          ok: false,
          error: `Invalid curl metrics: ${parseError.message}`,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
        });
      }
    });
  });
}

async function identityBytes(url) {
  const { stdout } = await execFileAsync(
    "curl",
    [
      ...CURL,
      "--max-time",
      "20",
      "--header",
      "Accept-Encoding: identity",
      "--output",
      "/dev/null",
      "--write-out",
      "%{size_download}",
      url,
    ],
    { maxBuffer: 1024 * 1024 },
  );
  return Number(stdout.trim());
}

function percentile(values, p) {
  const sorted = values.filter(Number.isFinite).toSorted((a, b) => a - b);
  if (!sorted.length) return null;
  const rank = (sorted.length - 1) * p;
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower);
}

function summary(values, digits = 2) {
  const rounded = (value) => (value == null ? null : Number(value.toFixed(digits)));
  return {
    count: values.filter(Number.isFinite).length,
    p25: rounded(percentile(values, 0.25)),
    p50: rounded(percentile(values, 0.5)),
    p75: rounded(percentile(values, 0.75)),
    min: rounded(Math.min(...values)),
    max: rounded(Math.max(...values)),
  };
}

function sum(values) {
  return values.reduce((total, value) => total + (Number.isFinite(value) ? value : 0), 0);
}

function summarizeBatches(batches) {
  const okBatches = batches.filter((batch) => batch.ok);
  const bytes = okBatches.map((batch) => batch.encodedBytes);
  const decodedBytes = okBatches.map((batch) => batch.identityBytes);
  const wall = okBatches.map((batch) => batch.wallMs);
  const throughput = okBatches.map((batch) => (batch.encodedBytes / batch.wallMs) * 1000);
  const sumResourceTime = okBatches.map((batch) => sum(batch.resources.map((resource) => resource.time_total * 1000)));
  const maxTtfb = okBatches.map((batch) => Math.max(...batch.resources.map((resource) => resource.time_starttransfer * 1000)));
  const maxResource = okBatches.map((batch) => Math.max(...batch.resources.map((resource) => resource.time_total * 1000)));
  const allResources = okBatches.flatMap((batch) => batch.resources);
  const byPath = new Map();
  for (const resource of allResources) {
    const list = byPath.get(resource.path) ?? [];
    list.push(resource);
    byPath.set(resource.path, list);
  }
  const resourceSummaries = [...byPath.entries()].map(([path, resources]) => ({
    path,
    type: resources[0].type,
    encodedBytes: summary(resources.map((resource) => Number(resource.size_download)), 0),
    ttfbMs: summary(resources.map((resource) => Number(resource.time_starttransfer) * 1000), 2),
    totalMs: summary(resources.map((resource) => Number(resource.time_total) * 1000), 2),
  }));
  resourceSummaries.sort((a, b) => b.totalMs.p50 - a.totalMs.p50);
  return {
    runs: okBatches.length,
    failedRuns: batches.length - okBatches.length,
    resourcesPerRun: okBatches[0]?.resources.length ?? 0,
    encodedBytes: summary(bytes, 0),
    identityBytes: summary(decodedBytes, 0),
    batchWallMs: summary(wall, 2),
    aggregateEncodedThroughputBytesPerSecond: summary(throughput, 0),
    sumOfResourceDownloadMs: summary(sumResourceTime, 2),
    slowestResourceTtfbMsPerRun: summary(maxTtfb, 2),
    slowestResourceTotalMsPerRun: summary(maxResource, 2),
    resourceSummaries,
  };
}

async function main() {
  const startedAt = new Date().toISOString();
  const discoveries = Object.fromEntries(deployments.map((deployment) => [deployment.id, discover(deployment)]));
  // Every deployment's load on every backend, in an order alternated each round.
  const loads = deployments.flatMap((deployment) =>
    backends.map(({ id }) => ({ deployment, backend: id, key: `${deployment.id}/${id}` })),
  );
  const results = {};
  for (let round = 0; round < RUNS; round++) {
    const order = round % 2 === 0 ? loads : loads.toReversed();
    for (const { deployment, backend, key } of order) {
      const discovery = discoveries[deployment.id];
      const resources = discovery.loads[backend].map((resource) => ({ ...resource, url: urlFor(deployment, resource.path) }));
      if (round === 0) {
        const identity = [];
        for (const resource of resources) {
          identity.push({ path: resource.path, bytes: await identityBytes(resource.url) });
        }
        results[key] = {
          id: deployment.id,
          name: deployment.name,
          backend,
          entry: urlFor(deployment, ""),
          discovery,
          identity,
          batches: [],
        };
      }
      const started = performance.now();
      const requestResults = await Promise.all(resources.map((resource) => requestResource(resource.url, resource)));
      const ended = performance.now();
      const failed = requestResults.filter((resource) => !resource.ok);
      results[key].batches.push({
        run: round + 1,
        orderInRound: order.findIndex((load) => load.key === key),
        startedAt: new Date().toISOString(),
        wallMs: ended - started,
        ok: failed.length === 0,
        failures: failed.map((resource) => ({ path: resource.path, error: resource.error, stderr: resource.stderr })),
        encodedBytes: sum(requestResults.map((resource) => Number(resource.size_download))),
        identityBytes: sum(results[key].identity.map((resource) => resource.bytes)),
        resources: requestResults,
      });
      console.error(`${deployment.name} ${backend}: run ${round + 1}/${RUNS} ${failed.length ? "FAILED" : "ok"} ${Math.round(ended - started)} ms`);
    }
  }
  for (const result of Object.values(results)) {
    result.summary = summarizeBatches(result.batches);
  }
  const artifact = {
    startedAt,
    finishedAt: new Date().toISOString(),
    runsPerLoad: RUNS,
    backends: backends.map(({ id }) => id),
    transport: {
      client: "curl",
      addressFamily: "IPv4",
      httpVersionRequested: "HTTP/2",
      acceptEncoding: ACCEPT_ENCODING,
      localCache: "none: fresh curl process per resource request",
      batchMode: "one page load's resources per backend requested concurrently; deployment and backend order alternated each round",
      timingScope: "DNS, TCP, TLS, server wait, and body transfer as reported by curl",
      encodedBytes: "curl size_download with --raw and Accept-Encoding br, gzip; response body bytes on the wire",
      identityBytes: "one additional identity-encoding GET per resource, used only for payload-size baseline",
    },
    deployments: results,
  };
  const resultDate = new Date().toISOString().slice(0, 10);
  const outputDirectory = "artifacts/performance/host-download";
  const outputPath = `${outputDirectory}/host-download-benchmark-${resultDate}.json`;
  await mkdir(outputDirectory, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(JSON.stringify({ outputPath, startedAt, finishedAt: artifact.finishedAt, summaries: Object.fromEntries(Object.entries(results).map(([id, result]) => [id, result.summary])) }, null, 2));
}

main().catch((error) => {
  console.error(error.stack ?? error);
  process.exitCode = 1;
});
