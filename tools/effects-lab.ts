// Effects lab: one scripted scene (tanks laying tracks and dust, every munition in
// flight, and one event of every kind) fed to the engine's wgpu effects, beside a
// reference frame of the same script from the game's former Three.js effect classes
// and an amplified difference image. The references were captured before Three.js left
// the project (`scripts/README.md`, Labs); the Three side drew its cosmetic randomness
// from its own stream, so only the overall match is meaningful (the last calibration
// measured a mean error of about 2 of 255 at t = 4). Without the references the lab
// still draws and reports the effects.
// Build the labs engine with `pnpm run wasm:labs`, then open
// /sloppy-tanks/tools/effects-lab.html on the dev server.
// `?t=<seconds>` (default 4) runs the script to that time and freezes it;
// `?live` loops it. `?theme=quarry|harbor` switches dust colors and effects.
import type { EffectsLab } from "../src/generated/engine-labs/engine.js";
import { compareImages, loadReference, pixels } from "./lab-references";
import { loadLabsEngine } from "./labs-engine";

type Vec3 = [number, number, number];
type VehicleKind = "scout" | "balanced" | "heavy" | "humvee";
type Weapon = "standard" | "spread" | "rocket" | "ricochet" | "piercing" | "tow";
/** A simulation event as the engine reads it (`SimEvent`, camelCase JSON). */
type SimEvent = { type: string; x: number; z: number } & Record<string, unknown>;
interface RenderShot {
  id: number;
  weapon: Weapon;
  team: 0 | 1;
  x: number;
  z: number;
  y: number;
  visualY: number;
  vx: number;
  vz: number;
}
const STEP = 1 / 60;
const PERIOD = 5;
const params = new URLSearchParams(location.search);
const live = params.has("live");
const freezeAt = Number(params.get("t") ?? 4);
const theme = params.get("theme") ?? "village";
const base = import.meta.env.BASE_URL;
const rustCanvas = document.querySelector<HTMLCanvasElement>("#rust")!;
const referenceCanvas = document.querySelector<HTMLCanvasElement>("#reference")!;
const diffCanvas = document.querySelector<HTMLCanvasElement>("#diff")!;
const status = document.querySelector<HTMLElement>("#status")!;
const statsView = document.querySelector<HTMLElement>("#stats")!;
const CAMERA = { position: [0, 21, 25] as Vec3, target: [0, 0, 0.5] as Vec3 };
/** The captured close-up of the blasts (`effects-<theme>-close`). */
const CLOSE_CAMERA = { position: [0, 7, 17] as Vec3, target: [0, 1, 5] as Vec3 };
/** The references show the script frozen at this time. */
const REFERENCE_TIME = 4;

// ------------------------------------------------------------ the script

interface LabTank {
  id: number;
  kind: VehicleKind;
  team: 0 | 1;
  alive: boolean;
  heading: number;
  laser: number;
  previous: { x: number; z: number };
  position: { x: number; y: number; z: number };
  velocity: { x: number; y: number; z: number };
}

/** Tank poses at script time `t`: two drive over the crossroads, one pivots. */
function tankPoses(t: number): LabTank[] {
  const tank = (
    id: number,
    kind: VehicleKind,
    team: 0 | 1,
    at: (t: number) => { x: number; z: number; heading: number },
    laser = 0,
  ): LabTank => {
    const now = at(t);
    const before = at(Math.max(0, t - STEP));
    return {
      id,
      kind,
      team,
      alive: true,
      heading: now.heading,
      laser,
      previous: { x: before.x, z: before.z },
      position: { x: now.x, y: 0.65, z: now.z },
      velocity: { x: (now.x - before.x) / STEP, y: 0, z: (now.z - before.z) / STEP },
    };
  };
  return [
    tank(1, "balanced", 0, (t) => ({ x: -4, z: -26 + 8 * t, heading: 0 })),
    tank(2, "humvee", 1, (t) => ({ x: 22 - 7 * t, z: 2.5, heading: -Math.PI / 2 })),
    tank(3, "heavy", 0, (t) => ({ x: 11, z: -7, heading: t * 1.2 }), 10),
  ];
}

const MUNITIONS: Weapon[] = ["standard", "spread", "rocket", "ricochet", "piercing", "tow"];

/** Every munition in flight across the lower lanes, both teams. */
function shotsAt(t: number): RenderShot[] {
  const shots: RenderShot[] = [];
  MUNITIONS.forEach((weapon, i) => {
    for (const team of [0, 1] as const) {
      const lane = (t * 9 + i * 1.3 + team * 2.5) % 7;
      shots.push({
        id: i * 2 + team + 1,
        weapon,
        team,
        x: -17 + i * 3.2 + team * 1.4,
        z: -12 + lane,
        y: 1.2,
        visualY: 1.4,
        vx: 3,
        vz: 14,
      });
    }
  });
  return shots;
}

