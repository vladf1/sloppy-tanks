// Render lab: one calibration scene drawn by the engine's wgpu renderer (lights, fog,
// ACES, PCF shadows, planar water, custom effects, jointed and faded models), beside a
// reference image of the same scene from the game's former Three.js r185 setup and an
// amplified difference image. The references were captured from the Three.js side
// before it left the project (`scripts/README.md`, Labs); without them the lab still
// draws and reports the renderer, and the difference stays blank.
// Build the labs engine with `pnpm run wasm:labs`, then open
// /sloppy-tanks/tools/render-lab.html on the dev server.
// `?freeze=<seconds>` pins the effect/water clock for screenshots (the references use
// 1.25).
import type { RenderLab } from "../src/generated/engine-labs/engine.js";
import { compareImages, loadReference, pixels } from "./lab-references";
import { loadLabsEngine } from "./labs-engine";

type Vec3 = [number, number, number];
interface TextureSpec {
  path?: string;
  generated?: string;
  wrap: "clamp" | "repeat" | "mirror";
  repeat?: [number, number];
  srgb: boolean;
  anisotropy?: number;
  /** Three's `flipY` (default true). */
  flipY?: boolean;
}
interface MaterialSpec {
  shading?: "standard" | "basic";
  color: number;
  roughness?: number;
  metalness?: number;
  emissive?: number;
  emissiveIntensity?: number;
  map?: TextureSpec;
  emissiveMap?: TextureSpec;
  bumpMap?: TextureSpec;
  bumpScale?: number;
  vertexColors?: boolean;
  flatShading?: boolean;
  transparent?: boolean;
  opacity?: number;
  alphaTest?: number;
  alphaToCoverage?: boolean;
  side?: "front" | "back" | "double";
  blending?: "normal" | "additive";
  depthWrite?: boolean;
  /** Three's polygon offset as [factor, units]. */
  polygonOffset?: [number, number];
  effect?: { name: "wave" | "pulse"; params: number[] };
}
type GeometrySpec =
  | { type: "box"; width: number; height: number; depth: number }
  | { type: "sphere"; radius: number; widthSegments: number; heightSegments: number }
  | {
      type: "plane";
      width: number;
      height: number;
      widthSegments?: number;
      heightSegments?: number;
    }
  | { type: "ground"; extent: number };
interface ObjectSpec {
  name: string;
  geometry: GeometrySpec;
  material: MaterialSpec;
  position: Vec3;
  rotation?: Vec3;
  castShadow?: boolean;
  receiveShadow?: boolean;
  static?: boolean;
  instances?: Vec3[];
  /** Named children are movable joints; unnamed ones are rigid parts. */
  children?: ObjectSpec[];
  /** More instances of the same model at these offsets. */
  copies?: Vec3[];
}

const TEAM_BLUE = 0x008cff;
const TEAM_RED = 0xff303e;
const leaf: TextureSpec = { generated: "lab-leaf", wrap: "clamp", srgb: true };
const concrete: TextureSpec = {
  path: "textures/walls/weathered-concrete.webp",
  wrap: "repeat",
  srgb: true,
  anisotropy: 4,
};
const lit = { castShadow: true, receiveShadow: true };

