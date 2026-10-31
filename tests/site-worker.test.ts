import { test } from "node:test";
import assert from "node:assert/strict";
import redirect from "../deploy/site/redirect.mjs";
import worker, { preferredEncoding } from "../deploy/site/worker.mjs";

test("encoding negotiation honors supported encodings, quality and exclusions", () => {
  assert.equal(preferredEncoding("gzip, deflate, br, zstd"), "br");
  assert.equal(preferredEncoding("br;q=0, gzip;q=0.5, zstd;q=1"), "gzip");
  assert.equal(preferredEncoding("br;q=0, *;q=0.8"), "gzip");
  assert.equal(preferredEncoding("br;q=0.2, gzip;q=0.8"), "gzip");
  assert.equal(preferredEncoding("br;q=0.5, gzip;q=0.5"), "br");
  assert.equal(preferredEncoding("br;q=invalid, gzip;q=1"), "gzip");
  assert.equal(preferredEncoding("br ;q=0, gzip"), "gzip");
  assert.equal(preferredEncoding("br;q=2, gzip;q=-1"), null);
  assert.equal(preferredEncoding("identity"), null);
  assert.equal(preferredEncoding("br;q=0, gzip;q=0, zstd;q=0"), null);
  assert.equal(preferredEncoding(null), null);
});

test("Wasm variants preserve bytes, metadata and conditional responses", async () => {
  for (const status of [200, 304]) {
    let upstream: Request | undefined;
    const bytes = new Uint8Array([1, 2, 3]);
    const response = await worker.fetch(
      new Request("https://example.test/assets/engine.wasm", {
        headers: { "Accept-Encoding": "br", "If-None-Match": '"variant"' },
      }),
      {
        ASSETS: {
          fetch: async (request: Request) => {
            upstream = request;
            return new Response(status === 200 ? bytes : null, {
              status,
              headers: {
                "Content-Type": "application/octet-stream",
                ETag: '"variant"',
                "X-Robots-Tag": "noindex",
              },
            });
          },
        },
      },
    );
    assert.equal(upstream?.url, "https://example.test/assets/engine.wasm.br");
    assert.equal(upstream?.headers.get("Accept-Encoding"), "identity");
    assert.equal(upstream?.headers.get("If-None-Match"), '"variant"');
    assert.equal(response.status, status);
    assert.equal(response.headers.get("Content-Encoding"), "br");
    assert.equal(response.headers.get("Content-Type"), "application/wasm");
    assert.equal(response.headers.get("ETag"), '"variant"');
    assert.equal(response.headers.get("X-Robots-Tag"), "noindex");
    assert.equal(response.headers.get("Vary"), "Accept-Encoding");
    assert.match(response.headers.get("Cache-Control")!, /no-transform/);
    if (status === 200) assert.deepEqual(new Uint8Array(await response.arrayBuffer()), bytes);
  }
});

test("range requests and missing compressed variants fall back without mislabeling HTML", async () => {
  for (const range of [false, true]) {
    const paths: string[] = [];
    const response = await worker.fetch(
      new Request("https://example.test/assets/engine.wasm", {
        headers: { "Accept-Encoding": "br", ...(range ? { Range: "bytes=0-3" } : {}) },
      }),
      {
        ASSETS: {
          fetch: async (request: Request) => {
            paths.push(new URL(request.url).pathname);
            return new Response("fallback", { headers: { "Content-Type": "text/html" } });
          },
        },
      },
    );
    assert.deepEqual(
      paths,
      range ? ["/assets/engine.wasm"] : ["/assets/engine.wasm.br", "/assets/engine.wasm"],
    );
    assert.equal(response.headers.get("Content-Encoding"), null);
  }
});

test("Cloudflare's original client encoding wins over its normalized header", async () => {
  const request = new Request("https://example.test/assets/engine.wasm", {
    headers: { "Accept-Encoding": "br, gzip" },
  });
  Object.defineProperty(request, "cf", { value: { clientAcceptEncoding: "gzip" } });
  let path = "";
  const response = await worker.fetch(request, {
    ASSETS: {
      fetch: async (asset: Request) => {
        path = new URL(asset.url).pathname;
        return new Response("encoded", { headers: { "Content-Type": "application/octet-stream" } });
      },
    },
  });
  assert.equal(path, "/assets/engine.wasm.gz");
  assert.equal(response.headers.get("Content-Encoding"), "gzip");
});

test("HTML and canonical redirects pass through to the static asset service unchanged", async () => {
  const paths = [
    "/",
    "/?webgl",
    "/index.html",
    "/test-pages",
    "/health/",
    "/tests/maps.browser.html",
  ];
  for (const path of paths) {
    for (const method of ["GET", "HEAD"]) {
      for (const status of [200, 304]) {
        const request = new Request("https://example.test" + path, {
          method,
          headers: { "Accept-Encoding": "br", "If-None-Match": '"page"' },
        });
        const assetResponse = new Response(status === 304 || method === "HEAD" ? null : "page", {
          status,
          headers: { "Content-Type": "text/html", ETag: '"page"' },
        });
        let upstream: Request | undefined;
        const response = await worker.fetch(request, {
          ASSETS: {
            fetch: async (asset: Request) => {
              upstream = asset;
              return assetResponse;
            },
          },
        });
        assert.equal(upstream, request);
        assert.equal(response, assetResponse);
      }
    }
  }
});

test("the site's other hostnames redirect to its origin with the path and query", () => {
  const response = redirect.fetch(new Request("https://www.example.test/health/?room=abc"), {
    SITE_ORIGIN: "https://example.test",
  });
  assert.equal(response.status, 301);
  assert.equal(response.headers.get("Location"), "https://example.test/health/?room=abc");
});
