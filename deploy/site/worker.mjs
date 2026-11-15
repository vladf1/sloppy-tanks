// The site's Worker, for the dev site and production. Static assets serve everything
// except the engine binaries, the shell's font collection, its shader tables and the
// game page, which `run_worker_first` routes here: Cloudflare compresses those on every
// request (the tables 32-45% larger than Brotli-11), so precompress.mjs stores Brotli and
// gzip copies at deploy time and this script sends the one the client accepts.
//
// The game page comes this way for its ETag. A browser asks for the page again at every
// visit, and Cloudflare sends its own compressed copy of an HTML file without one, so
// every visit downloaded the whole page. A precompressed copy leaves here marked
// `no-transform`, which keeps the ETag the asset service gives it (the hash of that
// copy, so it changes exactly when the page does); the browser's next visit names it in
// `If-None-Match`, the asset service answers 304, and nothing downloads.

/** The precompressed files under `/assets/`, by extension, with the type each is served
 * as; a browser keeps them for good. */
const contentTypes = {
  ".wasm": "application/wasm",
  ".ttc": "font/collection",
  ".json": "application/json",
};
const KEPT_FOR_GOOD = "public, max-age=31536000, immutable, no-transform";

/** The game page: its address, its file in the build, and how it is served. */
const PAGE_PATH = "/";
const PAGE_FILE = "/index.html";
const PAGE_TYPE = "text/html; charset=utf-8";
const ASKED_FOR_EACH_VISIT = "public, max-age=0, must-revalidate, no-transform";

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    const page = path === PAGE_PATH;
    const contentType = page ? PAGE_TYPE : contentTypes[path.slice(path.lastIndexOf("."))];
    if ((!page && !path.startsWith("/assets/")) || contentType === undefined) {
      return env.ASSETS.fetch(request);
    }

    const file = page ? PAGE_FILE : path;
    const response = await fetchCompressedAsset(request, env.ASSETS, file);
    if (response === null) {
      return env.ASSETS.fetch(request);
    }

    response.headers.set("Content-Type", contentType);
    response.headers.set("Cache-Control", page ? ASKED_FOR_EACH_VISIT : KEPT_FOR_GOOD);
    response.headers.set("X-Sloppy-Delivery", "precompressed");
    return response;
  },
};

export function preferredEncoding(acceptEncoding) {
  const qualityByEncoding = new Map();

  for (const entry of (acceptEncoding ?? "").toLowerCase().split(",")) {
    const [coding, ...parameters] = entry.split(";");
    let quality = 1;

    for (const parameter of parameters) {
      const setting = parameter.trim();
      if (setting.startsWith("q=")) {
        quality = Number(setting.slice(2));
        break;
      }
    }

    if (!Number.isFinite(quality) || quality < 0 || quality > 1) {
      quality = 0;
    }
    qualityByEncoding.set(coding.trim(), quality);
  }

  const wildcardQuality = qualityByEncoding.get("*") ?? 0;
  const brotliQuality = qualityByEncoding.get("br") ?? wildcardQuality;
  const gzipQuality = qualityByEncoding.get("gzip") ?? wildcardQuality;

  if (brotliQuality > 0 && brotliQuality >= gzipQuality) {
    return "br";
  }
  if (gzipQuality > 0) {
    return "gzip";
  }
  return null;
}

async function fetchCompressedAsset(request, assets, path) {
  if (request.method !== "GET" && request.method !== "HEAD") {
    return null;
  }
  if (request.headers.has("Range")) {
    return null;
  }

  // Cloudflare may rewrite Accept-Encoding before the request reaches this Worker.
  const acceptEncoding = request.cf?.clientAcceptEncoding ?? request.headers.get("Accept-Encoding");
  const encoding = preferredEncoding(acceptEncoding);
  if (encoding === null) {
    return null;
  }

  const compressedUrl = new URL(request.url);
  const extension = encoding === "br" ? ".br" : ".gz";
  compressedUrl.pathname = path + extension;

  const compressedRequest = new Request(compressedUrl, request);
  compressedRequest.headers.set("Accept-Encoding", "identity");
  const response = await assets.fetch(compressedRequest);

  if (response.status !== 200 && response.status !== 304) {
    return null;
  }
  // A missing file can return the SPA's index.html with status 200.
  if (response.headers.get("Content-Type")?.includes("text/html")) {
    return null;
  }

  const headers = new Headers(response.headers);
  headers.set("Content-Encoding", encoding);
  headers.set("Vary", "Accept-Encoding");

  // The file is already compressed; send its bytes without encoding them again.
  return new Response(response.body, { status: response.status, headers, encodeBody: "manual" });
}
