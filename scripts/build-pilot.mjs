// Build the pilot engine for the bots Worker: `sloppy-pilot`'s library as Wasm (the
// `pilot` profile in Cargo.toml: release without LTO, which builds far faster) into the
// ignored `bots/generated/sloppy_pilot.wasm`, which `bots/pilots-worker.ts` imports and
// `pnpm run pilots:deploy` bundles. The module imports nothing (`crates/pilot/src/abi.rs`),
// so it needs no wasm-bindgen glue.
//
// Like the browser engine it is stamped with the content version (`SLOPPY_CONTENT_VERSION`)
// its joins send: a game server whose `/health` reports another one refuses the pilots,
// so deploy the pilots Worker from the commit the servers it should play on run.
//
// In CI it bypasses Kache (`KACHE_DISABLED`): the server job's Kache store is sized for
// the server build (150 MiB), and the engine's wasm32 objects (about 1 GB) overflowed it,
// so a background GC ran during the store's save and the save failed. rust-cache keeps
// the engine's dependencies in `target/` there instead.
//
// Cargo runs from the repository root so `.cargo/config.toml` (SIMD) applies.
import { spawn } from "node:child_process";
import { copyFile, mkdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const output = fileURLToPath(new URL("../bots/generated/sloppy_pilot.wasm", import.meta.url));
const version = await contentVersion();

await new Promise((resolve, reject) => {
  const child = spawn(
    "cargo",
    [
      "build",
      "--locked",
      "--profile",
      "pilot",
      "--target",
      "wasm32-unknown-unknown",
      "-p",
      "sloppy-pilot",
      "--lib",
    ],
    {
      cwd: root,
      stdio: "inherit",
      env: {
        ...process.env,
        SLOPPY_CONTENT_VERSION: version,
        ...(process.env.CI ? { KACHE_DISABLED: "1" } : {}),
      },
    },
  );
  child.on("error", (error) =>
    reject(
      new Error(`cargo: ${error.message}. Install Rust with the wasm32-unknown-unknown target.`),
    ),
  );
  child.on("exit", (status) =>
    status === 0 ? resolve() : reject(new Error(`cargo exited with ${status}`)),
  );
});
await mkdir(fileURLToPath(new URL("../bots/generated/", import.meta.url)), { recursive: true });
await copyFile(`${root}target/wasm32-unknown-unknown/pilot/sloppy_pilot.wasm`, output);
const { size } = await stat(output);
console.log(
  `Pilot engine: bots/generated/sloppy_pilot.wasm, ${(size / 1024).toFixed(0)} KiB, content ${version}`,
);