/** Village lighting from presentation.ts with a short lab fog, so fog shows. */
const SCENE = {
  background: 0xaacbc2,
  fog: { color: 0xaacbc2, near: 25, far: 75 },
  hemisphere: { sky: 0xbdd5f5, ground: 0x75859b, intensity: 1.65 },
  sun: {
    color: 0xffd59b,
    intensity: 2.8,
    position: [-45, 68, 25] as Vec3,
    target: [0, 0, 0] as Vec3,
  },
  shadow: { mapSize: 2048, half: 70, near: 0.5, depth: 219.5, bias: -0.0002, normalBias: 0.05 },
  pointLight: {
    color: 0xffc178,
    intensity: 40,
    distance: 20,
    decay: 2,
    position: [-5.5, 2.4, 2] as Vec3,
  },
  exposure: 1,
  camera: {
    fov: 43,
    near: 0.1,
    far: 320,
    position: [0, 9, 30] as Vec3,
    target: [0, 1.5, 0] as Vec3,
  },
  water: { width: 44, depth: 14, height: -0.35, center: [0, 15] as [number, number] },
  objects: [
    {
      name: "ground",
      geometry: { type: "ground", extent: 40 },
      material: {
        color: 0xaee6a6,
        roughness: 1,
        vertexColors: true,
        map: {
          path: "textures/ground/dry-grass.webp",
          wrap: "mirror",
          srgb: true,
          anisotropy: 4,
        },
      },
      position: [0, 0, -12],
      receiveShadow: true,
      static: true,
    },
    ...[
      [0, 0.15, 0xb8b8b8],
      [0, 0.5, 0xb8b8b8],
      [0, 1, 0xb8b8b8],
      [1, 0.3, 0xd4af37],
      [1, 0.7, 0xd4af37],
    ].map(([metalness, roughness, color], i): ObjectSpec => ({
      name: `sphere-${i}`,
      geometry: { type: "sphere", radius: 1.2, widthSegments: 32, heightSegments: 16 },
      material: { color, metalness, roughness },
      position: [-8 + i * 4, 1.2, -4],
      ...lit,
    })),
    ...[TEAM_BLUE, TEAM_RED].map((color, i): ObjectSpec => ({
      name: `team-${i}`,
      geometry: { type: "box", width: 2, height: 2, depth: 2 },
      // model-primitives.ts `material(teamColor)`.
      material: {
        color,
        metalness: 0.05,
        roughness: 0.65,
        emissive: color,
        emissiveIntensity: 0.04,
      },
      position: [-7 + i * 3, 1, 0],
      ...lit,
    })),
    {
      name: "hull",
      geometry: { type: "box", width: 3, height: 1.2, depth: 2 },
      material: { color: 0x4b5d3a, metalness: 0.05, roughness: 0.65 },
      position: [-0.5, 0.6, 0.5],
      rotation: [0, 0.4, 0],
      ...lit,
    },
    {
      name: "flat-rock",
      geometry: { type: "sphere", radius: 1.1, widthSegments: 10, heightSegments: 6 },
      material: { color: 0x8a6f4d, roughness: 0.8, flatShading: true },
      position: [3, 1.1, 0],
      ...lit,
    },
    {
      name: "concrete",
      geometry: { type: "box", width: 2.2, height: 2.2, depth: 2.2 },
      material: {
        color: 0xffffff,
        roughness: 0.95,
        map: concrete,
        bumpMap: concrete,
        bumpScale: 0.06,
      },
      position: [7, 1.1, 0],
      rotation: [0, 0.5, 0],
      ...lit,
    },
    {
      name: "basic",
      geometry: { type: "box", width: 1.2, height: 1.2, depth: 1.2 },
      material: { shading: "basic", color: 0xffe522 },
      position: [-8.5, 0.6, 4.5],
      castShadow: true,
    },
    {
      name: "glass",
      geometry: { type: "box", width: 2, height: 2, depth: 2 },
      material: { color: 0x40a0ff, roughness: 0.3, transparent: true, opacity: 0.45 },
      position: [-5, 1, 4.5],
      ...lit,
    },
    {
      name: "glow",
      geometry: { type: "sphere", radius: 0.9, widthSegments: 24, heightSegments: 12 },
      material: {
        shading: "basic",
        color: 0xff8030,
        transparent: true,
        opacity: 0.8,
        blending: "additive",
        depthWrite: false,
      },
      position: [-2, 1.6, 5],
    },
    {
      name: "cutout",
      geometry: { type: "plane", width: 2.4, height: 2.4 },
      material: { color: 0xffffff, roughness: 0.9, map: leaf, alphaTest: 0.5, side: "double" },
      position: [1, 1.3, 5],
      ...lit,
    },
    {
      name: "coverage",
      geometry: { type: "plane", width: 2.4, height: 2.4 },
      material: {
        color: 0xffffff,
        roughness: 0.9,
        map: leaf,
        alphaTest: 0.5,
        alphaToCoverage: true,
        side: "double",
      },
      position: [3.8, 1.3, 5],
      rotation: [0, -0.3, 0],
      ...lit,
    },
    {
      name: "sheet",
      geometry: { type: "plane", width: 3, height: 2 },
      material: { color: 0xd0a060, roughness: 0.7, side: "double" },
      position: [7, 1.2, 4.5],
      rotation: [0, 2.4, 0],
      ...lit,
    },
    {
      // Transparent and double-sided: Three draws back faces, then front faces.
      name: "veil",
      geometry: { type: "sphere", radius: 1, widthSegments: 24, heightSegments: 12 },
      material: {
        color: 0x9ad0ff,
        roughness: 0.4,
        transparent: true,
        opacity: 0.5,
        side: "double",
      },
      position: [5.5, 1.1, 7],
      castShadow: true,
    },
    {
      name: "flag",
      geometry: { type: "plane", width: 3, height: 2, widthSegments: 24, heightSegments: 8 },
      material: {
        color: 0xd33a2c,
        roughness: 0.8,
        side: "double",
        effect: { name: "wave", params: [0.18, 1.6, 3.0, 0] },
      },
      position: [-9.5, 3.4, -1],
      ...lit,
    },
    {
      name: "pole",
      geometry: { type: "box", width: 0.12, height: 4.6, depth: 0.12 },
      material: { color: 0x777777, metalness: 0.5, roughness: 0.4 },
      position: [-11.05, 2.3, -1],
      ...lit,
      static: true,
    },
    {
      name: "pulse",
      geometry: { type: "box", width: 1.4, height: 2.8, depth: 1.4 },
      material: {
        color: 0x2a9d8f,
        roughness: 0.5,
        transparent: true,
        opacity: 0.85,
        effect: { name: "pulse", params: [0.2, 0.9, 0.6, 2.0, 5.0, 0.35, 0, 0] },
      },
      position: [10.5, 1.4, 4],
      ...lit,
    },
    {
      name: "crates",
      geometry: { type: "box", width: 0.8, height: 0.8, depth: 0.8 },
      material: { color: 0x9c6b3e, roughness: 0.8, metalness: 0.05 },
      position: [-4, 0.4, -9],
      instances: [0, 1, 2, 3, 4, 5].map((i): Vec3 => [i * 1.3, (i % 2) * 0.8, (i % 3) * 0.4]),
      ...lit,
      static: true,
    },
    // A jointed model drawn three times: painted rigid parts merge per joint, and
    // the copies share each merged mesh in one instanced draw.
    {
      name: "tank",
      geometry: { type: "box", width: 2.6, height: 0.7, depth: 1.8 },
      material: { color: 0x4b5d3a, metalness: 0.05, roughness: 0.65 },
      position: [-9, 0.65, -13],
      rotation: [0, 0.3, 0],
      copies: [
        [4, 0, 0],
        [8, 0, 1],
      ],
      ...lit,
      children: [
        ...[-0.95, 0.95].map((z): ObjectSpec => ({
          name: "",
          geometry: { type: "box", width: 2.9, height: 0.55, depth: 0.45 },
          material: { color: 0x2b2b2b, metalness: 0.05, roughness: 0.65 },
          position: [0, -0.2, z],
          ...lit,
        })),
        {
          name: "turret",
          geometry: { type: "box", width: 1.5, height: 0.55, depth: 1.3 },
          material: { color: 0x5a6e45, metalness: 0.05, roughness: 0.65 },
          position: [-0.1, 0.62, 0],
          rotation: [0, 0.5, 0],
          ...lit,
          children: [
            {
              name: "barrel",
              geometry: { type: "box", width: 1.9, height: 0.16, depth: 0.16 },
              material: { color: 0x2b2b2b, metalness: 0.05, roughness: 0.65 },
              position: [1.5, 0.05, 0],
              ...lit,
            },
            {
              name: "",
              geometry: { type: "box", width: 0.5, height: 0.2, depth: 0.5 },
              material: {
                color: TEAM_BLUE,
                metalness: 0.05,
                roughness: 0.65,
                emissive: TEAM_BLUE,
                emissiveIntensity: 0.04,
              },
              position: [-0.3, 0.35, 0],
              ...lit,
            },
          ],
        },
      ],
    },
    {
      // Emissive map: the pictogram-glow path of pickups.
      name: "glow-map",
      geometry: { type: "box", width: 1.4, height: 1.4, depth: 1.4 },
      material: {
        color: 0x404040,
        roughness: 0.6,
        emissive: 0xffb060,
        emissiveIntensity: 0.9,
        emissiveMap: concrete,
      },
      position: [11, 0.7, -3],
      rotation: [0, 0.6, 0],
      ...lit,
    },
    {
      // An unflipped upload (the house tiles' former DataTexture rows).
      name: "unflipped",
      geometry: { type: "plane", width: 2.4, height: 2.4 },
      material: {
        color: 0xffffff,
        roughness: 0.9,
        map: { ...concrete, flipY: false },
        side: "double",
      },
      position: [11, 1.4, -8],
      rotation: [0, -0.5, 0],
      ...lit,
    },
    {
      // A drift over coplanar ground, held on top by its polygon offset.
      name: "decal",
      geometry: { type: "plane", width: 3, height: 2 },
      material: {
        color: 0xc05030,
        roughness: 1,
        transparent: true,
        opacity: 0.8,
        depthWrite: false,
        polygonOffset: [-1, -1],
      },
      position: [1.5, 0, -7],
      rotation: [-Math.PI / 2, 0, 0.3],
      receiveShadow: true,
    },
    {
      name: "column",
      geometry: { type: "box", width: 0.8, height: 5, depth: 0.8 },
      material: { color: 0xc8c8c8, roughness: 0.6 },
      position: [4, 2.5, -9],
      ...lit,
      static: true,
    },
  ] as ObjectSpec[],
};
const base = import.meta.env.BASE_URL;
const params = new URLSearchParams(location.search);
const frozen = params.has("freeze") ? Number(params.get("freeze")) : undefined;

