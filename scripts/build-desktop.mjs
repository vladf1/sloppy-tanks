// Build the desktop app (crates/desktop, binary `sloppy-tanks`) into the ignored
// `artifacts/desktop/`, or the same app for iPad into `artifacts/ipad/`.
//
//   node scripts/build-desktop.mjs --mac       # Sloppy Tanks.app for Apple silicon, a zip and a .pkg
//   node scripts/build-desktop.mjs --windows   # sloppy-tanks.exe (on a Windows host)
//   node scripts/build-desktop.mjs             # whichever this machine is
//   node scripts/build-desktop.mjs --ipad              # Sloppy Tanks.app and an .ipa for iPads
//   node scripts/build-desktop.mjs --ipad-simulator    # the app for Xcode's iPad simulators
//
// Add `--install` to an iPad build to install and open it on the first connected iPad
// (`SLOPPY_IOS_DEVICE` names another device) or on a booted iPad simulator (one is
// booted when none is).
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
//
// An iPad build for devices is signed with an "Apple Development" identity and the
// provisioning profile Xcode keeps for it whose app ID covers the bundle identifier
// (the team's wildcard profile does); `SLOPPY_IOS_PROFILE` names a profile file and
// `SLOPPY_IOS_IDENTITY` an identity instead. Without them the app is signed ad hoc and
// installs on no device. Simulator builds need no signing.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";
import { releaseVersion } from "./release-version.mjs";
import { verifyDesktopIcon } from "./generate-desktop-icon.mjs";

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
// The iPad app needs Metal and the landscape full-screen window iPadOS 16 gives every
// iPad since 2017.
const MINIMUM_IPADOS = "16.0";
const IPAD_TARGET = "aarch64-apple-ios";
const IPAD_SIMULATOR_TARGET = "aarch64-apple-ios-sim";
// The launch screen's colour, Battle Setup's backdrop (`theme::BACKDROP`).
const LAUNCH_BACKGROUND = [0x0a, 0x15, 0x27];

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
function build(target, env = {}) {
  run(
    "cargo",
    [
      "build",
      "--locked",
      "--timings",
      "--profile",
      PROFILE,
      "--target",
      target,
      "-p",
      "sloppy-desktop",
    ],
    { env: { ...process.env, ...stamps, ...env } },
  );
  const extension = target.includes("windows") ? ".exe" : "";
  return join(root, "target", target, PROFILE, BINARY + extension);
}

function megabytes(path) {
  return `${(statSync(path).size / 1e6).toFixed(1)} MB`;
}

/** A property list of `entries`: strings, booleans, integers, arrays and objects. */
function plist(entries) {
  const escape = (text) =>
    text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  const element = (value, indent) => {
    if (typeof value === "boolean") return `<${value}/>`;
    if (typeof value === "number") return `<integer>${value}</integer>`;
    if (typeof value === "string") return `<string>${escape(value)}</string>`;
    const inner = `${indent}\t`;
    if (Array.isArray(value)) {
      const items = value.map((item) => `${inner}${element(item, inner)}`).join("\n");
      return `<array>\n${items}\n${indent}</array>`;
    }
    return `<dict>\n${dictionary(value, inner)}\n${indent}</dict>`;
  };
  const dictionary = (object, indent) =>
    Object.entries(object)
      .map(([key, value]) => `${indent}<key>${key}</key>\n${indent}${element(value, indent)}`)
      .join("\n");
  return (
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n' +
    `<plist version="1.0">\n<dict>\n${dictionary(entries, "\t")}\n</dict>\n</plist>\n`
  );
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
    console.error("The macOS app needs a Mac (codesign, productbuild, notarytool).");
    process.exit(1);
  }
  let appIcon;
  try {
    appIcon = verifyDesktopIcon();
  } catch (error) {
    console.error(error.message);
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
  copyFileSync(appIcon, join(contents, "Resources", "AppIcon.icns"));
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

/** A provisioning profile's fields, or null when it cannot be read. */
function readProfile(file, scratch) {
  const decoded = join(scratch, "profile.plist");
  if (!attempt("security", ["cms", "-D", "-i", file, "-o", decoded]).ok) return null;
  const field = (key, format = "raw") =>
    capture("plutil", ["-extract", key, format, "-o", "-", decoded]);
  const certificates = [];
  for (let index = 0; ; index += 1) {
    const certificate = field(`DeveloperCertificates.${index}`);
    if (!certificate) break;
    certificates.push(
      createHash("sha1").update(Buffer.from(certificate, "base64")).digest("hex").toUpperCase(),
    );
  }
  return {
    file,
    name: field("Name"),
    appId: field("Entitlements.application-identifier"),
    team: field("TeamIdentifier.0"),
    platforms: field("Platform", "json"),
    expires: new Date(field("ExpirationDate")),
    debuggable: field("Entitlements.get-task-allow") === "true",
    // App Store profiles list no devices and install on none.
    devices: field("ProvisionedDevices", "json") !== "",
    certificates,
  };
}

/** The keychain's code-signing identities: SHA-1 and name. */
function signingIdentities() {
  const listed = capture("security", ["find-identity", "-v", "-p", "codesigning"]);
  return [...listed.matchAll(/\) ([0-9A-F]{40}) "([^"]+)"/g)].map(([, hash, name]) => ({
    hash,
    name,
  }));
}

