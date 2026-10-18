export function preferredEncoding(acceptEncoding) {
  const qualityByEncoding = new Map();

  for (const entry of (acceptEncoding ?? "").toLowerCase().split(",")) {
    const [encoding, ...parameters] = entry.trim().split(";");
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
    qualityByEncoding.set(encoding, quality);
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

export async function fetchCompressedAsset(request, assets, path) {
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
