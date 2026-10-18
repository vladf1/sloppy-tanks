# Hosting speed tests: 17 October 2026

GitHub Pages delivered this build faster from the test Mac than either Cloudflare
location. Precompressing Wasm helped the Worker, but did not close that gap.
Precompressing initial HTML saved about 4.4 KB without a reliable latency benefit,
so HTML precompression was removed. The remaining delay was mostly before the
first byte, including when the complete response was demonstrably cached.

These are historical measurements from one client and network, taken on
17 October Pacific time (18 October UTC). They do not establish a global ranking
of the providers or predict browser startup or gameplay performance.

## Sites and build

| Site                                  | URL                                                    | Observed client-facing edge |
| ------------------------------------- | ------------------------------------------------------ | --------------------------- |
| GitHub Pages, production              | <https://sloppy-tanks.fridman.me/>                     | Fastly BUR, Burbank         |
| Cloudflare Pages, dev                 | <https://sloppy-tanks-dev.fridman.me/>                 | Cloudflare LAX, Los Angeles |
| Cloudflare Workers Static Assets, dev | <https://sloppy-tanks-dev-worker.vova145.workers.dev/> | Cloudflare LAX, Los Angeles |

All three reported game source commit `1a655e2`, protocol 4, and content version
`2bb4911ce39d85f479b2332a`. Production reported release `1.1.1.234`; both dev
locations reported `1.1.1`. Pages and Worker used the same dev build. Production
was built separately: its HTML and Wasm were slightly different, with the raw
WebGPU Wasm differing by less than 0.04%. The 36 assets used in the shared static
pack were byte-identical across hosts. The controlled compression comparisons
below used identical decoded files on the same Worker.

The edge labels identify where the client connected. They do not identify an
internal storage location or explain the provider's internal request path. Both
providers have a CDN in front of the static files. The game servers were left off
for the dev experiments; these requests served static content.

## Three-host comparison

There were 18 rounds per host and workload, using every ordering of the three
hosts three times. Requests used HTTP/2 over IPv4, canonical URLs, no client cache
or cache-busting, and `Accept-Encoding: gzip, deflate, br, zstd`. URLs had already
been fetched for identity checks; this was not a controlled cold edge-cache test.
All 270 timed batches succeeded, and no slow samples were removed.

Values below are median network time in milliseconds, with the full minimum to
maximum range. Network time includes DNS, connection setup, first-byte wait and
body transfer, and excludes launching curl. At this stage the Worker used
precompressed Wasm and ordinary automatic compression for HTML.

| Resource           |         GitHub Pages |       Cloudflare Pages |           Dev Worker |
| ------------------ | -------------------: | ---------------------: | -------------------: |
| Initial HTML       |  23.53 (21.27–29.32) |   67.99 (41.68–105.88) |  58.67 (39.18–73.62) |
| WebGPU Wasm        | 45.64 (43.05–123.03) | 216.77 (182.06–501.45) | 80.20 (63.60–143.89) |
| WebGL Wasm         | 49.56 (43.43–111.82) | 251.63 (192.66–406.50) | 90.28 (62.67–131.48) |
| Tank preview atlas | 28.79 (24.99–110.31) |    56.60 (45.73–95.64) |  58.06 (48.46–88.86) |

The separate 36-file pack, fetched with up to eight parallel transfers, took
77.15 ms on GitHub Pages (66.42–419.35), 173.38 ms on Cloudflare Pages
(148.89–275.16), and 185.96 ms on the Worker (141.17–336.10). These are elapsed
batch times, including curl startup, rather than the single-request metric in
the table. The pack includes optional map assets and is not the game's startup
download graph.

A smaller response did not necessarily arrive sooner. GitHub served WebGPU Wasm
as 1,364,993 bytes of gzip; Cloudflare Pages served about 1,285,347 bytes of Brotli;
the Worker served 1,010,143 bytes of prebuilt Brotli. For that file the median body
transfer alone was about 20.11, 23.41 and 19.25 ms respectively. Much of the total
difference therefore occurred before the body arrived.

Early exploratory runs advertised only Brotli and gzip. Those exaggerated the
Worker's automatic-compression cost relative to a browser advertising Zstandard.
The comparison above and the modern-client controls below include Zstandard.

## Wasm precompression

This comparison used the same Worker and the same decoded Wasm files. The
`/delivery-baseline/` copies used automatic static compression and bypassed the
custom handler. Each engine and mode had 18 timed samples, with all six mode
orderings repeated three times: 108 successful requests in total. All samples
were retained.

| Delivery mode                                         | WebGPU median network time | WebGL median network time |
| ----------------------------------------------------- | -------------------------: | ------------------------: |
| Prebuilt Brotli                                       |    86.85 ms (58.92–123.27) |   79.65 ms (59.71–122.52) |
| Automatic, modern encoding header; Zstandard selected |   100.44 ms (84.39–217.66) |  119.78 ms (88.51–189.09) |
| Automatic, Brotli/gzip only; Brotli selected          |  218.00 ms (182.02–295.41) | 264.77 ms (219.50–548.82) |

