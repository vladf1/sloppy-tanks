# Drop Yard — Rust/WebGPU experiment

A standalone browser scene: 24 Rapier rigid bodies, a ramp, a launchable block,
fixed-step physics, an orbit camera, instanced rendering, a directional shadow
map and 4× MSAA. This is a starting point, not a port or performance comparison
of Sloppy Tanks. It imports no game code, Three.js or game assets.

The separate [full rewrite handoff](REWRITE-TASK.md) describes the follow-up task;
this experiment does not implement or initiate that rewrite.

All source lives here. The separate Vite configuration does not participate in
the game's builds or deployments. Generated Wasm, binding glue, Rust caches and
the Vite production build live under the repository's ignored
`artifacts/performance/rust-webgpu/build/` directory.

## Setup

Use the repository's Node/pnpm versions and run `pnpm install` at its root.
Install Rust through [rustup](https://rustup.rs/), then:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

The CLI version must match the `wasm-bindgen` dependency in `Cargo.toml`.
`Cargo.lock` pins the Rust dependency graph. This experiment was built with
Rust 1.98.1, wgpu 30 and Rapier 0.36.

From the repository root:

```sh
pnpm --dir experiments/rust-webgpu dev
```

Open the printed localhost URL (port 5188). WebGPU and a secure context are
required; localhost works. No WebGL fallback or public tunnel is created.
After editing Rust or WGSL, run the following in another terminal and reload:

```sh
pnpm --dir experiments/rust-webgpu wasm
```

Vite reloads HTML, CSS and JavaScript normally. It does not compile Rust on save.

## Controls

- Drag the canvas to orbit; scroll to zoom.
- Launch a block with the button or Space while the canvas is focused.
- Pause/Resume freezes physics while leaving the camera usable.
- Reset scene or R rebuilds the physics world, preserving camera and pause state.
- Launches are capped at 16; the oldest launched body and collider are removed.

## Build and check

```sh
pnpm --dir experiments/rust-webgpu test
pnpm --dir experiments/rust-webgpu build
# With the dev server running:
pnpm --dir experiments/rust-webgpu check:browser
```

The Rust tests verify collisions, refresh-rate-independent stepping, the launch
cap and reset. The headless Chrome smoke check exercises physical button clicks,
keyboard input, orbit/zoom, resize, repeated reset and unavailable-WebGPU handling.
It also rejects GPU/console errors and requests for game/Three.js modules.
Screenshots go to `artifacts/performance/rust-webgpu/`.

To check the production output, start a separate preview server:

```sh
pnpm exec vite preview --config experiments/rust-webgpu/vite.config.mjs --port 5189
RUST_LAB_URL=http://127.0.0.1:5189/ pnpm --dir experiments/rust-webgpu check:browser
```

## Boundaries

- `src/physics.rs`: Rapier world, bounded bodies and 60 Hz accumulator. Display
  frames are clamped to 100 ms to avoid an unbounded catch-up loop after a stall.
  Previous/current poses interpolate for drawing without moving physics bodies.
- `src/renderer.rs`: browser `wgpu` device, camera, fixed-capacity instance buffer,
  shadow/main passes and resource cleanup. Resets reuse GPU resources; resize
  replaces only the size-dependent attachments.
- `src/scene.wgsl`: cube geometry, directional lighting, shadow sampling and grid.
- `main.js`: Wasm loading, browser events, requestAnimationFrame and HTML UI.
  Device/render failures stop the loop and show an error; reload to recover.

This is single-threaded Wasm. There is no networking, audio, game logic, asset
pipeline or native window backend. The physics tests run natively, but the
renderer currently targets a browser canvas only. Frame rate is display cadence,
not a GPU timing benchmark. Physics parity with the game's older Rapier version
is not implied.

## Rapier API size probe

`src/feature_probe.rs` runs a separate tiny world at page startup and exposes
11 observable checks through `window.rustLab.featureChecks`. It exercises the
main game's additional Rapier API families: convex hulls, triangle meshes,
multiple colliders, rotation constraints/soft CCD, filtered rays and normals,
ball/cuboid sweeps, ball overlaps, local hull ray tests, cylinder replacement,
collider offset/group changes, impulses and contact-force events. This keeps
those paths reachable in optimized Wasm without changing the rendered scene.
It is API coverage, not the game's full simulation or a performance benchmark.

`.cargo/config.toml` enables `simd128` for browser builds, matching the main
Rapier package's SIMD capability. Native tests retain their normal target.
