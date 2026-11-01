// Build the desktop app (crates/desktop, binary `sloppy-tanks`) into the ignored
// `artifacts/desktop/`, or the same app for iPhone and iPad into `artifacts/ios/`, and
// either for the App Store into `artifacts/mac-app-store/` or `artifacts/ios-app-store/`.
//
//   node scripts/build-desktop.mjs --mac       # Sloppy Tanks.app for Apple silicon, a zip and a .pkg
//   node scripts/build-desktop.mjs --mac --app-store   # the Mac App Store .pkg for App Store Connect and TestFlight
//   node scripts/build-desktop.mjs --windows   # sloppy-tanks.exe (on a Windows host)
//   node scripts/build-desktop.mjs             # whichever this machine is
//   node scripts/build-desktop.mjs --ios               # Sloppy Tanks.app and an .ipa for iPhones and iPads
//   node scripts/build-desktop.mjs --ios --app-store   # the .ipa for App Store Connect and TestFlight
//   node scripts/build-desktop.mjs --ios-simulator     # the app for Xcode's iPhone and iPad simulators
//
// The iOS app is one bundle for both (`UIDeviceFamily` 1 and 2): an iPhone plays the
// phone edition and an iPad the tablet's (`touch::detect`). Add `--install` to an iOS
// build to install and open it on the iPhone or iPad on a cable (or the only one
// paired), or on a booted simulator of either (an iPhone simulator is booted when none
// is); `--iphone` or `--ipad` takes only that kind, and `SLOPPY_IOS_DEVICE` names a
// device or simulator.
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
// An iOS build for devices is signed with an "Apple Development" identity and the
// provisioning profile Xcode keeps for it whose app ID covers the bundle identifier
// (the team's wildcard profile does); `SLOPPY_IOS_PROFILE` names a profile file and
// `SLOPPY_IOS_IDENTITY` an identity instead. Without them the app is signed ad hoc and
// installs on no device. Simulator builds need no signing.
//
// `--app-store` signs the app for App Store Connect instead, with an "Apple Distribution"
// identity and an App Store provisioning profile (no devices, no debugger): the iOS app
// as an .ipa, the Mac app sandboxed and in a package signed by a "3rd Party Mac Developer
// Installer" identity. It stops without them; `SLOPPY_MAC_PROFILE` names the Mac profile
// as `SLOPPY_IOS_PROFILE` the iOS one. Its build (CFBundleVersion) is
// `SLOPPY_APP_STORE_BUILD`, CI's run number, as App Store Connect takes one to three
// integers that rise with every upload, not the release's four parts. `--upload` then
// sends it to App Store Connect with the API key notarization uses and waits until Apple
// has processed it, when it is in TestFlight.
import { spawnSync } from "node:child_process";
import { createHash, sign } from "node:crypto";
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
// App Store Connect takes an Apple silicon-only Mac app from macOS 12 on (ITMS-90869), the
// first release TestFlight for Mac runs on too.
const MINIMUM_MACOS_APP_STORE = "12.0";
const MAC_TARGET = "aarch64-apple-darwin";
const WINDOWS_TARGET = "x86_64-pc-windows-msvc";
// The iOS app needs Metal and the full-screen window iOS 16 gives every iPhone since
// the 8 and iPadOS 16 every iPad since 2017.
const MINIMUM_IOS = "16.0";
const IOS_TARGET = "aarch64-apple-ios";
const IOS_SIMULATOR_TARGET = "aarch64-apple-ios-sim";
// The launch screen's colour, Battle Setup's backdrop (`theme::BACKDROP`).
const LAUNCH_BACKGROUND = [0x0a, 0x15, 0x27];
// Apple's required-reason APIs, by the names an executable imports or looks up (C
// functions and Foundation constants with their underscore, Objective-C classes and
// selectors without): App Store Connect refuses a build that uses one without a reason
// for its category in the privacy manifest. A category with no reason here stops the
// build until one of Apple's approved reasons is chosen for it.
const REQUIRED_REASON_APIS = [
  {
    category: "NSPrivacyAccessedAPICategoryFileTimestamp",
    // The standard library sizes and stats the app's own files (settings, records).
    reason: "C617.1",
    names: [
      ...["_stat", "_fstat", "_fstatat", "_lstat", "_getattrlist", "_fgetattrlist"],
      ...["_getattrlistat", "_getattrlistbulk", "_NSFileCreationDate", "_NSFileModificationDate"],
      ...["_NSURLContentModificationDateKey", "_NSURLCreationDateKey"],
      ...["creationDate", "modificationDate", "fileModificationDate"],
    ],
  },
  {
    category: "NSPrivacyAccessedAPICategorySystemBootTime",
    // Monotonic clocks time frames and events within the app.
    reason: "35F9.1",
    names: ["_mach_absolute_time", "systemUptime"],
  },
  {
    category: "NSPrivacyAccessedAPICategoryDiskSpace",
    names: [
      ...["_statfs", "_statvfs", "_fstatfs", "_fstatvfs", "_getattrlist", "_fgetattrlist"],
      ...["_getattrlistat", "_NSFileSystemFreeSize", "_NSFileSystemSize"],
      ...["_NSURLVolumeAvailableCapacityKey", "_NSURLVolumeAvailableCapacityForImportantUsageKey"],
      ...["_NSURLVolumeAvailableCapacityForOpportunisticUsageKey", "_NSURLVolumeTotalCapacityKey"],
    ],
  },
  { category: "NSPrivacyAccessedAPICategoryActiveKeyboards", names: ["activeInputModes"] },
  { category: "NSPrivacyAccessedAPICategoryUserDefaults", names: ["NSUserDefaults"] },
];
const APP_STORE_CONNECT_API = "https://api.appstoreconnect.apple.com/v1";
// Apple usually processes a build in 5 to 20 minutes.
const PROCESSING_TIMEOUT_MS = 45 * 60 * 1000;
const PROCESSING_POLL_MS = 30 * 1000;
const API_TRIES = 4;
const API_RETRY_MS = 5 * 1000;

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
// The three-part version in package.json, the bundles' short version.
const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
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
      ...(singlePlayer ? ["--features", "single-player"] : []),
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

