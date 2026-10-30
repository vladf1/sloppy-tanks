// Manual builder/critic loop evidence. A successful capture means "ready for review",
// never "the critic passed". Keep this out of CI and the normal `check` command.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { delimiter, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { desktopUiSourceHash } from "./desktop-ui-source.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const args = process.argv.slice(2);
const value = (flag, fallback) => {
  const index = args.indexOf(flag);
  return index < 0 ? fallback : args[index + 1];
};
const base = resolve(root, value("--out", "artifacts/performance/desktop-ui"));
const label = value("--round", new Date().toISOString().replaceAll(/[:.]/g, "-"));
assert.match(label, /^[\w.-]+$/, "Round names use letters, digits, dots, underscores or dashes");
const sizes = value("--sizes", "1280x800,1600x900").split(",");
// The reference page: the frozen pre-migration build (desktop-ui-capture.mjs), or another.
const referenceArgs = ["--reference", "--reference-dist"].flatMap((flag) =>
  args.includes(flag) ? [flag, value(flag)] : [],
);
for (const size of sizes) assert.match(size, /^\d{3,4}x\d{3,4}$/);
const output = resolve(base, label);
assert.ok(!existsSync(output), "Choose a fresh --round name; evidence is never overwritten");
await mkdir(output, { recursive: true });
const git = (args) => spawnSync("git", args, { cwd: root, encoding: "utf8" }).stdout;
const sourceHash = await desktopUiSourceHash(root);
const env = {
  ...process.env,
  PATH: ["/opt/homebrew/opt/rustup/bin", resolve(homedir(), ".cargo/bin"), process.env.PATH].join(
    delimiter,
  ),
};
const run = (command, args, extra = {}) => {
  const result = spawnSync(command, args, {
    cwd: root,
    env: { ...env, ...extra },
    stdio: "inherit",
  });
  assert.equal(result.status, 0, `${command} ${args.join(" ")} failed`);
};
const build = spawnSync(
  "cargo",
  [
    "test",
    "--locked",
    "-p",
    "sloppy-desktop",
    "--profile",
    "desktop",
    "--no-run",
    "--message-format=json",
  ],
  { cwd: root, env, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
);
await writeFile(resolve(output, "build.log"), build.stdout + build.stderr);
assert.equal(build.status, 0, `Desktop test build failed; see ${output}/build.log`);
const binary = build.stdout
  .split("\n")
  .filter(Boolean)
  .map((line) => {
    try {
      return JSON.parse(line);
    } catch {
      return {};
    }
  })
  .find(
    (message) =>
      message.reason === "compiler-artifact" &&
      message.target.name === "sloppy-tanks" &&
      message.profile.test &&
      message.executable,
  )?.executable;
assert.ok(binary, "Cargo must report the desktop test executable");
run(binary, []);
const fingerprint = async (directory) => {
  const hash = createHash("sha256");
  for (const name of (await readdir(directory)).filter((name) => name.endsWith(".json")).sort()) {
    hash.update(name);
    hash.update(await readFile(resolve(directory, name)));
  }
  return hash.digest("hex");
};
const reports = [];
for (const size of sizes) {
  const native = resolve(output, size, "native");
  const referenceRoot = resolve(base, "reference", size);
  run(binary, ["ui::snapshots::snapshots", "--ignored", "--exact"], {
    SLOPPY_UI_SNAPSHOTS: native,
    SLOPPY_UI_SIZE: size,
  });
  const fixtures = await fingerprint(native);
  const lock = resolve(referenceRoot, "current.json");
  let reference;
  if (args.includes("--refresh-reference") || !existsSync(lock)) {
    reference = resolve(referenceRoot, label);
    run(process.execPath, [
      "scripts/desktop-ui-capture.mjs",
      "--native",
      native,
      "--out",
      reference,
      ...referenceArgs,
    ]);
    await writeFile(lock, JSON.stringify({ fixtures, directory: reference }, null, 2));
  } else {
    const saved = JSON.parse(await readFile(lock, "utf8"));
    assert.equal(
      saved.fixtures,
      fixtures,
      "Fixtures changed. Review the change and use --refresh-reference to capture the reference's matching states.",
    );
    reference = saved.directory;
  }
  const report = resolve(output, size, "review");
  run(process.execPath, ["scripts/desktop-ui-report.mjs", native, reference, report]);
  reports.push({ size, native, reference, report });
}
assert.equal(
  await desktopUiSourceHash(root),
  sourceHash,
  "Source changed during capture; run a fresh round",
);
await writeFile(
  resolve(output, "round.json"),
  JSON.stringify(
    {
      status: "needs-independent-review",
      createdAt: new Date().toISOString(),
      head: git(["rev-parse", "HEAD"]).trim(),
      sourceHash,
      diffHash: createHash("sha256")
        .update(git(["diff", "HEAD"]))
        .digest("hex"),
      reports,
    },
    null,
    2,
  ),
);
console.log(
  `Evidence ready at ${output}. A separate critic must inspect the A/B captures, record gaps, and require another round after fixes.`,
);
