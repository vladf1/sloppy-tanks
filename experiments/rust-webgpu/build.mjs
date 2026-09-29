import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const cwd = fileURLToPath(new URL(".", import.meta.url));
// Keep generated Rust/JS output out of the game's lint, format and build inputs.
const output = fileURLToPath(
  new URL("../../artifacts/performance/rust-webgpu/build/", import.meta.url),
);
const target = join(output, "target");
mkdirSync(output, { recursive: true });
const commands = process.argv.includes("test")
  ? [["cargo", ["test", "--locked"]]]
  : [
      ["cargo", ["build", "--locked", "--release", "--target", "wasm32-unknown-unknown"]],
      [
        "wasm-bindgen",
        [
          join(target, "wasm32-unknown-unknown/release/rust_webgpu_lab.wasm"),
          "--target",
          "web",
          "--out-dir",
          join(output, "pkg"),
          "--out-name",
          "lab",
        ],
      ],
    ];
for (const [command, args] of commands) {
  const result = spawnSync(command, args, {
    cwd,
    env: { ...process.env, CARGO_TARGET_DIR: target },
    stdio: "inherit",
  });
  if (result.error) console.error(`${command}: ${result.error.message}. See README.md for setup.`);
  if (result.status !== 0) process.exit(result.status ?? 1);
}