/** The Info.plist entries the Mac and iOS bundles share: the app's name, identifier,
 * executable and versions. */
function bundleInfo() {
  return {
    CFBundleDevelopmentRegion: "en",
    CFBundleDisplayName: APP_NAME,
    CFBundleExecutable: BINARY,
    CFBundleIdentifier: BUNDLE_ID,
    CFBundleInfoDictionaryVersion: "6.0",
    CFBundleName: APP_NAME,
    CFBundlePackageType: "APPL",
    CFBundleShortVersionString: version,
    CFBundleVersion: stamps.SLOPPY_RELEASE,
    LSApplicationCategoryType: "public.app-category.action-games",
    // TLS to the game servers is the only encryption, which export rules exempt.
    ITSAppUsesNonExemptEncryption: false,
  };
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

async function buildMac(appStore) {
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
  const scratch = join(root, "target", PROFILE, appStore ? "mac-app-store-bundle" : "bundle");
  rmSync(scratch, { recursive: true, force: true });
  mkdirSync(scratch, { recursive: true });
  // What an App Store build needs is found before it compiles.
  const store = appStore ? macAppStoreSigning(scratch) : null;
  const destination = process.argv.includes("--upload") ? await appStoreDestination() : null;
  ensureTargets([MAC_TARGET]);
  const minimum = appStore ? MINIMUM_MACOS_APP_STORE : MINIMUM_MACOS;
  // rustc targets macOS 11 by default; only the App Store build asks for a later one.
  const binary = build(MAC_TARGET, appStore ? { MACOSX_DEPLOYMENT_TARGET: minimum } : {});

  const app = join(scratch, `${APP_NAME}.app`);
  const contents = join(app, "Contents");
  mkdirSync(join(contents, "MacOS"), { recursive: true });
  mkdirSync(join(contents, "Resources"), { recursive: true });
  const executable = join(contents, "MacOS", BINARY);
  copyFileSync(binary, executable);
  copyFileSync(appIcon, join(contents, "Resources", "AppIcon.icns"));
  writeFileSync(join(contents, "Resources", "PrivacyInfo.xcprivacy"), privacyManifest(binary));
  writeFileSync(
    join(contents, "Info.plist"),
    plist({
      ...bundleInfo(),
      ...(appStore ? { CFBundleVersion: appStoreBuild() } : {}),
      ...toolchainInfo("macosx"),
      CFBundleIconFile: "AppIcon",
      LSMinimumSystemVersion: minimum,
      NSHighResolutionCapable: true,
      NSSupportsAutomaticGraphicsSwitching: true,
    }),
  );
  writeFileSync(join(contents, "PkgInfo"), "APPL????");
  if (store) {
    await finishForMacAppStore(app, store, destination, scratch);
    return;
  }
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

/** The identity and profile that sign the Mac app for the App Store, and the identity
 * that signs its package (see the top of the file). Stops the build without them. */
function macAppStoreSigning(scratch) {
  const signing = profileSigning(scratch, "OSX", true);
  const installer = identity(
    "3rd Party Mac Developer Installer",
    process.env.SLOPPY_MAC_INSTALLER_IDENTITY,
    [],
  );
  if (!signing || !installer) {
    console.error(
      `No Apple Distribution identity with a Mac App Store profile for ${BUNDLE_ID}, or ` +
        "no 3rd Party Mac Developer Installer identity.",
    );
    process.exit(1);
  }
  return { ...signing, installer };
}

/** Sign `app` for the Mac App Store and package it for App Store Connect, which takes a
 * signed package that installs the app in Applications; with a `destination`, upload it. */
async function finishForMacAppStore(app, store, destination, scratch) {
  const { profile, identity, installer } = store;
  copyFileSync(profile.file, join(app, "Contents", "embedded.provisionprofile"));
  const entitlements = join(scratch, "entitlements.plist");
  writeFileSync(
    entitlements,
    plist({
      "com.apple.application-identifier": `${profile.team}.${BUNDLE_ID}`,
      "com.apple.developer.team-identifier": profile.team,
      // The Mac App Store takes only sandboxed apps. The game reaches nothing but its
      // servers; settings land in the sandbox's own Application Support.
      "com.apple.security.app-sandbox": true,
      "com.apple.security.network.client": true,
      ...(profile.testFlight ? { "beta-reports-active": true } : {}),
    }),
  );
  console.log(`Signing with ${identity.name} and the profile "${profile.name}".`);
  run("codesign", ["--force", "--sign", identity.hash, "--entitlements", entitlements, app]);
  run("codesign", ["--verify", "--strict", "--verbose=2", app]);

  const folder = join(root, "artifacts", "mac-app-store");
  mkdirSync(folder, { recursive: true });
  const built = join(folder, `${APP_NAME}.app`);
  rmSync(built, { recursive: true, force: true });
  run("ditto", [app, built]);
  const installerPackage = join(
    folder,
    `${APP_NAME.replaceAll(" ", "-")}-${stamps.SLOPPY_RELEASE}-macOS-App-Store.pkg`,
  );
  rmSync(installerPackage, { force: true });
  run("productbuild", ["--component", app, "/Applications", "--sign", installer, installerPackage]);
  console.log(
    `Built ${built} (Apple silicon, ${megabytes(join(app, "Contents", "MacOS", BINARY))})`,
  );
  console.log(`Built ${installerPackage} (${megabytes(installerPackage)})`);
  if (destination) {
    await uploadToAppStore(installerPackage, "macos", destination, appStoreBuild(), scratch);
  }
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
    // iOS profiles name the app ID `application-identifier`, Mac ones with Apple's prefix.
    appId:
      field("Entitlements.application-identifier") ||
      field("Entitlements.com\\.apple\\.application-identifier"),
    team: field("TeamIdentifier.0"),
    platforms: field("Platform", "json"),
    expires: new Date(field("ExpirationDate")),
    debuggable: field("Entitlements.get-task-allow") === "true",
    // App Store profiles grant TestFlight's entitlement.
    testFlight: field("Entitlements.beta-reports-active") === "true",
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

/** The profile and identity that sign the app for `platform` ("iOS" or "OSX"): the iOS
 * app for devices, or with `appStore` either app for App Store Connect (see the top of the
 * file), or null. */
function profileSigning(scratch, platform, appStore) {
  const ios = platform === "iOS";
  const named = ios ? process.env.SLOPPY_IOS_IDENTITY : process.env.SLOPPY_MAC_APP_IDENTITY;
  const kind = appStore ? "Distribution" : "Development";
  const identities = signingIdentities().filter(({ name }) =>
    named
      ? name === named || name.startsWith(named)
      : new RegExp(`^(Apple|iPhone) ${kind}: `).test(name),
  );
  const home = process.env.HOME ?? "";
  const folders = [
    join(home, "Library/Developer/Xcode/UserData/Provisioning Profiles"),
    join(home, "Library/MobileDevice/Provisioning Profiles"),
  ];
  const chosen = ios ? process.env.SLOPPY_IOS_PROFILE : process.env.SLOPPY_MAC_PROFILE;
  const extension = ios ? ".mobileprovision" : ".provisionprofile";
  const files = chosen
    ? [chosen]
    : folders.flatMap((folder) =>
        existsSync(folder)
          ? readdirSync(folder)
              .filter((name) => name.endsWith(extension))
              .map((name) => join(folder, name))
          : [],
      );
  const candidates = [];
  for (const file of files) {
    const profile = readProfile(file, scratch);
    // A development profile lists the devices it installs on, an App Store one none.
    if (!profile || !profile.platforms.includes(platform) || profile.devices === appStore) continue;
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
  // iOS draws every size from one 1024-point square, the web app's artwork scaled up.
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
    ...["--minimum-deployment-target", MINIMUM_IOS],
    ...["--target-device", "iphone", "--target-device", "ipad"],
    ...["--app-icon", "AppIcon", "--output-partial-info-plist", partial],
    ...["--output-format", "human-readable-text", "--notices", "--warnings"],
  ]);
  return JSON.parse(capture("plutil", ["-convert", "json", "-o", "-", partial]) || "{}");
}

/** The Info.plist entries Xcode records about the SDK and toolchain of a build, which App
 * Store Connect reads to hold uploads to its minimum SDK and Xcode. */
function toolchainInfo(platform) {
  const sdkVersion = capture("xcrun", ["--sdk", platform, "--show-sdk-version"]);
  const sdkBuild = capture("xcrun", ["--sdk", platform, "--show-sdk-build-version"]);
  const [, xcode = "", xcodeBuild = ""] =
    capture("xcodebuild", ["-version"]).match(/^Xcode (\S+)\nBuild version (\S+)/) ?? [];
  // Xcode writes 26.4.1 as 2641 and 26.6 as 2660.
  const [major = "0", minor = "0", patch = "0"] = xcode.split(".");
  return {
    BuildMachineOSBuild: capture("sw_vers", ["-buildVersion"]),
    DTCompiler: "com.apple.compilers.llvm.clang.1_0",
    DTPlatformBuild: sdkBuild,
    DTPlatformName: platform,
    DTPlatformVersion: sdkVersion,
    DTSDKBuild: sdkBuild,
    DTSDKName: `${platform}${sdkVersion}`,
    DTXcode: `${major.padStart(2, "0")}${minor}${patch}`,
    DTXcodeBuild: xcodeBuild,
  };
}

/** The app's privacy manifest: no tracking, no data collected, and a reason for each
 * required-reason API category `executable` uses. Stops the build at a category without
 * one. */
function privacyManifest(executable) {
  const contents = readFileSync(executable);
  const used = REQUIRED_REASON_APIS.map((api) => ({
    ...api,
    found: api.names.filter((name) => contents.includes(`\0${name}\0`)),
  })).filter(({ found }) => found.length);
  const unexplained = used.filter(({ reason }) => !reason);
  if (unexplained.length) {
    for (const { category, found } of unexplained) {
      console.error(`The app uses ${category} (${found.join(", ")}), which has no reason.`);
    }
    console.error("Give each a reason in REQUIRED_REASON_APIS from Apple's approved list.");
    process.exit(1);
  }
  return plist({
    NSPrivacyTracking: false,
    NSPrivacyTrackingDomains: [],
    NSPrivacyCollectedDataTypes: [],
    NSPrivacyAccessedAPITypes: used.map(({ category, reason }) => ({
      NSPrivacyAccessedAPIType: category,
      NSPrivacyAccessedAPITypeReasons: [reason],
    })),
  });
}

/** The build number of an App Store upload (see the top of the file). */
function appStoreBuild() {
  const build = process.env.SLOPPY_APP_STORE_BUILD || version;
  if (!/^\d+(\.\d+){0,2}$/.test(build)) {
    console.error(`An App Store build is one to three integers, not ${build}.`);
    process.exit(1);
  }
  return build;
}

async function buildIos(simulator, appStore) {
  if (process.platform !== "darwin") {
    console.error("The iOS app builds on a Mac with Xcode (actool, codesign).");
    process.exit(1);
  }
  const kind = simulator ? "ios-simulator" : appStore ? "ios-app-store" : "ios";
  const scratch = join(root, "target", PROFILE, `${kind}-bundle`);
  rmSync(scratch, { recursive: true, force: true });
  mkdirSync(scratch, { recursive: true });
  // What an App Store build needs is found before it compiles.
  const signing = simulator ? null : profileSigning(scratch, "iOS", appStore);
  if (appStore && !signing) {
    console.error(
      `No Apple Distribution identity with an App Store provisioning profile for ${BUNDLE_ID}.`,
    );
    process.exit(1);
  }
  const destination = process.argv.includes("--upload") ? await appStoreDestination() : null;
  const target = simulator ? IOS_SIMULATOR_TARGET : IOS_TARGET;
  const platform = simulator ? "iphonesimulator" : "iphoneos";
  ensureTargets([target]);
  const binary = build(target, { IPHONEOS_DEPLOYMENT_TARGET: MINIMUM_IOS });

  const app = join(scratch, `${APP_NAME}.app`);
  mkdirSync(app, { recursive: true });
  copyFileSync(binary, join(app, BINARY));
  writeFileSync(join(app, "PrivacyInfo.xcprivacy"), privacyManifest(binary));
  const assets = compileAssets(app, scratch, platform);
  // Held either way, as the page lays out an upright phone or tablet; an iPhone not upside
  // down (winit's `LandscapeAndPortrait` asks the same).
  const landscape = ["UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"];
  const portrait = ["UIInterfaceOrientationPortrait"];
  writeFileSync(
    join(app, "Info.plist"),
    plist({
      ...bundleInfo(),
      ...(appStore ? { CFBundleVersion: appStoreBuild() } : {}),
      ...toolchainInfo(platform),
      CFBundleSupportedPlatforms: [simulator ? "iPhoneSimulator" : "iPhoneOS"],
      LSRequiresIPhoneOS: true,
      MinimumOSVersion: MINIMUM_IOS,
      // iPhone and iPad: an iPhone plays the phone edition's screens.
      UIDeviceFamily: [1, 2],
      UIRequiredDeviceCapabilities: ["arm64", "metal"],
      UIRequiresFullScreen: true,
      UISupportedInterfaceOrientations: [...portrait, ...landscape],
      "UISupportedInterfaceOrientations~ipad": [
        ...portrait,
        "UIInterfaceOrientationPortraitUpsideDown",
        ...landscape,
      ],
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
      ...assets,
    }),
  );
  writeFileSync(join(app, "PkgInfo"), "APPL????");

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
        ...(profile.testFlight ? { "beta-reports-active": true } : {}),
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
          `${BUNDLE_ID}: the app is signed ad hoc and installs on no device.`,
      );
    }
    run("codesign", ["--force", "--sign", "-", app]);
  }

  const folder = join(root, "artifacts", kind);
  mkdirSync(folder, { recursive: true });
  const built = join(folder, `${APP_NAME}.app`);
  rmSync(built, { recursive: true, force: true });
  run("ditto", [app, built]);
  console.log(`Built ${built} (${megabytes(join(app, BINARY))})`);
  const suffix = simulator ? "-Simulator" : appStore ? "-App-Store" : "";
  const artifact = join(
    folder,
    `${APP_NAME.replaceAll(" ", "-")}-${stamps.SLOPPY_RELEASE}-iOS${suffix}`,
  );
  if (simulator) {
    const zip = `${artifact}.zip`;
    rmSync(zip, { force: true });
    run("ditto", ["-c", "-k", "--keepParent", app, zip]);
    console.log(`Built ${zip} (${megabytes(zip)})`);
  } else {
    // An .ipa is the app in a Payload folder, zipped: what App Store Connect, Apple
    // Configurator, Xcode's Devices window and over-the-air installs take.
    const payload = join(scratch, "ipa");
    mkdirSync(join(payload, "Payload"), { recursive: true });
    run("ditto", [app, join(payload, "Payload", `${APP_NAME}.app`)]);
    const ipa = `${artifact}.ipa`;
    rmSync(ipa, { force: true });
    run("ditto", ["-c", "-k", "--norsrc", "--keepParent", join(payload, "Payload"), ipa]);
    console.log(`Built ${ipa} (${megabytes(ipa)})`);
    if (destination) await uploadToAppStore(ipa, "ios", destination, appStoreBuild(), scratch);
  }
  if (process.argv.includes("--install")) {
    if (simulator) installOnSimulator(built);
    else installOnDevice(built, scratch);
  }
}

