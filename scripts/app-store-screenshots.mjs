// App Store Connect screenshots of the iPhone, iPad and Mac apps, at the sizes Apple takes
// (App Store Connect's "Screenshot specifications"), into the ignored
// artifacts/app-store/screenshots/<device>/NN-<name>.png:
//
//   pnpm run screenshots:app-store                       # build both apps, then every device
//   pnpm run screenshots:app-store -- --no-build         # the apps artifacts/ already holds
//   pnpm run screenshots:app-store -- --only ipad-13,mac --no-report
//
// The apps are built single player, as the App Store has them (`--single-player`).
// Each device shows Battle Setup, three autoplayed single-player battles (one per standard
// map, the player's tank driving itself) and, unless `--no-report`, the third battle's
// report, which waits out its five-minute round. The game reads its command line as a
// page reads its query (`crates/desktop/src/main.rs`): `--map`, `--seed` and
// `--autoplay`, which starts the round once the arena is ready. The seed fixes each
// arena; the autoplayed match follows the frames and differs between runs, so each battle
// also leaves alternates (`ALTERNATE_SECONDS`).
//
// The iOS app runs on simulators this script creates once ("Sloppy Tanks <device>"),
// booted beside whatever else is, shut down again after. It is the simulator build
// (`build-desktop.mjs --ios-simulator`) with its Info.plist cut to landscape, so it opens
// on its side without turning the simulator, which `simctl` cannot do. The status bar is
// hidden by the app (UIStatusBarHidden); a Dynamic Island simulator draws its island
// into the screenshot as a black pill, as a device's own screenshots do not. The Mac app runs its binary with HOME in a
// scratch folder, so it reads these settings rather than the player's, in a window whose
// content is 1440×900 points (1280×800 where the screen above the Dock is too small),
// captured by `screencapture` at the display's 2×. That needs the Screen Recording
// permission for the app running the script, and moving and sizing the window through
// System Events needs Accessibility.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const output = join(root, "artifacts/app-store/screenshots");
const scratch = join(root, "artifacts/app-store/scratch");
const BUNDLE_ID = "me.fridman.sloppy-tanks";
const IOS_APP = join(root, "artifacts/ios-simulator/Sloppy Tanks.app");
const MAC_APP = join(root, "artifacts/desktop/Sloppy Tanks.app");
const MAC_BINARY = "Contents/MacOS/sloppy-tanks";
// `ROUND_TIME` (crates/core/src/sim/data.rs): an autoplayed round ends on the clock, or
// in overtime on the next kill when the scores are level.
const ROUND_SECONDS = 300;
// Past the round's end: the arena's build before GO and overtime's deciding kill.
const REPORT_MARGIN_SECONDS = 45;
// The saved settings every device starts from (`preferences.rs`): the High graphics tier
// whatever the device class or the simulator's frame rate would pick, and no sound.
const SETTINGS = { graphics: "high", volume: 0 };
// The shots, in App Store order. Battle Setup waits for its arena; a battle for its
// arena, GO and a fight under way.
const SHOTS = [
  { name: "battle-setup", map: "village", seed: 11, seconds: 25 },
  { name: "village", map: "village", seed: 11, seconds: 50, autoplay: true },
  { name: "harbor", map: "harbor", seed: 21, seconds: 50, autoplay: true },
  { name: "quarry", map: "quarry", seed: 31, seconds: 50, autoplay: true, report: true },
];
// A battle's moment may catch the player's tank destroyed, its respawn panel over the
// fight, and the seed cannot rule that out: the autoplayed tank's commands follow the
// frames, so the match differs between runs and devices. These later moments of each
// battle go to <device>/alternates/ to swap in.
const ALTERNATE_SECONDS = [7, 14];
// Apple's sizes, landscape (the game's orientation): iPhone 6.9" and 6.3" (Dynamic
// Island, large and medium display) and iPad 13", from simulators by device type, newest
// first; the Mac's follows its window.
const DEVICES = [
  {
    id: "iphone-6.9",
    size: [2868, 1320],
    types: ["iPhone-18-Pro-Max", "iPhone-17-Pro-Max", "iPhone-16-Pro-Max"],
  },
  { id: "iphone-6.3", size: [2622, 1206], types: ["iPhone-18-Pro", "iPhone-17-Pro"] },
  {
    id: "ipad-13",
    size: [2752, 2064],
    types: ["iPad-Pro-13-inch-M5-12GB", "iPad-Pro-13-inch-M4-8GB"],
  },
  { id: "mac", mac: true },
];
// The Mac window's content in points, the largest the screen holds above the Dock: 2880×1800
// or 2560×1600 pixels at a Retina display's 2×, both 16:10 sizes Apple takes.
const MAC_WINDOWS = [
  [1440, 900],
  [1280, 800],
];
// The app's first window content (`WINDOW_SIZE`, crates/desktop/src/app.rs).
const MAC_FIRST_WINDOW = [1280, 800];

