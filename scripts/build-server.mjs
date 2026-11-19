import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { contentVersion, serverBuild } from "./content-version.mjs";

/** Build the native multiplayer server. `--vps` (or `SLOPPY_SERVER_VPS=1`, which CI sets
 * so its tests drive the binary its image ships) cross-compiles the static Linux x86_64
 * (musl) binary the VPS runs; otherwise it builds for this machine. The server stamps
 * the same content version as the browser build, and a fingerprint of its own inputs,
 * which `/health` reports so deploys and the redeploy check notice changes. */
const repo = fileURLToPath(new URL("..", import.meta.url));
export const VPS_TARGET = "x86_64-unknown-linux-musl";
const vps = process.argv.includes("--vps") || process.env.SLOPPY_SERVER_VPS === "1";
const args = ["build", "--locked", "--profile", "server", "-p", "sloppy-server"];
if (vps) args.push("--target", VPS_TARGET);
const result = spawnSync("cargo", args, {
  cwd: repo,
  stdio: "inherit",
  env: {
    ...process.env,
    ...(vps ? muslCompiler() : {}),
    SLOPPY_CONTENT_VERSION: await contentVersion(),
    SLOPPY_SERVER_BUILD: await serverBuild(),
  },
});
if (result.error) console.error(`cargo: ${result.error.message}. See README.md for Rust setup.`);
if (result.status !== 0) process.exit(result.status ?? 1);
console.log(`Built ${vps ? `target/${VPS_TARGET}/server` : "target/server"}/sloppy-server`);

/** ring, the TLS provider's cryptography, compiles C and assembly for the target. The
 * host's clang (Xcode's on macOS) writes Linux objects without a musl sysroot, since
 * ring needs only the compiler's own headers; rustup's llvm-ar archives them with the
 * ELF symbol index rust-lld needs, which macOS `ar` does not write. */
function muslCompiler() {
  const output = (command, commandArgs) => {
    const run = spawnSync(command, commandArgs, { cwd: repo, encoding: "utf8" });
    if (run.status !== 0) {
      console.error(`${command} ${commandArgs.join(" ")} failed:\n${run.stderr ?? run.error}`);
      process.exit(1);
    }
    return run.stdout.trim();
  };
  const sysroot = output("rustc", ["--print", "sysroot"]);
  const host = output("rustc", ["--print", "host-tuple"]);
  const archiver = `${sysroot}/lib/rustlib/${host}/bin/llvm-ar`;
  if (!existsSync(archiver)) output("rustup", ["component", "add", "llvm-tools"]);
  return {
    CC_x86_64_unknown_linux_musl: "clang",
    AR_x86_64_unknown_linux_musl: archiver,
    // No libc headers: ring's own switch (RING_CORE_NOSTDLIBINC) for building without
    // them, and clang's stddef.h defers to musl's header unless __musl__ is undefined.
    CFLAGS_x86_64_unknown_linux_musl:
      "-ffreestanding -nostdlibinc -U__musl__ -DRING_CORE_NOSTDLIBINC=1",
  };
}
