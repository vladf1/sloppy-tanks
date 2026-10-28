// Build the desktop app (crates/desktop, binary `sloppy-tanks`) into the ignored
// `artifacts/desktop/`.
//
//   node scripts/build-desktop.mjs --mac       # Sloppy Tanks.app for Apple silicon, a zip and a .pkg
//   node scripts/build-desktop.mjs --windows   # sloppy-tanks.exe (on a Windows host)
//   node scripts/build-desktop.mjs             # whichever this machine is
//
// Like the browser engine and the server, the app is stamped with the content version
// multiplayer joins send (`SLOPPY_CONTENT_VERSION`), so only a build through this script
// can join the servers. It also gets the release version (`SLOPPY_RELEASE`, with CI's
// `SLOPPY_BUILD_NUMBER` as its fourth part) and the commit (`SLOPPY_COMMIT`) for Battle
// Setup's build line. The `desktop` profile (Cargo.toml) optimizes everything for speed.
//
// The macOS app is built for Apple silicon only; Intel Macs play in the browser. FMA
// stays off (no `target-cpu=native`): fused multiply-adds round differently from the
// server and the browser's Wasm.
//
// Signing follows what the machine holds. With a "Developer ID Application" identity in
// the keychain the app is signed for distribution (hardened runtime, secure timestamp),
// and with a "Developer ID Installer" one the package too; with notarization
// credentials both then go to Apple's notary service and get its ticket stapled, so
// Gatekeeper opens them without a warning, offline too. The credentials are a
// `notarytool` keychain profile (`SLOPPY_NOTARY_PROFILE`, default "sloppy-tanks") or, in
// CI, an App Store Connect API key (`SLOPPY_NOTARY_KEY` file, `SLOPPY_NOTARY_KEY_ID`,
// `SLOPPY_NOTARY_ISSUER`). `SLOPPY_MAC_APP_IDENTITY` and `SLOPPY_MAC_INSTALLER_IDENTITY`
// pick an identity when there are several. Without an identity the app gets only the
// ad-hoc signature arm64 needs to run at all, and the package none.
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
const MAC_TARGET = "aarch64-apple-darwin";
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

