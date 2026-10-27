// Build the desktop app (crates/desktop, binary `sloppy-tanks`) into the ignored
// `artifacts/desktop/`.
//
//   node scripts/build-desktop.mjs --mac       # universal Sloppy Tanks.app, a zip and a .pkg
//   node scripts/build-desktop.mjs --windows   # sloppy-tanks.exe (on a Windows host)
//   node scripts/build-desktop.mjs             # whichever this machine is
//
// Like the browser engine and the server, the app is stamped with the content version
// multiplayer joins send (`SLOPPY_CONTENT_VERSION`), so only a build through this script
// can join the servers. It also gets the release version (`SLOPPY_RELEASE`, with CI's
// `SLOPPY_BUILD_NUMBER` as its fourth part) and the commit (`SLOPPY_COMMIT`) for Battle
// Setup's build line. The `desktop` profile (Cargo.toml) optimizes everything for speed.
//
// The macOS app runs natively on Apple silicon and Intel: both targets are built and
// joined with `lipo`. It is unsigned apart from an ad-hoc signature, which arm64 needs to
// run at all, and the installer package beside its zip is unsigned. FMA stays off on both
// (no `target-cpu=native`): fused multiply-adds round differently from the server and the
// browser's Wasm.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";
import { releaseVersion } from "./release-version.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const output = join(root, "artifacts/desktop");
const PROFILE = "desktop";
const BINARY = "sloppy-tanks";
const APP_NAME = "Sloppy Tanks";
const BUNDLE_ID = "me.fridman.sloppy-tanks";
// wgpu's Metal backend and winit support macOS 10.13 and up; 11 (Big Sur) is the first
// release that runs on Apple silicon, and older ones no longer get Xcode or security fixes.
const MINIMUM_MACOS = "11.0";
const MAC_TARGETS = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const WINDOWS_TARGET = "x86_64-pc-windows-msvc";