/** The profile and identity that sign the iPad app for devices (see the top of the
 * file), or null. */
function iosSigning(scratch) {
  const identities = signingIdentities().filter(({ name }) =>
    process.env.SLOPPY_IOS_IDENTITY
      ? name === process.env.SLOPPY_IOS_IDENTITY || name.startsWith(process.env.SLOPPY_IOS_IDENTITY)
      : /^(Apple|iPhone) Development: /.test(name),
  );
  const home = process.env.HOME ?? "";
  const folders = [
    join(home, "Library/Developer/Xcode/UserData/Provisioning Profiles"),
    join(home, "Library/MobileDevice/Provisioning Profiles"),
  ];
  const files = process.env.SLOPPY_IOS_PROFILE
    ? [process.env.SLOPPY_IOS_PROFILE]
    : folders.flatMap((folder) =>
        existsSync(folder)
          ? readdirSync(folder)
              .filter((name) => name.endsWith(".mobileprovision"))
              .map((name) => join(folder, name))
          : [],
      );
  const candidates = [];
  for (const file of files) {
    const profile = readProfile(file, scratch);
    if (!profile || !profile.platforms.includes("iOS") || !profile.devices) continue;
    if (!(profile.expires > new Date())) continue;
    const wanted = `${profile.team}.${BUNDLE_ID}`;
    const covers =
      profile.appId === wanted ||
      (profile.appId.endsWith("*") && wanted.startsWith(profile.appId.slice(0, -1)));
    const identity = identities.find(({ hash }) => profile.certificates.includes(hash));
    if (covers && identity) candidates.push({ profile, identity, exact: profile.appId === wanted });
  }
  // An app ID of its own beats a wildcard; then the longest-lived profile.
  candidates.sort(
    (a, b) => Number(b.exact) - Number(a.exact) || b.profile.expires - a.profile.expires,
  );
  return candidates[0] ?? null;
}

/** The app icon and launch colour as an asset catalog, compiled into `app` by actool;
 * returns the Info.plist entries actool wrote for them. */
function compileAssets(app, scratch, platform) {
  const catalog = join(scratch, "Assets.xcassets");
  const info = { info: { author: "xcode", version: 1 } };
  const iconSet = join(catalog, "AppIcon.appiconset");
  mkdirSync(iconSet, { recursive: true });
  writeFileSync(join(catalog, "Contents.json"), JSON.stringify(info));
  // iPadOS draws every size from one 1024-point square, the web app's artwork scaled up.
  run("sips", [
    ...["-z", "1024", "1024", join(root, "public/icons/icon-512.png")],
    ...["--out", join(iconSet, "icon-1024.png")],
  ]);
  writeFileSync(
    join(iconSet, "Contents.json"),
    JSON.stringify({
      ...info,
      images: [
        { filename: "icon-1024.png", idiom: "universal", platform: "ios", size: "1024x1024" },
      ],
    }),
  );
  const colorSet = join(catalog, "LaunchBackground.colorset");
  mkdirSync(colorSet, { recursive: true });
  const [red, green, blue] = LAUNCH_BACKGROUND.map((channel) => `0x${channel.toString(16)}`);
  writeFileSync(
    join(colorSet, "Contents.json"),
    JSON.stringify({
      ...info,
      colors: [
        {
          idiom: "universal",
          color: { "color-space": "srgb", components: { red, green, blue, alpha: "1.000" } },
        },
      ],
    }),
  );
  const partial = join(scratch, "assets-info.plist");
  run("xcrun", [
    ...["actool", catalog, "--compile", app, "--platform", platform],
    ...["--minimum-deployment-target", MINIMUM_IPADOS, "--target-device", "ipad"],
    ...["--app-icon", "AppIcon", "--output-partial-info-plist", partial],
    ...["--output-format", "human-readable-text", "--notices", "--warnings"],
  ]);
  return JSON.parse(capture("plutil", ["-convert", "json", "-o", "-", partial]) || "{}");
}