Against the modern automatic mode, precompression reduced median network time by
about 14% for WebGPU and 34% for WebGL. WebGPU wire size fell from 1,307,373 to
1,010,143 bytes; WebGL fell from 1,582,094 to 1,222,444 bytes. These measurements
combine the smaller transfer, compression behavior and custom handler; they do
not isolate the CPU time spent compressing a response.

Brotli and gzip wire hashes and decoded hashes were checked for all three dev
Wasm builds, including the lab build. Headless Chrome checks passed startup,
starting a round, movement and firing with both WebGPU and WebGL.

## Initial HTML precompression

This separate trial used 30 samples for each of five cases: GitHub Pages,
Cloudflare Pages, precompressed Worker HTML, and two automatic Worker modes.
Balanced rotating and reversed case orders reduced ordering bias. All 150
requests succeeded and were retained. The Worker copies decoded to the same
76,148-byte HTML file.

| Worker HTML delivery | Median encoded bytes | Median network time and full range | Median body transfer |
| -------------------- | -------------------: | ---------------------------------: | -------------------: |
| Prebuilt Brotli      |               18,219 |             57.83 ms (39.00–98.39) |              0.59 ms |
| Automatic Zstandard  |               22,572 |            60.99 ms (39.16–259.27) |              0.66 ms |
| Automatic Brotli     |               21,445 |             63.02 ms (38.71–96.32) |              0.69 ms |

GitHub and Cloudflare Pages controls in this trial had median network times of
24.14 and 69.89 ms respectively. Do not combine these with the 18-round cohort as
if they were one experiment.

Prebuilt Brotli saved 4,353 bytes, or 19.3%, against automatic Zstandard. It was
faster in 17 of 30 paired rounds; the median paired saving was 1.30 ms. The mean
paired saving was 6.09 ms with plus or minus 16.53 ms of two standard errors,
including the slow automatic-response outlier. That does not demonstrate a
reliable latency improvement. Median body transfer was already below 1 ms;
first-byte waiting dominated. HTML precompression was removed.

## Connection reuse and full-response caching

A later diagnostic series separated fresh connections from verified reused
HTTP/2 connections, checked the provider Pages hostname, requested uncompressed
HTML, and compared platform-generated `/cdn-cgi/trace` responses. It also briefly
enabled native Workers Cache with a 600-second edge TTL while keeping browser
HTML revalidation. Across the diagnostic series, all 1,220 timed responses were
HTTP 200 and all outliers were kept.

Selected median first-byte times on reused connections:

| Endpoint                              | Before cache experiment, 28 samples | During experiment, 28 samples | After restoration, 14 samples |
| ------------------------------------- | ----------------------------------: | ----------------------------: | ----------------------------: |
| GitHub Pages HTML                     |                             4.22 ms |                       4.18 ms |                       4.33 ms |
| Pages custom hostname HTML            |                            45.30 ms |                      41.83 ms |                      47.72 ms |
| Pages provider hostname HTML          |                            37.94 ms |                      40.50 ms |                      33.16 ms |
| Worker automatic static HTML baseline |                            38.04 ms |                      78.92 ms |                      31.88 ms |
| Worker precompressed HTML             |                            35.65 ms |                      28.83 ms |                      31.29 ms |

These phases ran sequentially, so their differences include temporal variation.
The strongest cache evidence was a random marker generated inside the Worker:
56 responses returned the same marker while `Age` increased from 0 to 21 seconds.
That demonstrated reuse of a complete cached response without rerunning the
handler. It still did not approach GitHub's first-byte latency from this client.
The cached variant happened to be gzip, not Brotli; all copies decoded identically.

The automatic static baseline became slower with the extra cache enabled, so
adding a cache layer was not a general fix. The experiment was reverted. A
`CF-Cache-Status: HIT` header alone was not treated as proof of full-response
caching: the Static Assets implementation can infer that label from storage
lookup timing.

One retained Pages sample took 10.085 seconds, of which 10.036 seconds was
connection setup. Reused connections still showed a persistent gap, so DNS/TLS
setup alone did not explain it. The provider hostname was also slow, and the
platform trace endpoints were faster than the static HTML path. Those controls
narrow the issue to the delivery path, but do not identify Cloudflare's internal
bottleneck. A trace endpoint is not an equivalent static-asset workload.

## Resulting configuration and evidence

The dev Worker uses `main.mjs` and runs first for `/assets/*.wasm`. The local
`precompress.mjs` script generates Brotli and gzip Wasm variants before upload.
`shared.mjs` negotiates encoding and retrieves the stored compressed bytes. HTML
and other assets use ordinary Static Assets delivery. Native Workers Cache is
not enabled. Production remains on GitHub Pages.

Raw requests, summaries, hashes, benchmark scripts and browser screenshots remain
in the ignored `artifacts/performance/host-download/` directories:

- `2026-10-17-precompressed/`: corrected three-host comparison and controlled Wasm trial.
- `2026-10-17-precompressed-html/`: 30-round HTML trial and paired analysis.
- `2026-10-17-html-latency/`: connection, trace, cache and restoration diagnostics.

Those local artifacts are not included in Git. This report preserves the sample
counts, measured ranges, methods and limits needed to interpret the findings.
Re-measure before making a new hosting decision: these results describe one
client's HTTP delivery on that date, not end-to-end browser loading or gameplay.