/** A 64×64 cut-out rosette: hard alpha edges for alpha test and coverage. */
function leafPixels(): Uint8Array {
  const size = 64;
  const pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const u = ((x + 0.5) / size) * 2 - 1;
      const v = ((y + 0.5) / size) * 2 - 1;
      const r = Math.hypot(u, v);
      const petal = 0.62 + 0.28 * Math.cos(5 * Math.atan2(v, u));
      const alpha = Math.min(1, Math.max(0, (petal - r) * 8 + 0.5));
      const i = (y * size + x) * 4;
      pixels[i] = Math.round(60 + 80 * (1 - r) + 40 * (y / size));
      pixels[i + 1] = Math.round(140 + 90 * Math.max(0, 1 - r));
      pixels[i + 2] = 50;
      pixels[i + 3] = Math.round(alpha * 255);
    }
  }
  return pixels;
}
const LEAF = leafPixels();

const status = document.querySelector<HTMLElement>("#status")!;
const statsView = document.querySelector<HTMLElement>("#stats")!;
const rustCanvas = document.querySelector<HTMLCanvasElement>("#rust")!;
const referenceCanvas = document.querySelector<HTMLCanvasElement>("#reference")!;
const diffCanvas = document.querySelector<HTMLCanvasElement>("#diff")!;