/** One event of every kind, fired so each is at a telling moment at t = 4. */
const SCHEDULE: { at: number; event: SimEvent }[] = [
  { at: 3.7, event: { type: "explosion", x: -15, z: 8, size: 3 } },
  { at: 3.72, event: { type: "explosion", x: -8, z: 8, size: 6 } },
  { at: 3.7, event: { type: "destroy", x: -1, z: 8, coverKind: "drum" } },
  { at: 3.7, event: { type: "explosion", x: -1, z: 8, size: 4, coverKind: "drum" } },
  { at: 3.5, event: { type: "death", x: 7, z: 8, size: 3, id: 9 } },
  { at: 3.4, event: { type: "death", x: 15, z: 8, size: 3, id: 10, deathStyle: "burnout" } },
  { at: 3.75, event: { type: "destroy", x: -15, z: 0, coverKind: "timber" } },
  { at: 3.75, event: { type: "destroy", x: -10, z: 0, coverKind: "cargo", color: 0xb47a49 } },
  {
    at: 3.8,
    event: { type: "impact", x: 10, z: 1, coverKind: "tree", height: 5, color: 0x389b58 },
  },
  { at: 3.85, event: { type: "impact", x: 14, z: 1, coverKind: "timber" } },
  { at: 3.92, event: { type: "ricochet", x: -15, z: -5, color: 0xffdf91 } },
  { at: 3.94, event: { type: "shot", x: -12, z: -5, color: 0x008cff } },
  { at: 3.9, event: { type: "hurt", x: 5, z: -2, id: 2, size: 20 } },
  { at: 3.9, event: { type: "impact", x: -9, z: -5 } },
  { at: 3.6, event: { type: "pickup", x: 18, z: -1, color: 0xffcf54 } },
  { at: 3.7, event: { type: "promotion", x: -4, z: 5.5, id: 1, color: 0x7ee0ff } },
  {
    at: 3.95,
    event: { type: "laser", x: 15, z: -12, height: 1.2, from: { x: 11, y: 2.3, z: -7 } },
  },
  { at: 3.95, event: { type: "notice", x: 0, z: 0, label: "ignored" } },
];

function stateAt(t: number) {
  return {
    viewerId: 1,
    tanks: tankPoses(t),
    shots: shotsAt(t),
    elapsed: t,
    match: { phase: "playing" },
    mapTheme: theme,
    covers: [],
    fragments: [],
    mines: [],
    pickups: [],
  };
}

async function createRust(): Promise<EffectsLab> {
  const { EffectsLab } = await loadLabsEngine();
  const lab = await EffectsLab.create(rustCanvas, base);
  lab.set_seed(7);
  lab.set_camera(new Float32Array(CAMERA.position), new Float32Array(CAMERA.target));
  lab.set_state(JSON.stringify(stateAt(0)));
  for (;;) {
    const [compiled, remaining] = lab.prepare_step(4);
    status.textContent = `Compiling pipelines… ${remaining} left`;
    if (remaining === 0) break;
    if (compiled === 0) throw new Error("Pipeline preparation made no progress");
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  while (lab.textures_pending() > 0) {
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
  lab.warm_up();
  lab.reset();
  return lab;
}

async function main() {
  if (!navigator.gpu) throw new Error("WebGPU is required.");
  const rust = await createRust();
  const references = {
    frame: freezeAt === REFERENCE_TIME ? await loadReference(`effects-${theme}`) : undefined,
    close: freezeAt === REFERENCE_TIME ? await loadReference(`effects-${theme}-close`) : undefined,
  };
  let reference = references.frame;
  let t = 0;
  /** One fixed step: fire due events, then update and draw. */
  function step(dt = STEP) {
    const from = t % PERIOD;
    t += dt;
    const to = t % PERIOD;
    rust.set_state(JSON.stringify(stateAt(to)));
    for (const { at, event } of SCHEDULE) {
      const due = to >= from ? at > from && at <= to : at > from || at <= to;
      if (due) {
        rust.event(JSON.stringify(event), false);
      }
    }
    rust.frame(1, dt, t);
  }
  function compare() {
    // Redraw the current state, then diff in the same task.
    step(0);
    if (reference) {
      referenceCanvas.getContext("2d")!.putImageData(reference, 0, 0);
    } else {
      referenceCanvas
        .getContext("2d")!
        .clearRect(0, 0, referenceCanvas.width, referenceCanvas.height);
    }
    return { time: t, ...compareImages(pixels(rustCanvas), reference, diffCanvas) };
  }
  const api = {
    compare,
    stats: () => JSON.parse(rust.stats()),
    error: () => rust.error() ?? null,
    /** Run the script forward by whole fixed steps; the references no longer apply. */
    advance(seconds: number) {
      for (let i = 0; i < Math.round(seconds / STEP); i++) step();
      reference = undefined;
      return api.stats();
    },
    trigger(event: SimEvent) {
      rust.event(JSON.stringify(event), false);
    },
    setCamera(position: Vec3, target: Vec3) {
      rust.set_camera(new Float32Array(position), new Float32Array(target));
      reference = undefined;
    },
    /** The captured close-up pose of the blasts, compared with its reference. */
    closeUp() {
      api.setCamera(CLOSE_CAMERA.position, CLOSE_CAMERA.target);
      reference = references.close;
      return compare();
    },
    reset() {
      rust.reset();
    },
  };
  Object.assign(window, { effectsLab: api });
  for (let i = 0; i < Math.round(freezeAt / STEP); i++) step();
  const report = compare();
  statsView.textContent = JSON.stringify({ ...report, rust: api.stats() }, null, 1);
  document.body.dataset.state = "ready";
  const referenced = references.frame
    ? "compared with the Three.js reference"
    : "no reference for this view: the difference stays blank";
  status.textContent = `${live ? "Live" : `Frozen at t = ${freezeAt} s`}; ${referenced}`;
  if (live) {
    reference = undefined;
    let last = performance.now();
    const loop = (now: number) => {
      try {
        let elapsed = Math.min((now - last) / 1000, 0.25);
        last = now;
        while (elapsed >= STEP) {
          step();
          elapsed -= STEP;
        }
        requestAnimationFrame(loop);
      } catch (error) {
        fail(error);
      }
    };
    requestAnimationFrame(loop);
  }
}

function fail(error: unknown) {
  document.body.dataset.state = "error";
  status.textContent = String(error instanceof Error ? error.message : error);
  console.error(error);
}

main().catch(fail);