function buildIpad(simulator) {
  if (process.platform !== "darwin") {
    console.error("The iPad app builds on a Mac with Xcode (actool, codesign).");
    process.exit(1);
  }
  const target = simulator ? IPAD_SIMULATOR_TARGET : IPAD_TARGET;
  const platform = simulator ? "iphonesimulator" : "iphoneos";
  ensureTargets([target]);
  const binary = build(target, { IPHONEOS_DEPLOYMENT_TARGET: MINIMUM_IPADOS });

  const scratch = join(
    root,
    "target",
    PROFILE,
    simulator ? "ipad-simulator-bundle" : "ipad-bundle",
  );
  rmSync(scratch, { recursive: true, force: true });
  const app = join(scratch, `${APP_NAME}.app`);
  mkdirSync(app, { recursive: true });
  copyFileSync(binary, join(app, BINARY));
  const assets = compileAssets(app, scratch, platform);
  const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const landscape = ["UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"];
  writeFileSync(
    join(app, "Info.plist"),
    plist({
      CFBundleDevelopmentRegion: "en",
      CFBundleDisplayName: APP_NAME,
      CFBundleExecutable: BINARY,
      CFBundleIdentifier: BUNDLE_ID,
      CFBundleInfoDictionaryVersion: "6.0",
      CFBundleName: APP_NAME,
      CFBundlePackageType: "APPL",
      CFBundleShortVersionString: version,
      CFBundleVersion: stamps.SLOPPY_RELEASE,
      CFBundleSupportedPlatforms: [simulator ? "iPhoneSimulator" : "iPhoneOS"],
      LSApplicationCategoryType: "public.app-category.action-games",
      LSRequiresIPhoneOS: true,
      MinimumOSVersion: MINIMUM_IPADOS,
      // iPad only: the menus and HUD need a landscape screen of 1024 points or more.
      UIDeviceFamily: [2],
      UIRequiredDeviceCapabilities: ["arm64", "metal"],
      UIRequiresFullScreen: true,
      UISupportedInterfaceOrientations: landscape,
      "UISupportedInterfaceOrientations~ipad": landscape,
      UIStatusBarHidden: true,
      UIApplicationSupportsIndirectInputEvents: true,
      UILaunchScreen: { UIColorName: "LaunchBackground" },
      // Apps built with the iOS 27 SDK launch only with the scene life cycle; the app
      // puts winit's window into this one scene (crates/desktop/src/ios.rs).
      UIApplicationSceneManifest: {
        UIApplicationSupportsMultipleScenes: false,
        UISceneConfigurations: {
          UIWindowSceneSessionRoleApplication: [{ UISceneConfigurationName: "Default" }],
        },
      },
      // TLS to the game servers is the only encryption, which export rules exempt.
      ITSAppUsesNonExemptEncryption: false,
      ...assets,
    }),
  );
  writeFileSync(join(app, "PkgInfo"), "APPL????");

  const signing = simulator ? null : iosSigning(scratch);
  if (signing) {
    const { profile, identity } = signing;
    copyFileSync(profile.file, join(app, "embedded.mobileprovision"));
    const entitlements = join(scratch, "entitlements.plist");
    writeFileSync(
      entitlements,
      plist({
        "application-identifier": `${profile.team}.${BUNDLE_ID}`,
        "com.apple.developer.team-identifier": profile.team,
        "get-task-allow": profile.debuggable,
      }),
    );
    console.log(`Signing with ${identity.name} and the profile "${profile.name}".`);
    run("codesign", [
      ...["--force", "--sign", identity.hash, "--entitlements", entitlements],
      ...["--generate-entitlement-der", app],
    ]);
    run("codesign", ["--verify", "--strict", "--verbose=2", app]);
  } else {
    if (!simulator) {
      console.warn(
        "No Apple Development identity with a provisioning profile for " +
          `${BUNDLE_ID}: the app is signed ad hoc and installs on no iPad.`,
      );
    }
    run("codesign", ["--force", "--sign", "-", app]);
  }

  const folder = join(root, simulator ? "artifacts/ipad-simulator" : "artifacts/ipad");
  mkdirSync(folder, { recursive: true });
  const built = join(folder, `${APP_NAME}.app`);
  rmSync(built, { recursive: true, force: true });
  run("ditto", [app, built]);
  console.log(`Built ${built} (${megabytes(join(app, BINARY))})`);
  const artifact = join(
    folder,
    `${APP_NAME.replaceAll(" ", "-")}-${stamps.SLOPPY_RELEASE}-iPad${simulator ? "-Simulator" : ""}`,
  );
  if (simulator) {
    const zip = `${artifact}.zip`;
    rmSync(zip, { force: true });
    run("ditto", ["-c", "-k", "--keepParent", app, zip]);
    console.log(`Built ${zip} (${megabytes(zip)})`);
  } else {
    // An .ipa is the app in a Payload folder, zipped: what Apple Configurator, Xcode's
    // Devices window and over-the-air installs take.
    const payload = join(scratch, "ipa");
    mkdirSync(join(payload, "Payload"), { recursive: true });
    run("ditto", [app, join(payload, "Payload", `${APP_NAME}.app`)]);
    const ipa = `${artifact}.ipa`;
    rmSync(ipa, { force: true });
    run("ditto", ["-c", "-k", "--norsrc", "--keepParent", join(payload, "Payload"), ipa]);
    console.log(`Built ${ipa} (${megabytes(ipa)})`);
  }
  if (process.argv.includes("--install")) {
    if (simulator) installOnSimulator(built);
    else installOnIpad(built);
  }
}

