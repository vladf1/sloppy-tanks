# First-load improvements

Measured September 13, 2026 against original commit `2d72aeb38425ba0e04897084790c50fd9289e701`. Changes are local; the public deployment has not been updated.

The default Pine Village game payload fell from **3.892 MB to 2.298 MB (41.0%)**. With approximately 92 KB of unchanged fonts, expect roughly **4.0 MB → 2.4 MB** under comparable compression. These are file-body download bytes, not GPU memory or total repository size.

The menu appeared **0.534 seconds sooner (24.4%)**, and the final asset download finished **1.343 seconds sooner (35.4%)**. A loading message now appears in **84 ms**, instead of leaving users without game content until roughly **2.17 seconds**.

## Attribution: each change applied independently

Each “only” build starts from the same original source/assets and adds only that change. The texture build is also an isolated comparison. Timing differences overlap and should not be added together.

| Build | Download | First visible content | Menu appearance | Last asset downloaded | Main-thread blocking before menu |
|---|---:|---:|---:|---:|---:|
| Original | 3.892 MB | 2.168 s | 2.183 s | 3.795 s | 599 ms |
| Textures only | 2.533 MB | 1.988 s | 2.007 s | 2.701 s | 596 ms |
| Offline previews only | 3.942 MB | 2.092 s | 2.109 s | 3.829 s | 530 ms |
| Startup sequencing only | 3.894 MB | 0.080 s | 2.133 s | 3.735 s | 598 ms |
| Separate WASM only | 3.606 MB | 1.920 s | 1.937 s | 3.538 s | 606 ms |
| All changes | 2.298 MB | 0.084 s | 1.650 s | 2.452 s | 545 ms |

- **Textures:** save **1.359 MB**, remove **1.093 s** from completion of downloads, and make the menu appear **177 ms** sooner. This is the largest byte reduction. CPU blocking is effectively unchanged.
- **Offline tank previews:** remove runtime creation of a second WebGL renderer and six render/readback/PNG-encoding operations. They reduce measured blocking by **69 ms** and menu latency by **75 ms**. The three visible WebP cards add **49.5 KB net**, making the final download finish **35 ms later** in isolation. Only the current team's three pictures load initially; all six are generated offline.
- **Startup sequencing:** a small bootstrap loads the game asynchronously, the HTML provides immediate feedback, and the tank texture downloads while physics initializes. First visible content moves from **2168 ms to 80 ms** in isolation. The actual menu improves by only **51 ms**; this change primarily removes the blank wait. It adds approximately **1.3 KB**. There is a visible retry action if startup fails.
- **Separate physics WASM:** saves **286 KB**, moves the menu **246 ms earlier**, and finishes downloads **257 ms sooner**. The physics binary is identical; it is delivered separately, preloaded, and can stream into the WASM compiler. The browser no longer parses the base64 binary as JavaScript. Main-thread blocking measured **7 ms worse** in this isolated run; there is no demonstrated CPU-blocking win from this step.

Combined blocking fell from **599 ms to 545 ms (9%)**. Most remaining startup processing is scene construction and initial rendering. These measurements do not establish an in-game FPS improvement.

## Texture quality and sizes

| Asset | Before | After | Reduction |
|---|---:|---:|---:|
| Grass | 745.2 KB | 370.5 KB | 50.3% |
| Dirt | 507.4 KB | 198.2 KB | 60.9% |
| Concrete | 456.9 KB | 155.4 KB | 66.0% |
| Tank wear | 241.5 KB | 23.0 KB | 90.5% |
| Conifer foliage | 252.7 KB | 97.7 KB | 61.3% |

The three large environment textures change from 1254×1254 to 1024×1024 at WebP quality 80. Tank wear stays 512×512 at quality 85. Conifer foliage stays 512×512 at quality 90; all **262,144 alpha pixels match exactly**. Original PNG sources are retained outside the deployed public folder. Menu previews retain their original 640×400 dimensions and framing.

The default scene and tank cards were inspected in the built-in browser. Full-size before/after menu captures are [original](performance/loading/baseline.png) and [updated](performance/loading/combined.png).

## Method and practical limits

- Five fresh browser contexts per build, empty HTTP cache and local storage, Chrome 152.0.7977.83, 1440×900 at device scale 1.
- Local production files, gzip bodies, 10 Mbps downstream, 50 ms emulated latency, unthrottled CPU on this Mac. Fonts are replaced with the same fallback in every run to exclude external-server variation.
- Values are medians. OS/GPU shader caches are not forcibly cleared; initial samples can be much slower. This is a controlled HTTP-cache comparison, not a universal first-ever-device load time. The raw files include every sample rather than hiding the slow first runs.
- “Menu appearance” is a rendering opportunity after the menu DOM appears, not proof that every texture is decoded/uploaded. “Last asset downloaded” measures network completion separately. Neither metric promises all GPU work is finished.
- “Blocking” sums the portion above 50 ms of each long main-thread task before menu appearance. Small differences around a few milliseconds should be treated as noise.
- Deployed sizes and times depend on hosting compression and the device. The new WASM asset should be served with `application/wasm` and gzip or Brotli. Wrong-MIME buffered fallback is checked separately. Actual deployment headers for the new binary remain to be verified when published.

## Reproduce and maintain

Run `npm run benchmark:loading -- <label>` against saved production files at `artifacts/performance/loading/<label>/`. An optional second argument selects a different build directory. The saved original and isolated build directories, per-run JSON, screenshots, and comparison fixture scripts are in `artifacts/performance/loading/`. Summary data is [loading-results.json](loading-results.json).

Run `npm run optimize:textures` with `cwebp` installed to re-encode original artwork. Run `npm run generate:previews` with Google Chrome installed after tank geometry/material changes. Ordinary builds use the checked-in images; players and CI do not render the previews.

## Validation

- `npm run check`: lint, formatting, TypeScript 7 production build, **180/180 tests** passed.
- Existing real keyboard/mouse checks, pause/resume, ammunition input, and ten rendered resets passed.
- Preview checks: three 640×400 images loaded; choosing Big Rig starts the correct tank; no page errors.
- Native-WASM and compatibility-loader simulations in the same Chrome version: **90 exact snapshot comparisons**, six scenarios across three maps, 900 ticks per scenario. The WASM files also match byte-for-byte.
- Production map-start and loading-failure checks: see [production-check.json](performance/loading/production-check.json).