/** The App Store Connect API key notarization uses (see the top of the file), or null. */
function appStoreConnectKey() {
  const {
    SLOPPY_NOTARY_KEY: file,
    SLOPPY_NOTARY_KEY_ID: id,
    SLOPPY_NOTARY_ISSUER: issuer,
  } = process.env;
  return file && id && issuer ? { file, id, issuer } : null;
}

/** The `data` of `path` with `query` from the App Store Connect API, each request with a
 * token of its own, as one lasts 20 minutes at most. */
async function appStoreConnect(key, path, query) {
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const claims = { iss: key.issuer, iat: now, exp: now + 10 * 60, aud: "appstoreconnect-v1" };
  const unsigned = `${encode({ alg: "ES256", kid: key.id, typ: "JWT" })}.${encode(claims)}`;
  const signature = sign("sha256", Buffer.from(unsigned), {
    key: readFileSync(key.file, "utf8"),
    dsaEncoding: "ieee-p1363",
  });
  const url = `${APP_STORE_CONNECT_API}${path}?${new URLSearchParams(query)}`;
  const headers = { Authorization: `Bearer ${unsigned}.${signature.toString("base64url")}` };
  // A connection kept alive through the compile is often gone by the next request
  // (EPIPE), and a poll over many minutes meets the odd network failure.
  let response;
  for (let tries = 1; !response; tries += 1) {
    try {
      response = await fetch(url, { headers });
    } catch (error) {
      if (tries === API_TRIES) throw error;
      await new Promise((resolve) => setTimeout(resolve, API_RETRY_MS));
    }
  }
  if (!response.ok) {
    console.error(
      `App Store Connect answered ${path} with ${response.status}: ${await response.text()}`,
    );
    process.exit(1);
  }
  return (await response.json()).data;
}