function run(command, args, options = {}) {
  return (execFileSync(command, args, { encoding: "utf8", ...options }) ?? "").trim();
}

/** What follows `name` on the command line. */
function argument(name) {
  const index = process.argv.indexOf(name);
  return index < 0 ? undefined : process.argv[index + 1];
}

const only = argument("--only")?.split(",");
const devices = DEVICES.filter((device) => !only || only.includes(device.id));
if (only && devices.length !== only.length) {
  console.error(`--only takes ${DEVICES.map((device) => device.id).join(", ")}.`);
  process.exit(1);
}
const withReport = !process.argv.includes("--no-report");
const shots = SHOTS.map((shot, index) => ({ ...shot, file: `0${index + 1}-${shot.name}.png` }));

function log(device, message) {
  console.log(`[${device.id}] ${message}`);
}

function launchArguments(shot) {
  return [
    ...["--map", shot.map, "--seed", String(shot.seed)],
    ...(shot.autoplay ? ["--autoplay"] : []),
  ];
}

/** The PNG's pixel size. */
function pixelSize(file) {
  const text = run("sips", ["-g", "pixelWidth", "-g", "pixelHeight", file]);
  return [/pixelWidth: (\d+)/, /pixelHeight: (\d+)/].map((pattern) =>
    Number(text.match(pattern)[1]),
  );
}

// A PNG (argv[0]) redrawn into argv[1] without its alpha channel, every pixel and the colour
// profile as they were (simctl's and screencapture's PNGs carry an opaque alpha channel):
// CoreGraphics through JavaScript for Automation, which every Mac has.
const FLATTEN = `ObjC.import("CoreGraphics");
ObjC.import("ImageIO");
function run([input, output]) {
  const source = $.CGImageSourceCreateWithURL($.NSURL.fileURLWithPath(input), null);
  const image = $.CGImageSourceCreateImageAtIndex(source, 0, null);
  const width = $.CGImageGetWidth(image);
  const height = $.CGImageGetHeight(image);
  // 5 is kCGImageAlphaNoneSkipLast: four bytes a pixel, the last unused.
  const context = $.CGBitmapContextCreate(null, width, height, 8, 0, $.CGImageGetColorSpace(image), 5);
  $.CGContextDrawImage(context, { origin: { x: 0, y: 0 }, size: { width, height } }, image);
  const url = $.NSURL.fileURLWithPath(output);
  const destination = $.CGImageDestinationCreateWithURL(url, $("public.png"), 1, null);
  $.CGImageDestinationAddImage(destination, $.CGBitmapContextCreateImage(context), null);
  if (!$.CGImageDestinationFinalize(destination)) throw new Error("no PNG written");
}`;

/** Keep `file` as the device's `name`, landscape, at exactly Apple's size and opaque
 * (App Store Connect refuses an alpha channel). */
