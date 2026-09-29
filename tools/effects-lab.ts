// Effects lab: one scripted scene (tanks laying tracks and dust, every munition in
// flight, and one event of every kind) fed to the Rust wgpu effects and to the
// game's former Three.js effect classes, side by side with a difference image.
// Both sides get the same state and events each fixed step; only their cosmetic
// random streams differ. Build the engine with `pnpm run wasm`, then open
// /sloppy-tanks/tools/effects-lab.html on the dev server.
// `?t=<seconds>` (default 4) runs the script to that time and freezes it;
// `?live` loops it. `?theme=quarry|harbor` switches dust colors and effects.
import * as THREE from "three/webgpu";
import { LaserVisuals } from "../src/game/laser-visuals";
import { tankModel } from "../src/game/models";
import { ParticleEffects } from "../src/game/particle-effects";
import { ProjectileVisuals } from "../src/game/projectile-visuals";
import { QuarryDust } from "../src/game/quarry-dust";
import type { RenderShot, RenderState } from "../src/game/render-state";
import { TrackDust } from "../src/game/track-dust";
import { TrackTrails } from "../src/game/tracks";
import type { SimEvent, VehicleKind, Weapon } from "../src/game/types";
import { FEEDBACK } from "../src/game/view-settings";
import { VEHICLES } from "../src/game/data";
import init, { EffectsLab } from "../src/generated/engine/engine.js";
import wasmUrl from "../src/generated/engine/engine_bg.wasm?url";

type Vec3 = [number, number, number];
const STEP = 1 / 60;
const PERIOD = 5;
const params = new URLSearchParams(location.search);
const live = params.has("live");
const freezeAt = Number(params.get("t") ?? 4);
const theme = params.get("theme") ?? "village";
const base = import.meta.env.BASE_URL;
const rustCanvas = document.querySelector<HTMLCanvasElement>("#rust")!;
const threeCanvas = document.querySelector<HTMLCanvasElement>("#three")!;
const diffCanvas = document.querySelector<HTMLCanvasElement>("#diff")!;
const status = document.querySelector<HTMLElement>("#status")!;
const statsView = document.querySelector<HTMLElement>("#stats")!;
const CAMERA = { position: [0, 21, 25] as Vec3, target: [0, 0, 0.5] as Vec3 };

/** Repeatable cosmetic randomness for the Three side (the game used Math.random). */
function mulberry32(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
Math.random = mulberry32(7);

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

function stateAt(t: number): RenderState {
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
  } as unknown as RenderState;
}

// ------------------------------------------------------------ Three side