/** Install `app` on the booted iPad simulator (booting one if none is) and open it. */
function installOnSimulator(app) {
  const listed = JSON.parse(capture("xcrun", ["simctl", "list", "devices", "available", "--json"]));
  const ipads = Object.values(listed.devices ?? {})
    .flat()
    .filter((device) => device.name.startsWith("iPad"));
  let device = ipads.find((candidate) => candidate.state === "Booted");
  if (!device) {
    device = ipads[0];
    if (!device) {
      console.error("No iPad simulator: add one in Xcode's Devices and Simulators window.");
      process.exit(1);
    }
    run("xcrun", ["simctl", "boot", device.udid]);
  }
  run("xcrun", ["simctl", "install", device.udid, app]);
  run("xcrun", ["simctl", "launch", device.udid, BUNDLE_ID]);
  console.log(`Opened on ${device.name} (simulator).`);
}

/** Install `app` on the first connected iPad, or the device `SLOPPY_IOS_DEVICE` names,
 * and open it. */
function installOnIpad(app) {
  let device = process.env.SLOPPY_IOS_DEVICE;
  if (!device) {
    const listing = join(root, "target", PROFILE, "ipad-bundle", "devices.json");
    run("xcrun", ["devicectl", "list", "devices", "--quiet", "--json-output", listing]);
    const devices = JSON.parse(readFileSync(listing, "utf8")).result?.devices ?? [];
    const ipad = devices.find(
      (candidate) =>
        candidate.hardwareProperties?.deviceType === "iPad" &&
        candidate.hardwareProperties?.reality === "physical" &&
        candidate.connectionProperties?.pairingState === "paired",
    );
    if (!ipad) {
      console.error(
        "No paired iPad is connected: set SLOPPY_IOS_DEVICE to a device's name or UDID.",
      );
      process.exit(1);
    }
    device = ipad.hardwareProperties.udid;
  }
  run("xcrun", ["devicectl", "device", "install", "app", "--device", device, app]);
  run("xcrun", ["devicectl", "device", "process", "launch", "--device", device, BUNDLE_ID]);
}

const mac = process.argv.includes("--mac");
const windows = process.argv.includes("--windows");
const ipad = process.argv.includes("--ipad");
const ipadSimulator = process.argv.includes("--ipad-simulator");
if (ipad || ipadSimulator) buildIpad(ipadSimulator);
else if (mac || (!windows && process.platform === "darwin")) buildMac();
else if (windows || process.platform === "win32") buildWindows();
else {
  console.error("Desktop builds are for macOS and Windows: pass --mac or --windows.");
  process.exit(1);
}
