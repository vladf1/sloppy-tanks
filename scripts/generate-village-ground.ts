import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { encodePixelsWebp } from "./encode-webp";

// Pine Village's ground plan (`crates/core/src/models/village_ground.rs`): the Rust
// bake knows the arena layout, roads, cottages and fields, so this only runs it and
// stores the pixels losslessly (every channel is data, not colour):
//
//   node --import tsx scripts/generate-village-ground.ts
const size = 512;
const scratch = mkdtempSync(path.join(tmpdir(), "village-ground-"));
try {
  const raw = path.join(scratch, "plan.rgba");
  execFileSync(
    "cargo",
    ["run", "-q", "-p", "sloppy-core", "--release", "--example", "village_ground", "--", raw],
    { stdio: "inherit" },
  );
  const pixels = new Uint8ClampedArray(readFileSync(raw));
  writeFileSync(
    new URL("../public/textures/ground/village-ground.webp", import.meta.url),
    encodePixelsWebp(pixels, size * 2, size),
  );
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