async function createThree() {
  const renderer = new THREE.WebGPURenderer({ canvas: threeCanvas, antialias: true });
  await renderer.init();
  renderer.setPixelRatio(1);
  renderer.setSize(threeCanvas.width, threeCanvas.height, false);
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  const scene = new THREE.Scene();
  // The village theme from presentation.ts reset().
  scene.background = new THREE.Color(0xaacbc2);
  scene.fog = new THREE.Fog(0xaacbc2, 210, 380);
  scene.add(new THREE.HemisphereLight(0xbdd5f5, 0x75859b, 1.65));
  const sun = new THREE.DirectionalLight(0xffd59b, 2.8);
  sun.position.set(-45, 68, 25);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const shadowCamera = sun.shadow.camera;
  shadowCamera.left = shadowCamera.bottom = -70;
  shadowCamera.right = shadowCamera.top = 70;
  shadowCamera.near = 0.5;
  shadowCamera.far = 0.5 + 219.5;
  shadowCamera.updateProjectionMatrix();
  sun.shadow.normalBias = 0.05;
  sun.shadow.bias = -0.0002;
  scene.add(sun, sun.target);
  const flash = new THREE.PointLight(0xffc178, 0, 20, 2);
  scene.add(flash);
  const ground = new THREE.Mesh(
    new THREE.PlaneGeometry(160, 160).rotateX(-Math.PI / 2),
    new THREE.MeshStandardMaterial({ color: 0x9a8260, roughness: 1, metalness: 0 }),
  );
  ground.receiveShadow = true;
  scene.add(ground);
  const particles = new ParticleEffects();
  const tracks = new TrackTrails();
  const dust = new TrackDust();
  const quarryDust = new QuarryDust();
  quarryDust.mesh.visible = theme === "quarry";
  const projectiles = new ProjectileVisuals();
  const laser = new LaserVisuals();
  scene.add(tracks.mesh, dust.mesh, dust.gravel.mesh, quarryDust.mesh);
  scene.add(projectiles.group, laser.group, particles.mesh, particles.explosions.group);
  const tanks = new Map<number, THREE.Object3D>();
  // presentation.ts pickupEffect()/updatePickupEffects().
  const ringGeometry = new THREE.RingGeometry(0.88, 1, 48);
  const glowGeometry = new THREE.SphereGeometry(1, 16, 10);
  let pickupEffects: { group: THREE.Group; age: number; tankId?: number }[] = [];
  function pickupEffect(color: number) {
    const group = new THREE.Group();
    const ringMaterial = new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity: 0.9,
      depthWrite: false,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
    });
    const glowMaterial = ringMaterial.clone();
    glowMaterial.opacity = 0.2;
    glowMaterial.side = THREE.BackSide;
    const ring = new THREE.Mesh(ringGeometry, ringMaterial);
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = 0.08;
    group.add(ring, new THREE.Mesh(glowGeometry, glowMaterial));
    return group;
  }
  const camera = new THREE.PerspectiveCamera(43, threeCanvas.width / threeCanvas.height, 0.1, 320);
  camera.position.set(...CAMERA.position);
  camera.lookAt(...CAMERA.target);
  let time = 0;
  return {
    /** presentation.ts event(), minus HUD and reticle feedback. */
    event(event: SimEvent) {
      if (event.type === "debris-impact" || event.type === "notice") return;
      laser.event(event);
      if (event.type === "hurt" && (event.id === undefined || (event.size ?? 0) <= 0)) return;
      if (event.type === "respawn") return;
      if (event.type === "pickup" || event.type === "promotion") {
        const group = pickupEffect(event.color ?? 0xffffff);
        group.position.set(event.x, 0, event.z);
        scene.add(group);
        pickupEffects.push({ group, age: 0, tankId: event.id });
      }
      if (particles.event(event)) {
        flash.position.set(event.x, 3, event.z);
        flash.intensity = 45;
      }
    },
    /** presentation.ts render() effect updates, in its order. */
    frame(state: RenderState, alpha: number, dt: number) {
      time += dt;
      pickupEffects = pickupEffects.filter((effect) => {
        effect.age += dt;
        const progress = effect.age / FEEDBACK.pickupSeconds;
        if (progress >= 1) {
          scene.remove(effect.group);
          return false;
        }
        const [ring, glow] = effect.group.children as THREE.Mesh<
          THREE.BufferGeometry,
          THREE.MeshBasicMaterial
        >[];
        ring.scale.setScalar(1 + progress * 3);
        ring.material.opacity = 0.85 * (1 - progress) ** 2;
        const tank = state.tanks.find((tank) => tank.id === effect.tankId && tank.alive);
        glow.visible = !!tank;
        if (tank) {
          glow.position.set(
            THREE.MathUtils.lerp(tank.previous.x, tank.position.x, alpha) - effect.group.position.x,
            1.1,
            THREE.MathUtils.lerp(tank.previous.z, tank.position.z, alpha) - effect.group.position.z,
          );
          glow.scale
            .set(1.65, 1.25, 1.9)
            .multiplyScalar(VEHICLES[tank.kind].scale * (1 + progress * 0.15));
          glow.material.opacity = 0.2 * (1 - progress) ** 2;
        }
        return true;
      });
      tracks.update(state, alpha);
      dust.update(state);
      quarryDust.update(state, dt);
      flash.intensity *= Math.exp(-dt * FEEDBACK.flashDecay);
      for (const tank of state.tanks) {
        let model = tanks.get(tank.id);
        if (!model) {
          model = tankModel(tank.kind, tank.team);
          tanks.set(tank.id, model);
          scene.add(model);
        }
        model.position.set(
          THREE.MathUtils.lerp(tank.previous.x, tank.position.x, alpha),
          tank.position.y - 0.4,
          THREE.MathUtils.lerp(tank.previous.z, tank.position.z, alpha),
        );
        model.rotation.y = tank.heading;
      }
      projectiles.update(state.shots, time);
      laser.update(state, alpha, dt);
      particles.update(dt, time);
      renderer.render(scene, camera);
    },
    setCamera(position: Vec3, target: Vec3) {
      camera.position.set(...position);
      camera.lookAt(...target);
    },
  };
}