function keep(device, file, name) {
  let [width, height] = pixelSize(file);
  if (width < height) {
    // A simulator's framebuffer stays upright, the app on its side within it.
    run("sips", ["--rotate", "270", file]);
    [width, height] = [height, width];
  }
  if (width !== device.size[0] || height !== device.size[1]) {
    throw new Error(`${name} is ${width}×${height}, not ${device.size.join("×")}`);
  }
  if (run("sips", ["-g", "hasAlpha", file]).includes("hasAlpha: yes")) {
    const flat = `${file}.flat.png`;
    run("osascript", ["-l", "JavaScript", "-e", FLATTEN, file, flat]);
    copyFileSync(flat, file);
    rmSync(flat);
  }
  const kept = join(output, device.id, name);
  mkdirSync(dirname(kept), { recursive: true });
  copyFileSync(file, kept);
  rmSync(file);
  log(device, `${device.id}/${name} (${width}×${height})`);
}

/** Where a capture of the device's `name` lands before it is kept. */
function scratchFile(device, name) {
  return join(scratch, `${device.id}-${name.replaceAll("/", "-")}`);
}

/** Take `shot`'s pictures through `capture` (a name under the device's folder), counting
 * from the app's launch at `launched`: the shot, a battle's alternates and the battle
 * report. */
async function shoot(device, shot, launched, capture) {
  const at = (seconds) => sleep(launched + seconds * 1000 - Date.now());
  await at(shot.seconds);
  capture(shot.file);
  if (shot.autoplay) {
    for (const [index, seconds] of ALTERNATE_SECONDS.entries()) {
      await at(shot.seconds + seconds);
      capture(`alternates/${shot.file.replace(".png", `-${index + 2}.png`)}`);
    }
  }
  if (shot.report && withReport) {
    const end = ROUND_SECONDS + REPORT_MARGIN_SECONDS;
    log(
      device,
      `waiting ${Math.round(end - (Date.now() - launched) / 1000)} s for the battle report`,
    );
    await at(end);
    capture(`0${shots.length + 1}-battle-report.png`);
  }
}

// ---------------------------------------------------------------- iOS simulators

function simulators() {
  return JSON.parse(run("xcrun", ["simctl", "list", "devices", "--json"])).devices;
}

/** The simulator for `device`, created on the newest iOS runtime with its device type
 * the first time. */
function simulator(device) {
  const name = `Sloppy Tanks ${device.id}`;
  const existing = Object.values(simulators())
    .flat()
    .find((candidate) => candidate.name === name && candidate.isAvailable);
  if (existing) return existing.udid;
  const runtimes = JSON.parse(run("xcrun", ["simctl", "list", "runtimes", "--json"]))
    .runtimes.filter((runtime) => runtime.platform === "iOS" && runtime.isAvailable)
    .sort((a, b) => b.version.localeCompare(a.version, undefined, { numeric: true }));
  for (const type of device.types) {
    const identifier = `com.apple.CoreSimulator.SimDeviceType.${type}`;
    const runtime = runtimes.find((candidate) =>
      candidate.supportedDeviceTypes.some((supported) => supported.identifier === identifier),
    );
    if (runtime) {
      log(device, `creating the simulator "${name}" (${type}, iOS ${runtime.version})`);
      return run("xcrun", ["simctl", "create", name, identifier, runtime.identifier]);
    }
  }
  throw new Error(`no iOS runtime has a ${device.types.join(" or ")} simulator`);
}

/** The simulator build turned landscape-only and signed again, once for every device. */
let landscapeApp;
function landscapeIosApp() {
  if (landscapeApp) return landscapeApp;
  if (!existsSync(IOS_APP)) {
    throw new Error(`${IOS_APP} is missing: run pnpm run ios:simulator, or drop --no-build`);
  }
  landscapeApp = join(scratch, "ios", "Sloppy Tanks.app");
  rmSync(landscapeApp, { recursive: true, force: true });
  mkdirSync(join(scratch, "ios"), { recursive: true });
  run("ditto", [IOS_APP, landscapeApp]);
  const plist = join(landscapeApp, "Info.plist");
  const landscape = JSON.stringify([
    "UIInterfaceOrientationLandscapeRight",
    "UIInterfaceOrientationLandscapeLeft",
  ]);
  for (const key of ["UISupportedInterfaceOrientations", "UISupportedInterfaceOrientations~ipad"]) {
    run("plutil", ["-replace", key, "-json", landscape, plist]);
  }
  run("codesign", ["--force", "--sign", "-", landscapeApp]);
  return landscapeApp;
}

