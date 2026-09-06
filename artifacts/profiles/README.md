# Chrome profiling — September 6, 2026

Rendering dominates the measured game CPU work. This pass removes excess scenery geometry, batches distant trees, and avoids uploading unused effect-buffer capacity. It does not change simulation or combat behavior.

## Conservative before/after comparison

Final crowded repeat, with no CPU sampler attached, 2560×1440:

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Average rendering CPU, ms/frame | 1.546 | 1.485 | −4.0% |
| Median GPU rendering, ms/frame | 2.129 | 2.046 | −3.9% |
| GPU rendering p95, ms/frame | 2.375 | 2.265 | −4.6% |
| Average draw calls/frame | 443 | 400 | −9.7% |
| Average rendered triangles/frame | 361,811 | 296,232 | −18.1% |
| Frame interval p95, ms | 9.3 | 9.3 | unchanged |
| Average FPS | 119.75 | 119.72 | effectively unchanged |

Both versions stayed near the display's 120 Hz ceiling. This run did not reproduce a sustained 111 FPS slowdown. The change reduces rendering cost; it is not evidence that every occasional frame drop is fixed.

The initial profiled normal run reduced draw calls from 332 to 287 and triangles from 293,639 to 216,532. Initial GPU medians fell from 2.07 to 1.27 ms (normal) and 2.58 to 1.39 ms (crowded). These larger timing gains did **not** reproduce consistently: the conservative final repeat above should guide expectations. The initial baseline crowded GPU p95 of 5.30 ms also did not recur. No overall CPU improvement was established by the initial sampled runs.

## What the Chrome CPU profiler showed

Four actual 20-second Chrome CPU captures used a 1 ms sampling interval. Rendering accounted for 85–92% of baseline sampled render-plus-simulation time. Three.js render submission, matrix updates, scene traversal, vertex-array binding, and buffer uploads dominated. Simulation averaged 0.20–0.33 ms/frame across all runs; no dominant collision/AI CPU bottleneck appeared in these workloads.

Buffer-upload self samples (`bufferSubData`) dropped from 83.7 to 27.8 ms over the normal capture and 53.6 to 21.2 ms over the crowded capture. These are sampling observations, not whole-frame savings or exact call-duration measurements. Total rendering samples were roughly unchanged in the crowded capture.

Raw captures, importable into Chrome DevTools:

- [Normal before](before-normal.cpuprofile) / [normal after](after-normal.cpuprofile)
- [Crowded before](before-crowded.cpuprofile) / [crowded after](after-crowded.cpuprofile)
- [All six benchmark runs and settings](results.json)

## Changes

| Scenery | Before triangles | After triangles |
|---|---:|---:|
| 12 arena pine trees | 34,368 | 8,736 |
| 24 decorative boundary trees | 68,736 | 3,744 |
| 20 fences | 28,080 | 3,120 |

The earlier audit missed the 24 boundary trees using full-detail models. Those now use a distant model and batch into two rows: 48 mesh objects become four. Arena trees retain five foliage tiers, 18 branch tufts, bark ridges, and roots, with fewer radial segments. Fence dimensions stay the same; sub-pixel rounded bevels are removed. House surfaces and pickup icon textures are unchanged.

Track marks now upload only newly written ring-buffer spans, including both sides of a wrap. Projectiles, debris, and particles upload only their active instance prefixes instead of full-capacity buffers. Track lifetime/fading and effect counts remain unchanged.

## Method and limits

Chrome 152, isolated profile, foreground tab, Vite development server, seed 207, both speed sliders at 115%, fixed 2560×1440 framebuffer, normal follow camera at zoom 34. Normal workload: 12 tanks. Crowded workload: 24 tanks plus 100 opposing shells injected every six simulated seconds. Both warm the simulation for 30 seconds before 40 seconds of live rendering. CPU/frame summaries exclude the first five seconds; GPU queries sample every tenth frame across the full 40 seconds. Cosmetic particles use unseeded randomness, so triangle counts vary slightly. The crowded workloads all reached 114 simultaneous projectiles and 20 fragments.

CPU sampling ran during 20 seconds of each initial workload. The final crowded pair used no CPU sampler. GPU times cover WebGL rendering only, not DOM compositing or presentation; draw calls/triangles include render passes. These are local observations, not a cross-device performance guarantee. Baseline was a snapshot of the checkout immediately before this pass, including the previous house optimization.

Re-run using `/sloppy-tanks/tools/profile.html`. The dev-only page provides normal/crowded benchmark buttons and a scenery preview. `scripts/capture-cpu-profile.mjs LABEL 20` captures a Chrome instance launched with remote debugging on port 9227; it only uses the CDP CPU Profiler, with navigation and input performed through the browser UI.

Validation: all 64 tests pass; TypeScript and production build pass; whitespace check passes. Added scenery triangle budgets and a track upload regression covering ring wrap and reset. Compared baseline and optimized tree/fence appearances through the production renderer in Chrome. Detailed house shingles and pickup symbols remain visible.
