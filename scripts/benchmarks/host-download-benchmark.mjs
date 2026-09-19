import { execFile, execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const RUNS = 13;
const MAX_TIME_SECONDS = 60;
const ACCEPT_ENCODING = "br, gzip";
const deployments = [
  {
    id: "github-pages",
    name: "GitHub Pages",
    origin: "https://fridman.me",
    prefix: "/sloppy-tanks",
    entry: "https://fridman.me/sloppy-tanks/",
  },
  {
    id: "cloudflare-pages",
    name: "Cloudflare Pages",
    origin: "https://sloppy-tanks.fridman.me",
    prefix: "",
    entry: "https://sloppy-tanks.fridman.me/",
  },
];

const texturePaths = [
  "textures/barrels/painted-drum.webp",
  "textures/ground/dry-grass.webp",
  "textures/ground/packed-dirt.webp",
  "textures/houses/shingles.webp",
  "textures/houses/siding.webp",
  "textures/pickups/laser.webp",
  "textures/pickups/piercing.webp",
  "textures/pickups/rapid.webp",
  "textures/pickups/repair.webp",
  "textures/pickups/ricochet.webp",
  "textures/pickups/rocket.webp",
  "textures/pickups/shield.webp",
  "textures/pickups/speed.webp",
  "textures/pickups/spread.webp",
  "textures/tanks/armor-wear.webp",
  "textures/trees/bark.webp",
  "textures/trees/birch.webp",
  "textures/trees/conifer-spray.webp",
  "textures/trees/leaves.webp",
  "textures/trees/rings.webp",
  "textures/walls/weathered-concrete.webp",
  "textures/water/normals.webp",
];
const audioPaths = [
  "audio/explosion.mp3",
  "audio/hit.mp3",
  "audio/impact.mp3",
  "audio/laser.mp3",
  "audio/pickup.mp3",
  "audio/promotion.mp3",
  "audio/shot-piercing.mp3",
  "audio/shot-ricochet.mp3",
  "audio/shot-rocket.mp3",
  "audio/shot-spread.mp3",
  "audio/shot.mp3",
];

function curlText(url) {
  return execFileSync(
    "curl",
    ["--fail", "--silent", "--show-error", "--ipv4", "--http2", "--compressed", "--max-time", "20", url],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
  );
}

function urlFor(deployment, path) {
  return `${deployment.origin}${deployment.prefix}/${path.replace(/^\/+/, "")}`;
}

function discover(deployment) {
  const html = curlText(deployment.entry);
  const match = html.match(/import\((?:"|')([^"']+assets\/game-[^"']+\.js)(?:"|')\)/);
  if (!match) {
    throw new Error(`Could not find game chunk in ${deployment.entry}`);
  }
  const chunkPath = match[1].startsWith("/") ? match[1] : `${deployment.prefix}/${match[1]}`;
  const chunkResourcePath = chunkPath.startsWith(`${deployment.prefix}/`)
    ? chunkPath.slice(deployment.prefix.length + 1)
    : chunkPath.replace(/^\/+/, "");
  const paths = [
    { path: "", type: "html", label: "HTML entry" },
    { path: "favicon.svg", type: "startup", label: "Favicon" },
    { path: "previews/tanks.webp", type: "startup", label: "Tank preview atlas" },
    { path: chunkResourcePath, type: "code", label: "Main game chunk" },
    ...texturePaths.map((path) => ({ path, type: "texture", label: path })),
    ...audioPaths.map((path) => ({ path, type: "audio", label: path })),
  ];
  return { htmlBytes: Buffer.byteLength(html), chunkPath, paths };
}

function parseCurlJson(value) {
  const line = value.trim().split("\n").at(-1);
  return JSON.parse(line);
}

function requestResource(url, resource) {
  return new Promise((resolve) => {
    const started = performance.now();
    const args = [
      "--fail",
      "--silent",
      "--show-error",
      "--ipv4",
      "--http2",
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
    const child = execFile("curl", args, { maxBuffer: 1024 * 1024 }, (error, stdout, stderr) => {
      const ended = performance.now();
      if (error) {
        resolve({
          ...resource,
          url,
          ok: false,
          error: error.message,
          stderr: stderr.trim(),
          processWallMs: ended - started,
        });
        return;
      }
      try {
        const metrics = parseCurlJson(stdout);
        resolve({
          ...resource,
          url,
          ok: metrics.http_code === "200",
          ...metrics,
          processWallMs: ended - started,
        });
      } catch (parseError) {
        resolve({
          ...resource,
          url,
          ok: false,
          error: `Invalid curl metrics: ${parseError.message}`,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          processWallMs: ended - started,
        });
      }
    });
    child.on("error", (error) => {
      resolve({ ...resource, url, ok: false, error: error.message, processWallMs: performance.now() - started });
    });
  });
}

async function identityBytes(url) {
  const { stdout } = await execFileAsync(
    "curl",
    [
      "--fail",
      "--silent",
      "--show-error",
      "--ipv4",
      "--http2",
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
  const results = {};
  for (let round = 0; round < RUNS; round++) {
    const order = round % 2 === 0 ? deployments : deployments.toReversed();
    for (const deployment of order) {
      const discovery = discoveries[deployment.id];
      const resources = discovery.paths.map((resource) => ({ ...resource, url: urlFor(deployment, resource.path) }));
      if (round === 0) {
        const identity = [];
        for (const resource of resources) {
          identity.push({ path: resource.path, bytes: await identityBytes(resource.url) });
        }
        results[deployment.id] = {
          id: deployment.id,
          name: deployment.name,
          entry: deployment.entry,
          discovery,
          identity,
          batches: [],
        };
      }
      const started = performance.now();
      const requestResults = await Promise.all(resources.map((resource) => requestResource(resource.url, resource)));
      const ended = performance.now();
      const failed = requestResults.filter((resource) => !resource.ok);
      results[deployment.id].batches.push({
        run: round + 1,
        orderInRound: order.indexOf(deployment),
        startedAt: new Date().toISOString(),
        wallMs: ended - started,
        ok: failed.length === 0,
        failures: failed.map((resource) => ({ path: resource.path, error: resource.error, stderr: resource.stderr })),
        encodedBytes: sum(requestResults.map((resource) => Number(resource.size_download))),
        identityBytes: sum(results[deployment.id].identity.map((resource) => resource.bytes)),
        resources: requestResults,
      });
      console.error(`${deployment.name}: run ${round + 1}/${RUNS} ${failed.length ? "FAILED" : "ok"} ${Math.round(ended - started)} ms`);
    }
  }
  for (const result of Object.values(results)) {
    result.summary = summarizeBatches(result.batches);
  }
  const artifact = {
    startedAt,
    finishedAt: new Date().toISOString(),
    runsPerDeployment: RUNS,
    transport: {
      client: "curl",
      addressFamily: "IPv4",
      httpVersionRequested: "HTTP/2",
      acceptEncoding: ACCEPT_ENCODING,
      localCache: "none: fresh curl process per resource request",
      batchMode: "all resources requested concurrently; deployment order alternated each round",
      timingScope: "DNS, TCP, TLS, server wait, and body transfer as reported by curl",
      encodedBytes: "curl size_download with --raw and Accept-Encoding br, gzip; response body bytes on the wire",
      identityBytes: "one additional identity-encoding GET per resource, used only for payload-size baseline",
    },
    deployments: results,
  };
  const resultDate = new Date().toISOString().slice(0, 10);
  const outputPath = `scripts/benchmarks/results/host-download-benchmark-${resultDate}.json`;
  await mkdir("scripts/benchmarks/results", { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(artifact, null, 2)}\n`);
  console.log(JSON.stringify({ outputPath, startedAt, finishedAt: artifact.finishedAt, summaries: Object.fromEntries(Object.entries(results).map(([id, result]) => [id, result.summary])) }, null, 2));
}

main().catch((error) => {
  console.error(error.stack ?? error);
  process.exitCode = 1;
});