/** Boot `device`'s simulator and install the app afresh with the script's settings; the
 * simulator, and whether this script booted it. */
function prepareIos(device) {
  const udid = simulator(device);
  const booted =
    Object.values(simulators())
      .flat()
      .find((candidate) => candidate.udid === udid).state !== "Booted";
  if (booted) {
    log(device, "booting");
    run("xcrun", ["simctl", "boot", udid]);
  }
  run("xcrun", ["simctl", "bootstatus", udid, "-b"], { stdio: ["ignore", "ignore", "inherit"] });
  // A clean install: no settings or records from an earlier run.
  execFileSync("xcrun", ["simctl", "uninstall", udid, BUNDLE_ID], { stdio: "ignore" });
  run("xcrun", ["simctl", "install", udid, landscapeIosApp()]);
  const data = run("xcrun", ["simctl", "get_app_container", udid, BUNDLE_ID, "data"]);
  const settings = join(data, "Library/Application Support/Sloppy Tanks");
  mkdirSync(settings, { recursive: true });
  writeFileSync(join(settings, "settings.json"), JSON.stringify(SETTINGS));
  return { device, udid, booted };
}

async function captureIos({ device, udid }) {
  const capture = (name) => {
    const file = scratchFile(device, name);
    run("xcrun", ["simctl", "io", udid, "screenshot", "--mask=ignored", file], {
      stdio: ["ignore", "ignore", "pipe"],
    });
    keep(device, file, name);
  };
  for (const shot of shots) {
    log(device, `${shot.name}: launching ${launchArguments(shot).join(" ")}`);
    const launched = Date.now();
    run("xcrun", [
      ...["simctl", "launch", "--terminate-running-process", udid, BUNDLE_ID],
      ...launchArguments(shot),
    ]);
    await shoot(device, shot, launched, capture);
  }
  execFileSync("xcrun", ["simctl", "terminate", udid, BUNDLE_ID], { stdio: "ignore" });
}

// ---------------------------------------------------------------- Mac

/** The on-screen windows of process `pid`, with their bounds in points. */
function macWindows(pid) {
  const script = `ObjC.import("CoreGraphics");
const list = ObjC.castRefToObject(
  $.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, $.kCGNullWindowID));
const windows = [];
for (let i = 0; i < list.count; i++) {
  const window = ObjC.deepUnwrap(list.objectAtIndex(i));
  if (window.kCGWindowOwnerPID === ${pid} && window.kCGWindowLayer === 0)
    windows.push({ id: window.kCGWindowNumber, bounds: window.kCGWindowBounds });
}
JSON.stringify(windows);`;
  return JSON.parse(run("osascript", ["-l", "JavaScript", "-e", script]));
}

/** Bring process `pid` to the front and give its window `frame` (points, title bar
 * included). */
function placeMacWindow(pid, [x, y, width, height]) {
  run("osascript", [
    "-e",
    `tell application "System Events" to tell (first process whose unix id is ${pid})
      set frontmost to true
      set position of window 1 to {${x}, ${y}}
      set size of window 1 to {${width}, ${height}}
    end tell`,
  ]);
}

