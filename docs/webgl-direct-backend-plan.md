# Direct WebGL2 backend (glow): work plan and hand-off notes

> **Work in progress.** These notes hand the work from a cloud session to a local one.
> Delete this file (and `scripts/webgl-render-ab.mjs`, or move it to the ignored
> `artifacts/performance/`) before the pull request merges; fold what stays true into
> the renderer's module docs and `README.md`/`AGENTS.md` instead.

## Goal

Replace the WebGL2 fallback's path through wgpu (wgpu-core, wgpu-hal's GL backend,
glow) with a direct WebGL2 backend on [glow](https://crates.io/crates/glow), keeping
the WebGPU engine as it is. Recover the render CPU lost when PR 60 dropped its patched
wgpu-hal GL state cache, and shrink the WebGL engine's download and build, without
vendoring a library. Baseline: PR 60's head, `b67fd17` (this branch starts there).

Historical context from PR 60 (headless Chrome, M3 Max, not a fresh baseline): render
CPU per frame with the patched vs stock wgpu-hal was Village 1.15 vs 1.30 ms, Harbor
0.98 vs 1.15 ms, Stress Grid 1.51 vs 1.87 ms.

## What the cloud session established

The cloud VM (4-core Xeon at 2.8 GHz, 15 GB, no GPU, Linux) had only Playwright's
Chromium 141; WebGL2 ran on ANGLE over SwiftShader (software Vulkan), and WebGPU only
behind flags. SwiftShader drew the village at about **0.4-0.6 fps** (1280×720 and
480×270 alike) and took 30-130 s to load a map, with its threads competing with the
page for the same four cores. Timings there are not comparable with real hardware, so
none of them are recorded as a baseline below; only hardware-independent facts are.

**Baseline build** (`pnpm run wasm` at `b67fd17`, both engines in parallel, cold, Kache
not installed): 2 min 9 s wall, 6 min 58 s user + 42 s system CPU on that VM.

**Baseline engine sizes** (`b67fd17`):

| Engine                         | Raw         | gzip -9     | Brotli 11   |
| ------------------------------ | ----------- | ----------- | ----------- |
| WebGL (`engine-webgl_bg.wasm`) | 5,732,091 B | 2,099,872 B | 1,530,006 B |
| WebGPU (`engine_bg.wasm`)      | 3,516,978 B | 1,284,733 B | 959,036 B   |

**Baseline dependency graph** (`cargo tree -p sloppy-web --no-default-features
--features webgl --target wasm32-unknown-unknown -e normal`): the WebGL engine has 98
crates, the WebGPU engine 80. Only the WebGL engine has wgpu-core, wgpu-core-deps-wasm,
wgpu-hal, wgpu-naga-bridge, glow, naga, slotmap, parking_lot(_core), lock_api,
scopeguard, bit-set, bit-vec, codespan-reporting, unicode-width, half and
zerocopy(-derive). Both have wgpu and wgpu-types. The direct backend should end with
glow + naga (wgsl-in, glsl-out) and none of wgpu, wgpu-core, wgpu-hal or wgpu-types in
the WebGL graph; verify with the same `cargo tree` and `grep wgpu`.

**Baseline WebGL calls** (stepped overview frames, seed 12345, 640×360, counted with
`scripts/webgl-render-ab.mjs` and `AB_COUNT_CALLS=1`, 10 frames after 5 of warm-up; call
counts do not depend on the GPU's speed):

| Scene   | Draw calls (all passes) | Shadow / reflection | WebGL calls per frame | Wasm memory |
| ------- | ----------------------- | ------------------- | --------------------- | ----------- |
| Village | 1,147                   | 61 / 469            | 13,893                | 185.4 MiB   |
| Harbor  | 609                     | 53 / 248            | 10,570                | 138.8 MiB   |

Village's largest: `texParameteri` 3,181, `bindSampler` 1,635, `bindTexture` 1,598,
`activeTexture` 1,590, `uniform1ui` 1,147, `drawElementsInstanced` 1,146, `bindBuffer`
589, `enable`/`disableVertexAttribArray` 479/294, `vertexAttribDivisor` 479,
`vertexAttribPointer` 412, `bindBufferRange` 322. Stock wgpu-hal re-binds every
texture, sampler and texture parameter of a bind group on each set, about 12 calls per
draw. PR 60 reported 1,797 calls per village frame with its patched state cache (other
camera), so a direct backend with a state cache should land near that or below
(roughly a first-instance uniform and a draw per draw, plus real state changes). Since
the patch's call reduction saved 14-24% render CPU, the extra gain must come from
dropping wgpu-core's recording, tracking and validation and wgpu-hal's command replay:
the baseline profile in step 1 tells how large that share is.

## Where wgpu is used today (`crates/render/src/gpu/`)

| File                            | wgpu-specific part                                                                                                                                                | Shared part to keep                                                                                                                                                     |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `context.rs`                    | instance, adapter, device, surface, `ColorTarget` attachments                                                                                                     | `ErrorSlot`, `GRAPHICS_API`                                                                                                                                             |
| `resources.rs`                  | bind group layouts, page buffers and their writes/binding, material uniform buffer + bind group                                                                   | `MeshStore` bookkeeping on `PagePlanner`, shared/owned meshes, users and collection; `MaterialStore` interning, users, refresh-on-texture-generation; `MaterialUniform` |
| `pipelines.rs`, `precompile.rs` | pipelines, async precompile (WebGPU only)                                                                                                                         | key cache, `rank`, fixed pipeline set                                                                                                                                   |
| `textures.rs`                   | texture creation, `copy_external_image_to_texture`, mip blits, samplers                                                                                           | fetch/decode, generated `ImageData`, requests, states, failures, generation counter                                                                                     |
| `instance_store.rs`             | storage buffer (WebGPU) / RGBA32F texture (WebGL)                                                                                                                 | capacity, record layout                                                                                                                                                 |
| `pools.rs`                      | per-view frame bind groups                                                                                                                                        | everything else                                                                                                                                                         |
| `depth_copy.rs`                 | WebGL-only full-screen depth write                                                                                                                                | — (replace with a depth blit, below)                                                                                                                                    |
| `mod.rs`                        | `Renderer::new`, `resize`, `set_sun_shadow`, `set_water`, `warm_up` submit, `render` submit/present, `encode_scene`, `encode_output`, `DrawContext`, `BoundPages` | classes, models, instances, slabs, static records, culling, `build_draws`, merged shadow grouping, class order (`GROUP_DRAWS_BY_STATE`), stats                          |

## Proposed architecture

- Keep one `Renderer` with all shared logic in `gpu/mod.rs`. Move the wgpu code into
  `gpu/webgpu/` and add `gpu/webgl/`, selected with `#[cfg]` (`use webgl as backend`).
  No trait and no wgpu-like API: both modules expose the same few concrete types and
  functions that the shared code calls (a GPU handle, page buffers, material bindings,
  instance stores, targets, pipelines/programs, and one "draw this frame" entry that
  takes the built draw lists). Move WebGPU code verbatim where possible so its behavior
  and performance stay identical; verify with the WebGPU checks.
- Cargo: make `wgpu` optional (`webgpu = ["dep:wgpu", "wgpu/webgpu"]`) and add
  `webgl = ["dep:glow", "dep:naga"]` to `sloppy-render`, naga with `wgsl-in` and
  `glsl-out`. Drop the `opt-level = 3` overrides for wgpu-core and wgpu-hal in the root
  `Cargo.toml`; measure glow at "s"/"z"/3 (it is a thin layer over web-sys). naga only
  runs when programs are built, so "z" is fine. `serverBuild` hashes the root manifest.
- Shaders stay WGSL. Translate with naga exactly as the existing `shader::webgl_check`
  test does (one shared function for the test and the backend): `Version::Embedded {
version: 300, is_webgl: true }`, `WriterFlags::ADJUST_COORDINATE_SPACE |
FORCE_POINT_SIZE` (what wgpu-hal uses; no `DRAW_PARAMETERS`).
- Cache GL programs by `ShaderKey` (+ entry points), not by `PipelineKey`: most keys
  differ only in fixed-function state. A "pipeline" is then a program plus a small
  state record (blend, depth test/write/func, cull side, polygon offset, alpha to
  coverage) that the state cache applies field by field.

## GL details found while reading wgpu-hal 30 and the renderer

1. **Bindings.** ES 3.00 has no `layout(binding=)`. After linking, for every name in
   naga's `ReflectionInfo::uniforms` call `getUniformBlockIndex` + `uniformBlockBinding`
   (block names differ per stage, so bind each), and for every key of
   `texture_mapping` set its texture unit with `uniform1i` (wgpu-hal
   `gles/device.rs::create_program`). Give each `(group, binding)` texture a fixed unit
   (frame: shadow map, DFG LUT, instance records; material: map, bump, emissive, extra 0,
   extra 1; water: normals, reflection; output: HDR) so material switches touch only
   their units. Programs with only a vertex stage need an empty fragment shader
   (`#version 300 es\nvoid main(void) {}`), as wgpu-hal adds.
2. **First instance.** WebGL2 has no base instance. naga emits `uniform uint
naga_vs_first_instance` (`naga::back::glsl::FIRST_INSTANCE_BINDING`) and adds it to
   `gl_InstanceID`; set it with `uniform1ui` before each draw whose first instance
   differs from the program's current value (uniform values are per program). The
   merged shadow casters also read an instance-rate `base` attribute (location 2,
   `u32`, from `shadow_base_buffer`): re-point it per draw with
   `vertexAttribIPointer(2, 1, UNSIGNED_INT, 4, first * 4)`.
3. **Coordinates.** `ADJUST_COORDINATE_SPACE` flips clip-space Y and maps WebGPU depth
   0..1 to GL's -1..1, so offscreen targets hold rows in WebGPU order, sampling and
   `gl_FragCoord.y` match WebGPU, and stored depth equals WebGPU's (the shadow matrix
   keeps working). The flip reverses winding, so wgpu-hal swaps the front face
   (`FrontFace::Ccw` becomes `glow::CW`, `gles/conv.rs`); do the same or culling and
   `front_facing` invert. The canvas's default framebuffer is bottom-up: wgpu-hal
   renders into its own surface texture and blits it flipped at present. Cheaper: draw
   the output pass straight into the canvas and read the HDR row from the bottom
   (`output.wgsl` reads `textureLoad(hdr, vec2i(position.xy), 0)`; add a flip flag to
   the unused part of its `Output` uniform, 0 on WebGPU), and check orientation with a
   screenshot.
4. **Cached fixed-scenery shadow.** WebGL2 can copy depth with
   `blitFramebuffer(..., DEPTH_BUFFER_BIT, NEAREST)` between same-format depth
   attachments, which wgpu's GL backend could not; that replaces `depth_copy.rs` and
   `depth_copy.wgsl`. Verify on ANGLE (Metal, D3D11) and Firefox.
5. **Texture completeness.** The instance texture (RGBA32F, `texelFetch`) and HDR
   texture need NEAREST filters and `TEXTURE_MAX_LEVEL` 0 set on the texture, and no
   filtering sampler on their units. A depth texture read as a float texture needs
   NEAREST and compare mode NONE; the shadow map's comparison sampler sets
   `TEXTURE_COMPARE_MODE = COMPARE_REF_TO_TEXTURE` and `LEQUAL`.
6. **Targets.** Request the context with `antialias: false` (copy the other attributes
   from wgpu-hal `gles/web.rs::create_context_options`). Main view and reflection: 4×
   RGBA16F and DEPTH_COMPONENT32F renderbuffers, resolved into an RGBA16F texture with
   `blitFramebuffer`, then `invalidateFramebuffer` for the discarded attachments.
   Rendering to RGBA16F needs `EXT_color_buffer_float` (or `_half_float`); report its
   absence as "WebGL canvas unavailable: ..." so the page's error path recognizes it.
   Destroy superseded attachments on resize.
7. **Uploads.** Mesh pages: `bufferData(size)` once (no CPU zero vector), then
   `bufferSubData` per write; draws need no "written prefix" binding on GL, but keep
   the planner's prefix logic for WebGPU. Instance records: `texSubImage2D` of up to
   three RGBA32F rectangles, as `instance_store.rs` does. Textures:
   `UNPACK_FLIP_Y_WEBGL` for `ImageData` that flips (bitmaps are flipped at decode,
   `FLIP_BITMAPS_ON_DECODE`), premultiply off, colorspace conversion NONE; GL uploads
   run at once, so bitmaps can close right after (no `queue.submit([])` dance). Keep the
   shader mip chain through sRGB framebuffers for parity, or prove `generateMipmap`
   identical. Uniforms: material uniforms can share one UBO arena bound with
   `bindBufferRange` (offsets aligned to `UNIFORM_BUFFER_OFFSET_ALIGNMENT`).
8. **Fixed-function mapping** (match wgpu-hal for parity with the baseline): Normal
   blend `blendFuncSeparate(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, ONE, ONE_MINUS_SRC_ALPHA)`,
   Additive `(SRC_ALPHA, ONE, ONE, ONE)`; depth bias `polygonOffset(slope_scale,
constant)` with `POLYGON_OFFSET_FILL`; `SAMPLE_ALPHA_TO_COVERAGE`; `Side::Front`
   culls back faces, `Back` culls front, `Double` disables culling.
9. **State cache.** Track program, VAO, array/element/uniform buffer bindings, UBO
   ranges per index, active unit, texture and sampler per unit, enables (depth test,
   cull, blend, polygon offset, alpha to coverage), depth func/mask, color mask, blend
   funcs, cull face, front face, polygon offset, viewport, draw/read framebuffers and
   each program's first-instance value. Invalidate entries when an upload or a
   texture/buffer creation binds something, when a resource is deleted, around
   framebuffer changes and clears (clears obey the color/depth masks), and after
   context restoration. A VAO per vertex page (with its index page bound, since the
   element buffer is VAO state) makes a page switch one call.
10. **Errors.** Shader compile and link logs, `checkFramebufferStatus` when targets are
    built, `webglcontextlost` into the existing `ErrorSlot` ("GPU device lost: ...
    Reload to restart."), and `getError` only at a low rate (it is a synchronous call).
    `webgl-check.mjs` already turns Chrome's GL console warnings into failures.
11. **Compiles.** wgpu's GL backend compiles synchronously. `KHR_parallel_shader_compile`
    (`COMPLETION_STATUS_KHR`) can make `prepare_step` non-blocking on WebGL, the
    counterpart of `precompile.rs`; optional, measure startup if added.

## Plan

1. **Baseline evidence** (before any broad refactor). Two worktrees: this branch's
   base (`b67fd17`) and the candidate, each with `pnpm install`, `pnpm run wasm` and its
   own Vite (`pnpm exec vite --host 127.0.0.1 --port 5174 --strictPort`). Run
   `scripts/webgl-render-ab.mjs` with Village, Harbor and Stress Grid, ABBA rounds,
   fixed seed, viewport, overview camera and warm-up; keep every sample and outlier,
   record Chrome version, GPU (`UNMASKED_RENDERER_WEBGL`), OS and power state. Also
   count GL calls per frame (`AB_COUNT_CALLS=1`) and profile the baseline with Wasm
   names (`CARGO_PROFILE_RELEASE_STRIP=false`, `AB_PROFILE=dir`) to see how render time
   splits between wgpu-core, wgpu-hal, glow/web-sys and `sloppy_render`.
2. **Prototype.** The smallest direct path that draws those three scenes with
   identical draw lists: context, programs from naga, page buffers and VAOs, instance
   texture, frame/material uniforms, shadow (with the cached fixed shadow), reflection,
   main and output passes, and the state cache. The harness records draw, shadow and
   reflection draw counts per frame: they must equal the baseline's. Compare render
   CPU before expanding; stop and report if the gain is small.
3. **Complete the backend**: textures and mips, generated `ImageData`, pools,
   transparency order (two-pass double-sided), water, resize, reset/respawn ownership,
   stats (`gpu_bytes`, buffers, `graphicsApi`), labs build, errors and context loss,
   warm-up/prepare semantics (no late pipelines), and remove wgpu from the WebGL graph.
4. **Validate** (see the request): `pnpm run check`, `check:browser` on WebGPU and with
   `?webgl`, `webgl-check.mjs`, fixtures (maps, reinforcements, destruction), Stress
   Grid and Scrap Yard, still-frame parity against the baseline WebGL engine and
   WebGPU, camera movement, repeated resets, resizes, Wasm memory after each map,
   multiplayer with `?webgl`, touch checks, Chrome, Firefox and WebKit.
5. **Report**: render CPU, frame-time distribution, GL calls, Wasm memory, compressed
   engine sizes, cold and warm build costs (say whether Kache was on), added/removed/
   moved lines, and whether the backend earns its maintenance.