/** Run `command` from the repository root and exit the process unless it succeeds. */
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", ...options });
  if (result.error) {
    console.error(`${command}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(`${command} ${args.join(" ")} exited with ${result.status}`);
    process.exit(result.status ?? 1);
  }
  return result;
}

/** A command's trimmed output, or "" when it fails (no Git, no rustup). */
function capture(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : "";
}

const commit = capture("git", ["rev-parse", "--short", "HEAD"]) || "unknown";
const dirty = capture("git", ["status", "--porcelain"]) !== "";
const stamps = {
  SLOPPY_CONTENT_VERSION: await contentVersion(),
  SLOPPY_RELEASE: releaseVersion(process.env.SLOPPY_BUILD_NUMBER),
  SLOPPY_COMMIT: dirty ? `${commit}-dirty` : commit,
};

/** Add the rustup targets `rust-toolchain.toml` does not list (it is part of the content
 * version, so the desktop targets stay out of it). */
function ensureTargets(targets) {
  const installed = capture("rustup", ["target", "list", "--installed"]).split("\n");
  const missing = targets.filter((target) => !installed.includes(target));
  if (missing.length) run("rustup", ["target", "add", ...missing]);
}

/** Build the app for `target` and return its executable's path. */
function build(target) {
  run(
    "cargo",
    ["build", "--locked", "--profile", PROFILE, "--target", target, "-p", "sloppy-desktop"],
    { env: { ...process.env, ...stamps } },
  );
  const extension = target.includes("windows") ? ".exe" : "";
  return join(root, "target", target, PROFILE, BINARY + extension);
}

function megabytes(path) {
  return `${(statSync(path).size / 1e6).toFixed(1)} MB`;
}

function plist(entries) {
  const escape = (text) =>
    text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  const values = Object.entries(entries)
    .map(([key, value]) => {
      const element =
        typeof value === "boolean" ? `<${value}/>` : `<string>${escape(value)}</string>`;
      return `\t<key>${key}</key>\n\t${element}`;
    })
    .join("\n");
  return (
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n' +
    `<plist version="1.0">\n<dict>\n${values}\n</dict>\n</plist>\n`
  );
}

/** The app icon from the web app's 512-pixel one, at every size up to it. */
function makeIcns(destination, scratch) {
  const iconset = join(scratch, "AppIcon.iconset");
  mkdirSync(iconset, { recursive: true });
  const source = join(root, "public/icons/icon-512.png");
  for (const size of [16, 32, 128, 256, 512]) {
    for (const scale of [1, 2]) {
      const pixels = size * scale;
      if (pixels > 512) continue;
      const name = `icon_${size}x${size}${scale === 2 ? "@2x" : ""}.png`;
      run("sips", ["-z", `${pixels}`, `${pixels}`, source, "--out", join(iconset, name)], {
        stdio: "ignore",
      });
    }
  }
  run("iconutil", ["--convert", "icns", "--output", destination, iconset]);
}

function buildMac() {
  if (process.platform !== "darwin") {
    console.error("The macOS app needs a Mac (lipo, sips, iconutil, codesign).");
    process.exit(1);
  }
  ensureTargets(MAC_TARGETS);
  const binaries = MAC_TARGETS.map(build);

  const scratch = join(root, "target", PROFILE, "bundle");
  rmSync(scratch, { recursive: true, force: true });
  const app = join(scratch, `${APP_NAME}.app`);
  const contents = join(app, "Contents");
  mkdirSync(join(contents, "MacOS"), { recursive: true });
  mkdirSync(join(contents, "Resources"), { recursive: true });
  const executable = join(contents, "MacOS", BINARY);
  run("lipo", ["-create", "-output", executable, ...binaries]);
  makeIcns(join(contents, "Resources", "AppIcon.icns"), scratch);
  const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  writeFileSync(
    join(contents, "Info.plist"),
    plist({
      CFBundleDevelopmentRegion: "en",
      CFBundleDisplayName: APP_NAME,
      CFBundleExecutable: BINARY,
      CFBundleIconFile: "AppIcon",
      CFBundleIdentifier: BUNDLE_ID,
      CFBundleInfoDictionaryVersion: "6.0",
      CFBundleName: APP_NAME,
      CFBundlePackageType: "APPL",
      CFBundleShortVersionString: version,
      CFBundleVersion: stamps.SLOPPY_RELEASE,
      LSApplicationCategoryType: "public.app-category.action-games",
      LSMinimumSystemVersion: MINIMUM_MACOS,
      NSHighResolutionCapable: true,
      NSSupportsAutomaticGraphicsSwitching: true,
    }),
  );
  writeFileSync(join(contents, "PkgInfo"), "APPL????");
  // Apple silicon refuses unsigned code; an ad-hoc signature (no identity) is enough for
  // a downloaded app the player opens past Gatekeeper.
  run("codesign", ["--force", "--deep", "--sign", "-", app]);

  mkdirSync(output, { recursive: true });
  const artifact = join(output, `${APP_NAME.replaceAll(" ", "-")}-${stamps.SLOPPY_RELEASE}-macOS`);
  const zip = `${artifact}.zip`;
  rmSync(zip, { force: true });
  run("ditto", ["-c", "-k", "--keepParent", app, zip]);
  const installer = `${artifact}.pkg`;
  makeInstaller(app, version, installer, scratch);
  rmSync(join(output, `${APP_NAME}.app`), { recursive: true, force: true });
  run("ditto", [app, join(output, `${APP_NAME}.app`)]);
  console.log(`Built ${join(output, `${APP_NAME}.app`)} (universal, ${megabytes(executable)})`);
  console.log(`Built ${zip} (${megabytes(zip)})`);
  console.log(`Built ${installer} (${megabytes(installer)})`);
}

/** An unsigned installer package that puts `app` in /Applications, titled for Installer. */
function makeInstaller(app, version, destination, scratch) {
  const payload = join(scratch, "pkg-root");
  mkdirSync(payload, { recursive: true });
  run("ditto", [app, join(payload, `${APP_NAME}.app`)]);
  // A relocatable bundle (pkgbuild's default) makes Installer look for the bundle
  // identifier elsewhere on the disk and replace a copy it finds there (in Downloads, say)
  // instead of installing into /Applications; the app always goes where the package says.
  const components = join(scratch, "components.plist");
  run("pkgbuild", ["--analyze", "--root", payload, components]);
  run("plutil", ["-replace", "0.BundleIsRelocatable", "-bool", "NO", components]);
  const component = join(scratch, "SloppyTanks-component.pkg");
  run("pkgbuild", [
    ...["--root", payload, "--component-plist", components],
    ...["--install-location", "/Applications", "--identifier", BUNDLE_ID],
    ...["--version", version, component],
  ]);
  // A distribution around the component package gives Installer the game's name as its
  // window title, installs for the whole machine and refuses releases the app cannot run on.
  const distribution = join(scratch, "distribution.xml");
  run("productbuild", ["--synthesize", "--package", component, distribution]);
  const synthesized = readFileSync(distribution, "utf8");
  const extra =
    `    <title>${APP_NAME}</title>\n` +
    '    <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>\n' +
    `    <allowed-os-versions><os-version min="${MINIMUM_MACOS}"/></allowed-os-versions>\n`;
  const opening = /<installer-gui-script[^>]*>\n/;
  if (!opening.test(synthesized)) {
    console.error(`productbuild --synthesize wrote no <installer-gui-script> in ${distribution}`);
    process.exit(1);
  }
  writeFileSync(
    distribution,
    synthesized.replace(opening, (tag) => tag + extra),
  );
  rmSync(destination, { force: true });
  run("productbuild", [
    ...["--distribution", distribution, "--package-path", scratch],
    destination,
  ]);
}

function buildWindows() {
  if (process.platform !== "win32") {
    console.error("The Windows app builds on Windows (MSVC); CI's desktop workflow makes it.");
    process.exit(1);
  }
  ensureTargets([WINDOWS_TARGET]);
  const executable = build(WINDOWS_TARGET);
  mkdirSync(output, { recursive: true });
  const destination = join(output, `${BINARY}.exe`);
  copyFileSync(executable, destination);
  console.log(`Built ${destination} (${megabytes(destination)})`);
}

const mac = process.argv.includes("--mac");
const windows = process.argv.includes("--windows");
if (mac || (!windows && process.platform === "darwin")) buildMac();
else if (windows || process.platform === "win32") buildWindows();
else {
  console.error("Desktop builds are for macOS and Windows: pass --mac or --windows.");
  process.exit(1);
}