/** A command's output and whether it succeeded, without stopping the build. */
function attempt(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  return { ok: result.status === 0, output: `${result.stdout ?? ""}${result.stderr ?? ""}` };
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

/** The app icon in macOS's shape (`crates/desktop/examples/app_icon.rs`: the web app's
 * art in a rounded square with Apple's margin and shadow), at every size. */
function makeIcns(destination, scratch) {
  const iconset = join(scratch, "AppIcon.iconset");
  rmSync(iconset, { recursive: true, force: true });
  run("cargo", [
    "run",
    "--locked",
    "--profile",
    PROFILE,
    "--target",
    MAC_TARGET,
    "-q",
    "-p",
    "sloppy-desktop",
    "--example",
    "app_icon",
    "--",
    iconset,
  ]);
  run("iconutil", ["--convert", "icns", "--output", destination, iconset]);
}

/** The keychain's first identity named `kind` ("Developer ID Application"), or the one
 * `override` names. */
function identity(kind, override, policy) {
  if (override) return override;
  const listed = capture("security", ["find-identity", "-v", ...policy]);
  return listed.match(new RegExp(`"(${kind}: [^"]+)"`))?.[1] ?? null;
}

/** How this machine signs and notarizes the macOS app (see the top of the file). */
function macSigning() {
  const app = identity("Developer ID Application", process.env.SLOPPY_MAC_APP_IDENTITY, [
    "-p",
    "codesigning",
  ]);
  const installer = identity(
    "Developer ID Installer",
    process.env.SLOPPY_MAC_INSTALLER_IDENTITY,
    [],
  );
  const {
    SLOPPY_NOTARY_KEY: key,
    SLOPPY_NOTARY_KEY_ID: keyId,
    SLOPPY_NOTARY_ISSUER: issuer,
  } = process.env;
  let notary = null;
  if (key && keyId && issuer) {
    notary = ["--key", key, "--key-id", keyId, "--issuer", issuer];
  } else {
    const profile = process.env.SLOPPY_NOTARY_PROFILE ?? "sloppy-tanks";
    if (attempt("xcrun", ["notarytool", "history", "--keychain-profile", profile]).ok) {
      notary = ["--keychain-profile", profile];
    }
  }
  return { app, installer, notary: app ? notary : null };
}

/** Send `file` to Apple's notary service, wait for its verdict and staple its ticket to
 * `staple` (the file itself, or the app a zip carried). Stops the build if Apple refuses
 * it, with the service's log. */
function notarize(file, staple, notary) {
  console.log(`Notarizing ${file}…`);
  const submitted = attempt("xcrun", [
    ...["notarytool", "submit", file, ...notary],
    ...["--wait", "--output-format", "json"],
  ]);
  const result = (() => {
    try {
      return JSON.parse(submitted.output.slice(submitted.output.indexOf("{")));
    } catch {
      return {};
    }
  })();
  if (result.status !== "Accepted") {
    console.error(`Notarization of ${file} was not accepted:\n${submitted.output}`);
    if (result.id) run("xcrun", ["notarytool", "log", result.id, ...notary]);
    process.exit(1);
  }
  run("xcrun", ["stapler", "staple", staple]);
}

function buildMac() {
  if (process.platform !== "darwin") {
    console.error("The macOS app needs a Mac (sips, iconutil, codesign).");
    process.exit(1);
  }
  ensureTargets([MAC_TARGET]);
  const binary = build(MAC_TARGET);

  const scratch = join(root, "target", PROFILE, "bundle");
  rmSync(scratch, { recursive: true, force: true });
  const app = join(scratch, `${APP_NAME}.app`);
  const contents = join(app, "Contents");
  mkdirSync(join(contents, "MacOS"), { recursive: true });
  mkdirSync(join(contents, "Resources"), { recursive: true });
  const executable = join(contents, "MacOS", BINARY);
  copyFileSync(binary, executable);
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
  const signing = macSigning();
  if (signing.app) {
    // Distribution signing: the hardened runtime notarization requires, and a secure
    // timestamp so the signature outlives the certificate.
    run("codesign", [
      ...["--force", "--options", "runtime", "--timestamp"],
      ...["--sign", signing.app, app],
    ]);
    run("codesign", ["--verify", "--strict", "--verbose=2", app]);
  } else {
    // Apple silicon refuses unsigned code; an ad-hoc signature (no identity) is enough
    // for an app the player opens past Gatekeeper.
    console.warn("No Developer ID Application identity: the app gets an ad-hoc signature.");
    run("codesign", ["--force", "--deep", "--sign", "-", app]);
  }

  mkdirSync(output, { recursive: true });
  const artifact = join(output, `${APP_NAME.replaceAll(" ", "-")}-${stamps.SLOPPY_RELEASE}-macOS`);
  const zip = `${artifact}.zip`;
  rmSync(zip, { force: true });
  run("ditto", ["-c", "-k", "--keepParent", app, zip]);
  if (signing.notary) {
    // The zip carries the app to the notary; the zip shipped is made again from the
    // stapled app.
    notarize(zip, app, signing.notary);
    rmSync(zip, { force: true });
    run("ditto", ["-c", "-k", "--keepParent", app, zip]);
  } else if (signing.app) {
    console.warn("No notarization credentials: the signed app is not notarized.");
  }
  const installer = `${artifact}.pkg`;
  makeInstaller(app, version, installer, scratch, signing.installer);
  if (signing.notary && signing.installer) {
    notarize(installer, installer, signing.notary);
  }
  rmSync(join(output, `${APP_NAME}.app`), { recursive: true, force: true });
  run("ditto", [app, join(output, `${APP_NAME}.app`)]);
  console.log(`Built ${join(output, `${APP_NAME}.app`)} (Apple silicon, ${megabytes(executable)})`);
  console.log(`Built ${zip} (${megabytes(zip)})`);
  console.log(`Built ${installer} (${megabytes(installer)})`);
}

/** An installer package that puts `app` in /Applications, titled for Installer; signed
 * with `identity` when there is one. */
function makeInstaller(app, version, destination, scratch, identity) {
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
  // window title, installs for the whole machine and refuses Macs and releases the app
  // cannot run on (`--synthesize` allows Intel Macs whatever the app is built for).
  const distribution = join(scratch, "distribution.xml");
  run("productbuild", ["--synthesize", "--package", component, distribution]);
  const synthesized = readFileSync(distribution, "utf8").replace(
    /hostArchitectures="[^"]*"/,
    'hostArchitectures="arm64"',
  );
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
  if (!identity) {
    console.warn("No Developer ID Installer identity: the package is unsigned.");
  }
  run("productbuild", [
    ...["--distribution", distribution, "--package-path", scratch],
    ...(identity ? ["--sign", identity, "--timestamp"] : []),
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
