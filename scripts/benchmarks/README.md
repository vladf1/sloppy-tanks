# Host download benchmarks

This folder contains HTTP-only comparisons of the live Sloppy Tanks hosts: the GitHub
Pages production site and the Cloudflare Pages dev site. The dev site serves a
development build, so compare shared static assets rather than its game chunk.

## Run

From the repository root:

```sh
node scripts/benchmarks/host-download-benchmark.mjs
```

Each run writes a dated raw JSON artifact to the ignored `artifacts/performance/host-download/`.

This is a manual benchmark. It is intentionally excluded from the repository-wide CI lint and format checks and is not invoked by any GitHub build workflow.

## Scope and method

- 13 alternating rounds per deployment.
- Each round requests the HTML entry, favicon, tank preview atlas, hashed game chunk, and every texture (including the pickup atlas) and audio file the script lists.
- Optional Harbor/Quarry-only assets and unused public files are not part of the default `/` startup graph.
- Each resource is fetched concurrently using a fresh curl process with HTTP/2, IPv4, no local cache, and `Accept-Encoding: br, gzip`.
- Response bodies are consumed as raw encoded data so wire-byte totals include the encoding negotiated by each host.
- An additional identity-encoding request records the uncompressed payload-size baseline.

## Reported metrics

- `batchWallMs`: wall time until a round's complete batch finishes.
- `encodedBytes`: response body bytes transferred on the wire.
- `identityBytes`: response bytes without content encoding.
- `time_starttransfer`: per-resource time to first byte, including DNS/TCP/TLS and server wait.
- `time_total`: per-resource time until the body completes.
- `aggregateEncodedThroughputBytesPerSecond`: encoded batch bytes divided by batch wall time.

These are delivery measurements, not browser startup, rendering, JavaScript execution, or gameplay measurements. Results are specific to the test machine, network path, CDN edge, and time of day; retain raw artifacts when comparing future deployments.