// ------------------------------------------------------------ Rust side

async function createRust() {
  await init({ module_or_path: wasmUrl });
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

function pixels(canvas: HTMLCanvasElement): ImageData {
  const copy = document.createElement("canvas");
  copy.width = canvas.width;
  copy.height = canvas.height;
  const context = copy.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(canvas, 0, 0);
  return context.getImageData(0, 0, canvas.width, canvas.height);
}

async function main() {
  if (!navigator.gpu) throw new Error("WebGPU is required.");
  const [rust, three] = await Promise.all([createRust(), createThree()]);
  let t = 0;
  /** One fixed step: fire due events, then update and draw both sides. */
  function step(dt = STEP) {
    const from = t % PERIOD;
    t += dt;
    const to = t % PERIOD;
    const state = stateAt(to);
    rust.set_state(JSON.stringify(state));
    for (const { at, event } of SCHEDULE) {
      const due = to >= from ? at > from && at <= to : at > from || at <= to;
      if (due) {
        rust.event(JSON.stringify(event), false);
        three.event(event);
      }
    }
    rust.frame(1, dt, t);
    three.frame(state, 1, dt);
  }
  function compare() {
    // Redraw the current state in both, then diff in the same task.
    step(0);
    const a = pixels(rustCanvas);
    const b = pixels(threeCanvas);
    const diff = new ImageData(a.width, a.height);
    let sum = 0;
    for (let i = 0; i < a.data.length; i += 4) {
      let pixel = 0;
      for (let c = 0; c < 3; c++) {
        const d = Math.abs(a.data[i + c] - b.data[i + c]);
        pixel += d;
        diff.data[i + c] = Math.min(255, d * 4);
      }
      diff.data[i + 3] = 255;
      sum += pixel / 3;
    }
    diffCanvas.getContext("2d")!.putImageData(diff, 0, 0);
    return { time: t, meanAbsDiff: sum / (a.data.length / 4) };
  }
  const api = {
    compare,
    stats: () => JSON.parse(rust.stats()),
    error: () => rust.error() ?? null,
    /** Run the script forward by whole fixed steps. */
    advance(seconds: number) {
      for (let i = 0; i < Math.round(seconds / STEP); i++) step();
      return api.stats();
    },
    trigger(event: SimEvent) {
      rust.event(JSON.stringify(event), false);
      three.event(event);
    },
    setCamera(position: Vec3, target: Vec3) {
      rust.set_camera(new Float32Array(position), new Float32Array(target));
      three.setCamera(position, target);
    },
    reset() {
      rust.reset();
    },
  };
  Object.assign(window, { effectsLab: api });
  api.advance(freezeAt);
  const report = compare();
  statsView.textContent = JSON.stringify({ ...report, rust: api.stats() }, null, 1);
  document.body.dataset.state = "ready";
  status.textContent = live ? "Live" : `Frozen at t = ${freezeAt} s`;
  if (live) {
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