/** The API key and the App Store Connect app an upload goes to. Stops the build, before
 * it compiles, without either. */
async function appStoreDestination() {
  const key = appStoreConnectKey();
  if (!key) {
    console.error(
      "--upload needs an App Store Connect API key: SLOPPY_NOTARY_KEY, SLOPPY_NOTARY_KEY_ID " +
        "and SLOPPY_NOTARY_ISSUER.",
    );
    process.exit(1);
  }
  const apps = await appStoreConnect(key, "/apps", {
    "filter[bundleId]": BUNDLE_ID,
    "fields[apps]": "bundleId",
  });
  const app = apps.find((candidate) => candidate.attributes.bundleId === BUNDLE_ID);
  if (!app) {
    console.error(`App Store Connect has no app for ${BUNDLE_ID}: add it under Apps first.`);
    process.exit(1);
  }
  return { key, app: app.id };
}

/** Send `file` (an .ipa, or a Mac App Store .pkg), build `build` of the app for
 * `platform` ("ios" or "macos"), to the App Store Connect app `destination` names and wait
 * until Apple has processed it. */
async function uploadToAppStore(file, platform, destination, build, scratch) {
  const { key, app } = destination;
  // altool reads a key only from a file named for its ID in a `private_keys` folder, and
  // Xcode 26's can send a build to another of the team's apps unless `--apple-id` names it.
  const keys = join(scratch, "private_keys");
  mkdirSync(keys, { recursive: true });
  copyFileSync(key.file, join(keys, `AuthKey_${key.id}.p8`));
  console.log(`Uploading ${file} to App Store Connect…`);
  const upload = spawnSync(
    "xcrun",
    [
      ...["altool", "--upload-package", file, "--type", platform, "--apple-id", app],
      ...["--bundle-id", BUNDLE_ID, "--bundle-version", build],
      ...["--bundle-short-version-string", version],
      ...["--apiKey", key.id, "--apiIssuer", key.issuer],
    ],
    { cwd: scratch, stdio: "inherit" },
  );
  rmSync(keys, { recursive: true, force: true });
  if (upload.status !== 0) {
    console.error(`altool could not upload ${file}.`);
    process.exit(1);
  }
  await awaitProcessing(key, app, platform, build);
}