/** The reference poses: camera `[position, target]` and drawing-buffer size. */
const POSES = {
  default: { camera: [SCENE.camera.position, SCENE.camera.target], size: [640, 400] },
  overhead: {
    camera: [
      [0, 32.32, 24.48],
      [0, 0.7, 0],
    ],
    size: [640, 400],
  },
  close: {
    camera: [
      [-3, 3.2, 21],
      [0, 1.2, 0],
    ],
    size: [640, 400],
  },
  resized: { camera: [SCENE.camera.position, SCENE.camera.target], size: [800, 500] },
} as const satisfies Record<string, { camera: readonly [Vec3, Vec3]; size: readonly number[] }>;
type Pose = keyof typeof POSES;

async function createRust(): Promise<RenderLab> {
  const { RenderLab } = await loadLabsEngine();
  const lab = await RenderLab.create(rustCanvas, base);
  lab.set_generated_texture("lab-leaf", 64, 64, LEAF);
  lab.load_scene(JSON.stringify(SCENE));
  return lab;
}

async function waitFor(check: () => boolean): Promise<void> {
  while (!check()) {
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
}

/** Compile every pipeline in small steps, reporting progress, then warm up. */
async function prepareRust(lab: RenderLab): Promise<void> {
  for (;;) {
    const [compiled, remaining, compiling] = lab.prepare_step(4);
    status.textContent = `Compiling pipelines… ${remaining} left`;
    if (remaining === 0) break;
    if (compiled === 0 && compiling === 0) throw new Error("Pipeline preparation made no progress");
    // Background compiles finish on their own; poll them on a short timer.
    await new Promise((resolve) => setTimeout(resolve, compiled === 0 ? 16 : 0));
  }
  await waitFor(() => {
    // Direct GL compilation may finish before texture fetches. Keep draining uploads.
    lab.prepare_step(0);
    const error = lab.error();
    if (error) throw new Error(error);
    return lab.textures_pending() === 0;
  });
  lab.warm_up();
}

async function main() {
  if (!navigator.gpu) throw new Error("WebGPU is required.");
  const rust = await createRust();
  await prepareRust(rust);
  let time = frozen ?? 0;
  let pose: Pose = "default";
  const references = new Map<Pose, ImageData | undefined>();
  for (const name of Object.keys(POSES) as Pose[]) {
    references.set(name, await loadReference(`render-${name}`));
  }
  function resize(width: number, height: number) {
    for (const canvas of [rustCanvas, referenceCanvas, diffCanvas]) {
      canvas.width = width;
      canvas.height = height;
    }
    rust.resize(width, height);
  }
  /** Draw and diff against the pose's reference in the same task. */
  function compare(t = time) {
    rust.frame(t);
    const reference = references.get(pose);
    const image = pixels(rustCanvas);
    if (reference && reference.width === image.width && reference.height === image.height) {
      referenceCanvas.getContext("2d")!.putImageData(reference, 0, 0);
    } else {
      referenceCanvas
        .getContext("2d")!
        .clearRect(0, 0, referenceCanvas.width, referenceCanvas.height);
    }
    return { time: t, pose, ...compareImages(image, reference, diffCanvas) };
  }
  const api = {
    compare,
    /** The reference poses, `default`, `overhead`, `close` and `resized` (800×500). */
    usePose(name: Pose) {
      pose = name;
      const { camera, size } = POSES[name];
      resize(size[0], size[1]);
      api.setCamera(camera[0], camera[1]);
      return compare();
    },
    stats: () => JSON.parse(rust.stats()),
    error: () => rust.error() ?? null,
    textureFailures: () => rust.texture_failures(),
    setGeneratedTexture: (name: string, width: number, height: number, rgba: Uint8Array) =>
      rust.set_generated_texture(name, width, height, rgba),
    /** Reload the scene as a new round: round resources are released first. */
    reload() {
      rust.load_scene(JSON.stringify(SCENE));
      rust.frame(time);
      return api.stats();
    },
    setCamera(position: readonly number[], target: readonly number[]) {
      rust.set_camera(new Float32Array(position), new Float32Array(target));
    },
    setOpacity: (name: string, opacity: number) => rust.set_opacity(name, opacity),
    /** Traverse a joint (for example a tank turret). */
    poseJoint(name: string, copy: number, joint: string, yaw: number) {
      if (!rust.pose_joint(name, copy, joint, yaw)) throw new Error(`No joint ${name}/${joint}`);
    },
    /** Resize the drawing buffers (the CSS size stays fixed); no reference matches. */
    resize,
    pick: (x: number, y: number, height: number) => Array.from(rust.pick(x, y, height)),
  };
  Object.assign(window, { renderLab: api });
  const report = compare();
  statsView.textContent = JSON.stringify({ ...report, renderer: api.stats() }, null, 1);
  document.body.dataset.state = "ready";
  const referenced = [...references.values()].some(Boolean)
    ? "compared with the Three.js references"
    : "no references found: the difference stays blank";
  status.textContent = `${frozen === undefined ? "Live" : `Frozen at t = ${frozen} s`}; ${referenced}`;
  if (frozen === undefined) {
    const start = performance.now();
    const loop = (now: number) => {
      time = (now - start) / 1000;
      try {
        rust.frame(time);
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