async function captureMac(device) {
  const binary = join(MAC_APP, MAC_BINARY);
  if (!existsSync(binary)) {
    throw new Error(`${MAC_APP} is missing: run pnpm run desktop:mac, or drop --no-build`);
  }
  // Find out about the permission before the first window rather than at its capture.
  screenCapture([0, 0, 1, 1], join(scratch, "screen-recording-check.png"));
  rmSync(join(scratch, "screen-recording-check.png"));
  // The app keeps its settings in $HOME/Library/Application Support/Sloppy Tanks.
  const home = join(scratch, "mac-home");
  rmSync(home, { recursive: true, force: true });
  const settings = join(home, "Library/Application Support/Sloppy Tanks");
  mkdirSync(settings, { recursive: true });
  writeFileSync(join(settings, "settings.json"), JSON.stringify(SETTINGS));
  for (const shot of shots) {
    log(device, `${shot.name}: launching ${launchArguments(shot).join(" ")}`);
    const launched = Date.now();
    const app = spawn(binary, launchArguments(shot), {
      env: { ...process.env, HOME: home },
      stdio: "ignore",
    });
    try {
      let window;
      for (let tries = 0; !window; tries++) {
        if (tries > 100) throw new Error("the Mac app opened no window");
        await sleep(200);
        window = macWindows(app.pid)[0];
      }
      // The title bar is what the window has above its content (`WINDOW_SIZE` at first).
      const titleBar = window.bounds.Height - MAC_FIRST_WINDOW[1];
      // The largest content the screen holds above the Dock, at its top left; the region
      // it covers, in points.
      const content = () => {
        for (const points of MAC_WINDOWS) {
          placeMacWindow(app.pid, [0, 0, points[0], points[1] + titleBar]);
          const { X, Y, Width, Height } = macWindows(app.pid)[0].bounds;
          if (Width === points[0] && Height === points[1] + titleBar) {
            return [X, Y + titleBar, ...points];
          }
        }
        throw new Error(`the screen holds no ${MAC_WINDOWS.at(-1).join("×")} point window`);
      };
      const capture = (name) => {
        const region = content();
        const file = scratchFile(device, name);
        screenCapture(region, file);
        // A Retina display captures two pixels a point.
        keep({ ...device, size: [region[2] * 2, region[3] * 2] }, file, name);
      };
      content();
      await shoot(device, shot, launched, capture);
    } finally {
      app.kill();
    }
  }
}

/** The screen's `region` (x, y, width and height in points) into `file`. */
function screenCapture(region, file) {
  try {
    execFileSync("screencapture", ["-x", "-R", region.join(","), file], {
      stdio: ["ignore", "ignore", "pipe"],
    });
  } catch (error) {
    throw new Error(
      `screencapture failed (${String(error.stderr).trim()}): give the app running this ` +
        "script the Screen Recording permission (System Settings, Privacy & Security)",
      { cause: error },
    );
  }
}

// ---------------------------------------------------------------- main

if (process.platform !== "darwin") {
  console.error("The App Store screenshots need a Mac with Xcode's simulators.");
  process.exit(1);
}
const ios = devices.filter((device) => !device.mac);
const mac = devices.find((device) => device.mac);
if (!process.argv.includes("--no-build")) {
  const build = (mode) =>
    execFileSync("node", [join(root, "scripts/build-desktop.mjs"), mode, "--single-player"], {
      stdio: "inherit",
    });
  if (ios.length) build("--ios-simulator");
  if (mac) build("--mac");
}
for (const device of devices) rmSync(join(output, device.id), { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });
const failures = [];
const attempt = async (device, work) => {
  try {
    return await work();
  } catch (error) {
    failures.push(device);
    console.error(`[${device.id}] ${error.message}`);
  }
};
// The simulators are booted and installed one by one, then play side by side; the Mac
// app, whose window takes the screen, after them.
const prepared = [];
for (const device of ios) {
  const simulator = await attempt(device, () => prepareIos(device));
  if (simulator) prepared.push(simulator);
}
await Promise.all(
  prepared.map((simulator) => attempt(simulator.device, () => captureIos(simulator))),
);
for (const { udid, booted } of prepared) {
  if (booted) run("xcrun", ["simctl", "shutdown", udid]);
}
if (mac) await attempt(mac, () => captureMac(mac));
console.log(`Screenshots in ${output}`);
if (failures.length) process.exit(1);