/** Wait until App Store Connect has processed build `build` of `app` for `platform`,
 * which is then in TestFlight. Xcode 26's altool can report an upload that never arrives, so a build App
 * Store Connect still does not list at the deadline fails, while one it is still
 * processing only warns. */
async function awaitProcessing(key, app, platform, build) {
  const query = {
    "filter[app]": app,
    "filter[preReleaseVersion.platform]": platform === "ios" ? "IOS" : "MAC_OS",
    "filter[version]": build,
    "filter[preReleaseVersion.version]": version,
    "fields[builds]": "processingState",
  };
  const deadline = Date.now() + PROCESSING_TIMEOUT_MS;
  let state = null;
  while (Date.now() < deadline) {
    state = (await appStoreConnect(key, "/builds", query))[0]?.attributes.processingState ?? null;
    if (state === "VALID") {
      console.log(`App Store Connect processed build ${build}: it is in TestFlight.`);
      return;
    }
    if (state === "INVALID" || state === "FAILED") {
      console.error(
        `App Store Connect found build ${build} ${state.toLowerCase()}: Apple's email says why.`,
      );
      process.exit(1);
    }
    await new Promise((resolve) => setTimeout(resolve, PROCESSING_POLL_MS));
  }
  const minutes = PROCESSING_TIMEOUT_MS / 60_000;
  if (!state) {
    console.error(`App Store Connect lists no build ${build} ${minutes} minutes after its upload.`);
    process.exit(1);
  }
  console.warn(`Build ${build} is still ${state.toLowerCase()} after ${minutes} minutes.`);
}

