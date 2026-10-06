# Plan: less render CPU on the WebGPU engine

A hand-off for cutting the main-thread CPU the WebGPU engine spends per frame,
from what the WebGL work (#60, #68, #69, #70) measured. Delete this file once its
steps are done or dropped; the lasting rules belong in `AGENTS.md`.

The goal is the engine's `renderMs` (presentation plus `Renderer::render`), not
GPU time or frame rate: every desktop build already holds 60 Hz, and the slow
devices are where main-thread CPU decides frame drops.

## Where the time goes

PR 70's WebGPU engine (`crates/render/src/gpu/webgpu/` and the shared renderer),
Chrome 154 headless on an Apple M3 Max, overview camera at 1280×720, seed 12345,
600 profiled frames per scene. Profiling runs everything about twice as fast as a
plain run, so read these as proportions (µs per frame):

|                                  | Village | Harbor | Stress Grid |
| -------------------------------- | ------- | ------ | ----------- |
| `Presentation::render`           | 389     | 246    | 427         |
| `Renderer::render`               | 335     | 204    | 308         |
| `draw_frame` (encoding, submit)  | 138     | 108    | 108         |
| `queue.write_buffer`             | 57      | 33     | 66          |
| `DrawListBuilder::finish`        | 35      | 20     | 29          |
| `build_draws` (culling, records) | 27      | 18     | 37          |
| `Renderer::render` itself        | 26      | 17     | 50          |
| `reflection_bounds`              | 40      | 1      | —           |
| effects update and sync          | 29      | 21     | 69          |

WebGPU calls per frame (120 counted frames):

|                                      | Village     | Harbor      | Stress Grid |
| ------------------------------------ | ----------- | ----------- | ----------- |
| `drawIndexed`                        | 1,159       | 610         | 797         |
| `setBindGroup`                       | 1,060       | 523         | 666         |
| `setPipeline`                        | 382         | 147         | 324         |
| `setVertexBuffer` / `setIndexBuffer` | 118 / 81    | 125 / 85    | 78 / 43     |
| `writeBuffer` calls / bytes          | 16 / 227 KB | 14 / 134 KB | 17 / 296 KB |

For comparison, PR 70's WebGL engine binds about 240 materials and 66 programs
for the same village frame, because it groups draws by state.

## Steps, in the order to try

1. **Group WebGPU draws by state.** WebGPU keeps class index order
   (`GROUP_DRAWS_BY_STATE` is WebGL only), so it switches bind groups on 91% of
   village draws and pipelines on a third. That choice came from a 2026-10-03
   same-page A/B in which grouping by pipeline, page, pool and material cost the
   main thread 5–7% while saving the GPU process 1–13%. Since then the WebGL engine
   found material-first grouping (`draw_list::DrawState`) much better than
   page-first, and binding only what changes (`gpu/webgl/mod.rs` `DrawBinding`)
   worth 5–20%. The WebGPU loop (`webgpu/mod.rs` `encode`) already skips an
   unchanged pipeline, material and pool, so try grouping with the material-first
   order and expect `setBindGroup` near 300 and `setPipeline` near 70 in the
   village. Watch two costs: `order_classes` re-sorts every class whenever classes
   come or go (24 µs a frame on the Stress Grid, where debris adds classes every
   frame; make it incremental if grouping wins), and depth ties between coplanar
   parts, which the order decides (see `DrawListBuilder::finish`'s comment).
2. **Upload fewer instance bytes.** `writeBuffer` costs about 0.25 µs per KB here,
   about six times what WebGL's `texSubImage2D` costs for the same records
   (village: 57 µs against 9.5 µs). One or two calls of 64 KB or more carry most
   of the bytes, most likely the frame's dynamic instance records and the merged
   shadow records; confirm which. First measure
   whether the cost is per byte or per call with a page that writes N KB in one
   call. Then, in order of effort:
   - Keep the records of resting things (sleeping debris, idle props) in
     persistent ranges, as static scenery already is (`Source::Range`).
   - Shrink `InstanceRecord` from 96 bytes: `world` as 4×3 (80 bytes), or `tint`
     as RGBA8 too (68). It is WGSL `Instance` in `common.wgsl`, read through
     `instances_storage.wgsl` and `instances_texture.wgsl` and by effects that use
     `data`, on both engines.
   - Skip spans equal to the last frame's.
     Do not switch to `create_buffer_init` or `mapped_at_creation`: the browser
     backend stages the whole mapped range in a Wasm-side copy (`AGENTS.md`).
3. **Render bundles for static scenery.** Persistent instance ranges make static
   scenery draws identical from frame to frame, which suits a
   `wgpu::RenderBundle` replayed with `execute_bundles`: one call instead of a
   pipeline, bind group, buffer and draw call each. First count how many draws per
   pass are static ranges. Culling is the catch: a bundle draws all it recorded,
   so record one per pass and scenery cell, replay the visible cells, and record
   again when classes, pages or the round change.
4. **Shared CPU, both engines.**
   - **Reflection culling.** The reflection pass is about 41% of village and
     harbor draws, culled by a single rectangle around all visible water. Culling
     against the rectangles of `reflection_cull`'s clusters would drop objects
     beside a winding creek.
   - **Draw lists and effects.** `DrawListBuilder::finish`, `build_draws` and the
     effects' update and sync (69 µs on the Stress Grid) are the next shared
     costs.
5. **Fixed per-frame calls.** Two command encoders and two submits, the canvas
   texture and its view cost about 25–30 µs a frame together in the profile. Fold
   the passes into one encoder if that is easy; it is small.
6. **Optimization level.** `sloppy-render` builds at `opt-level = "s"`. Building it
   at 3 for the WebGPU engine alone (`--config
'profile.release.package.sloppy-render.opt-level=3'` in that engine's Cargo run,
   `scripts/build-wasm.mjs`) is untested; weigh its speed against the download,
   which most players pay.

## What did not pay

- **Fewer calls is not less CPU by itself.** `WEBGL_multi_draw` cut WebGL calls
  26–45% and saved nothing: Chrome validates every sub-draw, and wasm-bindgen
  builds a typed-array view per slice. Time every change; call counts only explain
  results.

## How to measure

- **Compare builds loaded at once.** Separate page runs drift 2× on this Mac
  (background processes, core placement). Build each variant as a static bundle
  with the debug API: `pnpm run wasm`, then
  `NODE_ENV=development pnpm exec vite build --minify false --outDir <dir>` (the
  harness finds the game's held loop by its `loop` name). Serve each bundle with
  `Cross-Origin-Opener-Policy: same-origin` and
  `Cross-Origin-Embedder-Policy: require-corp`, so `renderMs` has 5 µs
  resolution.
- **Step every page through the same frames.** Load one page per build, seeded
  (`seedGame`, 12345), with the overview camera. Hold `loop` and step every page
  through the same timestamps, starting at a fixed 1e6. Alternate 40-frame blocks
  in rotating order, with 300 warm-up and 1,500–2,000 timed frames, and pair frame
  _n_ across builds.
- **Report paired results.** Give the mean paired difference ± two standard errors
  over blocks. An A/A pair read +1.2% ± 2.5. Run every comparison a second time with
  the page order reversed: the first page created tends to be faster.
- **Sync the GPU on both sides.** WebGL pages get `gl.finish()` before each frame;
  give WebGPU pages the same, or a WebGPU-against-WebGL comparison counts WebGPU's
  waits on the GPU process. An init script can wrap
  `GPUAdapter.prototype.requestDevice` to keep the device, and the harness can
  then await `device.queue.onSubmittedWorkDone()` before each frame. A
  WebGPU-against-WebGPU A/B is fair either way.
- **Count calls** by wrapping the methods of `GPURenderPassEncoder`, `GPUQueue`
  and `GPUCommandEncoder` in an init script. Counting slows the page, so never time
  a counting run.
- **Profile** a build made with `CARGO_PROFILE_RELEASE_STRIP=false pnpm run wasm`
  through the Chrome DevTools Protocol profiler at a 100 µs interval, and sum each
  function's callees. The profiler roughly doubles everyone's speed, so compare
  proportions.
- **Keep the machine quiet:** no builds during a timing run.
- **Check the real loop:** confirm a result at 60 Hz with fresh pages per build,
  where renderer and GPU-process CPU are sampled with `ps`.

PR 70's worktree kept these harnesses, which are ignored and not in Git:
`artifacts/performance/webgl-best/` (`interleave.mjs`, `build-variant.sh`,
`callees.mjs`, `profile-summary.mjs`) and
`artifacts/performance/webgl-pr-compare/bench.mjs`. Copy them while they exist;
otherwise this section is enough to rebuild them.