/** The kinds of device `--install` may take: `--iphone` or `--ipad`, or either. */
function installKinds() {
  const iphone = process.argv.includes("--iphone");
  const ipad = process.argv.includes("--ipad");
  if (iphone === ipad) return ["iPhone", "iPad"];
  return iphone ? ["iPhone"] : ["iPad"];
}

/** Install `app` on the simulator `SLOPPY_IOS_DEVICE` names (name or UDID), or on a
 * booted iPhone or iPad simulator, booting one if none is, and open it. */
function installOnSimulator(app) {
  const listed = JSON.parse(capture("xcrun", ["simctl", "list", "devices", "available", "--json"]));
  const named = process.env.SLOPPY_IOS_DEVICE;
  const kinds = installKinds();
  const simulators = Object.values(listed.devices ?? {})
    .flat()
    .filter((device) =>
      named
        ? device.name === named || device.udid === named
        : kinds.some((kind) => device.name.startsWith(kind)),
    );
  let device = simulators.find((candidate) => candidate.state === "Booted");
  if (!device) {
    // The first kind asked for first: an iPhone unless `--ipad` says otherwise.
    device = named
      ? simulators[0]
      : kinds
          .map((kind) => simulators.find((candidate) => candidate.name.startsWith(kind)))
          .find(Boolean);
    if (!device) {
      console.error(
        named
          ? `No simulator named ${named}.`
          : `No ${kinds.join(" or ")} simulator: add one in Xcode's Devices and Simulators window.`,
      );
      process.exit(1);
    }
    run("xcrun", ["simctl", "boot", device.udid]);
  }
  run("xcrun", ["simctl", "install", device.udid, app]);
  run("xcrun", ["simctl", "launch", device.udid, BUNDLE_ID]);
  console.log(`Opened on ${device.name} (simulator).`);
}

/** Install `app` on the device `SLOPPY_IOS_DEVICE` names, the one iPhone or iPad on a
 * cable, or the only one paired, and open it. Paired devices answer over the local
 * network too, other people's among them, so it never picks one of several. */
function installOnDevice(app, scratch) {
  let device = process.env.SLOPPY_IOS_DEVICE;
  if (!device) {
    const kinds = installKinds();
    const listing = join(scratch, "devices.json");
    run("xcrun", ["devicectl", "list", "devices", "--quiet", "--json-output", listing]);
    const devices = JSON.parse(readFileSync(listing, "utf8")).result?.devices ?? [];
    const paired = devices.filter(
      (candidate) =>
        kinds.includes(candidate.hardwareProperties?.deviceType) &&
        candidate.hardwareProperties?.reality === "physical" &&
        candidate.connectionProperties?.pairingState === "paired",
    );
    const wired = paired.filter(
      (candidate) => candidate.connectionProperties?.transportType === "wired",
    );
    const chosen = wired.length === 1 ? wired[0] : paired.length === 1 ? paired[0] : null;
    if (!chosen) {
      const names = paired.map((candidate) => candidate.deviceProperties?.name).join(", ");
      console.error(
        paired.length
          ? `Several devices are paired (${names}): set SLOPPY_IOS_DEVICE to one's name or UDID.`
          : `No paired ${kinds.join(" or ")}: set SLOPPY_IOS_DEVICE to a device's name or UDID.`,
      );
      process.exit(1);
    }
    device = chosen.hardwareProperties.udid;
  }
  run("xcrun", ["devicectl", "device", "install", "app", "--device", device, app]);
  run("xcrun", ["devicectl", "device", "process", "launch", "--device", device, BUNDLE_ID]);
}

const mac = process.argv.includes("--mac");
const windows = process.argv.includes("--windows");
const ios = process.argv.includes("--ios");
const iosSimulator = process.argv.includes("--ios-simulator");
const appStore = process.argv.includes("--app-store");
const singlePlayer = process.argv.includes("--single-player");
if (appStore && !(ios || mac)) {
  console.error("--app-store builds the iOS app (--ios) or the Mac app (--mac) for the App Store.");
  process.exit(1);
}
if (process.argv.includes("--upload") && !appStore) {
  console.error("--upload sends an App Store build: pass it with --app-store.");
  process.exit(1);
}
if (appStore && process.argv.includes("--install")) {
  console.error("An App Store build installs only through TestFlight.");
  process.exit(1);
}
if (ios || iosSimulator) await buildIos(iosSimulator, appStore);
else if (mac || (!windows && process.platform === "darwin")) await buildMac(appStore);
else if (windows || process.platform === "win32") buildWindows();
else {
  console.error("Desktop builds are for macOS and Windows: pass --mac or --windows.");
  process.exit(1);
}
