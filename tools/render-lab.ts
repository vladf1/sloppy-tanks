// Render lab: one calibration scene drawn by the Rust wgpu renderer and by the
// game's Three.js r185 setup (lights, fog, ACES, PCF shadows, planar water), side
// by side with an amplified difference image. Both renderers build from the same
// JSON spec. Build the engine with `pnpm run wasm`, then open
// /sloppy-tanks/tools/render-lab.html on the dev server.
// `?freeze=<seconds>` pins the effect/water clock for screenshots.
import * as THREE from "three/webgpu";
import {
  Fn,
  cos,
  materialEmissive,
  materialOpacity,
  mix,
  normalLocal,
  normalize,
  positionLocal,
  positionWorld,
  sin,
  uniform,
  uv,
  vec3,
} from "three/tsl";
import { WaterSurface } from "../src/game/water-surface";
import init, { RenderLab } from "../src/generated/engine/engine.js";
import wasmUrl from "../src/generated/engine/engine_bg.wasm?url";

type Vec3 = [number, number, number];
interface TextureSpec {
  path?: string;
  generated?: string;
  wrap: "clamp" | "repeat" | "mirror";
  repeat?: [number, number];
  srgb: boolean;
  anisotropy?: number;
}
interface MaterialSpec {
  shading?: "standard" | "basic";
  color: number;
  roughness?: number;
  metalness?: number;
  emissive?: number;
  emissiveIntensity?: number;
  map?: TextureSpec;
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
      name: "column",
      geometry: { type: "box", width: 0.8, height: 5, depth: 0.8 },
      material: { color: 0xc8c8c8, roughness: 0.6 },
      position: [4, 2.5, -9],
      ...lit,
      static: true,
    },
  ] as ObjectSpec[],
};

const status = document.querySelector<HTMLElement>("#status")!;
const statsView = document.querySelector<HTMLElement>("#stats")!;
const rustCanvas = document.querySelector<HTMLCanvasElement>("#rust")!;
const threeCanvas = document.querySelector<HTMLCanvasElement>("#three")!;
const diffCanvas = document.querySelector<HTMLCanvasElement>("#diff")!;
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

// ------------------------------------------------------------ Three.js side

const labTime = uniform(0);
const loads: Promise<unknown>[] = [];
const textureCache = new Map<string, THREE.Texture>();

function threeTexture(spec: TextureSpec): THREE.Texture {
  const key = JSON.stringify(spec);
  let texture = textureCache.get(key);
  if (texture) return texture;
  if (spec.generated) {
    texture = new THREE.DataTexture(LEAF, 64, 64);
    texture.flipY = true;
    texture.generateMipmaps = true;
    texture.needsUpdate = true;
  } else {
    const loaded = new THREE.TextureLoader().loadAsync(`${base}${spec.path}`);
    texture = new THREE.Texture();
    const target = texture;
    loads.push(
      loaded.then((image) => {
        target.image = image.image;
        target.needsUpdate = true;
      }),
    );
  }
  const wrap =
    spec.wrap === "clamp"
      ? THREE.ClampToEdgeWrapping
      : spec.wrap === "mirror"
        ? THREE.MirroredRepeatWrapping
        : THREE.RepeatWrapping;
  texture.wrapS = texture.wrapT = wrap;
  texture.repeat.set(...(spec.repeat ?? [1, 1]));
  texture.colorSpace = spec.srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  texture.anisotropy = spec.anisotropy ?? 1;
  textureCache.set(key, texture);
  return texture;
}

function threeGeometry(spec: GeometrySpec): THREE.BufferGeometry {
  switch (spec.type) {
    case "box":
      return new THREE.BoxGeometry(spec.width, spec.height, spec.depth);
    case "sphere":
      return new THREE.SphereGeometry(spec.radius, spec.widthSegments, spec.heightSegments);
    case "plane":
      return new THREE.PlaneGeometry(
        spec.width,
        spec.height,
        spec.widthSegments,
        spec.heightSegments,
      );
    case "ground": {
      // createArenaFloor("dry-grass") and groundUVs().
      const segments = Math.max(1, Math.round(spec.extent / 2.5));
      const geometry = new THREE.PlaneGeometry(
        spec.extent,
        spec.extent,
        segments,
        segments,
      ).rotateX(-Math.PI / 2);
      const positions = geometry.getAttribute("position");
      const uvs = geometry.getAttribute("uv");
      const colors: number[] = [];
      for (let i = 0; i < positions.count; i++) {
        const x = positions.getX(i);
        const z = positions.getZ(i);
        uvs.setXY(i, x / 8, z / 8);
        const patch =
          0.5 + 0.25 * Math.sin(x * 0.18 + z * 0.09) + 0.25 * Math.sin(z * 0.22 - x * 0.1);
        colors.push(0.68 + patch * 0.28, 0.83 + patch * 0.14, 0.42 + patch * 0.36);
      }
      geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
      return geometry;
    }
  }
}

function threeMaterial(spec: MaterialSpec): THREE.Material {
  const common = {
    color: spec.color,
    map: spec.map ? threeTexture(spec.map) : null,
    vertexColors: spec.vertexColors ?? false,
    transparent: spec.transparent ?? false,
    opacity: spec.opacity ?? 1,
    alphaTest: spec.alphaTest ?? 0,
    alphaToCoverage: spec.alphaToCoverage ?? false,
    side:
      spec.side === "double"
        ? THREE.DoubleSide
        : spec.side === "back"
          ? THREE.BackSide
          : THREE.FrontSide,
    blending: spec.blending === "additive" ? THREE.AdditiveBlending : THREE.NormalBlending,
    depthWrite: spec.depthWrite ?? true,
  };
  if (spec.shading === "basic") {
    return new THREE.MeshBasicNodeMaterial(common);
  }
  const material = new THREE.MeshStandardNodeMaterial({
    ...common,
    roughness: spec.roughness ?? 1,
    metalness: spec.metalness ?? 0,
    emissive: spec.emissive ?? 0,
    emissiveIntensity: spec.emissiveIntensity ?? 1,
    flatShading: spec.flatShading ?? false,
    bumpMap: spec.bumpMap ? threeTexture(spec.bumpMap) : null,
    bumpScale: spec.bumpScale ?? 1,
  });
  const effect = spec.effect;
  if (effect?.name === "wave") {
    // TSL twin of crates/render/src/shaders/effects/wave.wgsl.
    const [amplitude, wavelength, speed] = effect.params;
    const k = (2 * Math.PI) / wavelength;
    material.positionNode = Fn(() => {
      const phase = positionLocal.x.mul(k).sub(labTime.mul(speed));
      const weight = uv().x.clamp(0, 1);
      const slope = weight.mul(cos(phase)).mul(amplitude * k);
      normalLocal.assign(normalize(normalLocal.add(vec3(slope.negate().mul(normalLocal.z), 0, 0))));
      return positionLocal.add(vec3(0, 0, sin(phase).mul(amplitude).mul(weight)));
    })();
  } else if (effect?.name === "pulse") {
    // TSL twin of effects/pulse.wgsl.
    const [r, g, b, speed, frequency, minimum] = effect.params;
    const band = sin(positionWorld.y.mul(frequency).sub(labTime.mul(speed)))
      .mul(0.5)
      .add(0.5);
    material.emissiveNode = materialEmissive.add(vec3(r, g, b).mul(band.mul(band)));
    material.opacityNode = materialOpacity.mul(mix(minimum, 1, band));
  }
  return material;
}

function euler(rotation: Vec3 | undefined): THREE.Euler {
  return new THREE.Euler(...(rotation ?? [0, 0, 0]));
}

async function createThree() {
  const renderer = new THREE.WebGPURenderer({ canvas: threeCanvas, antialias: true });
  await renderer.init();
  renderer.setPixelRatio(1);
  renderer.setSize(threeCanvas.width, threeCanvas.height, false);
  // presentation.ts
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = SCENE.exposure;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(SCENE.background);
  scene.fog = new THREE.Fog(SCENE.fog.color, SCENE.fog.near, SCENE.fog.far);
  // createLighting()
  const fill = new THREE.HemisphereLight(
    SCENE.hemisphere.sky,
    SCENE.hemisphere.ground,
    SCENE.hemisphere.intensity,
  );
  scene.add(fill);
  const sun = new THREE.DirectionalLight(SCENE.sun.color, SCENE.sun.intensity);
  sun.position.set(...SCENE.sun.position);
  sun.target.position.set(...SCENE.sun.target);
  scene.add(sun.target);
  sun.castShadow = true;
  sun.shadow.mapSize.set(SCENE.shadow.mapSize, SCENE.shadow.mapSize);
  const shadowCamera = sun.shadow.camera;
  shadowCamera.left = shadowCamera.bottom = -SCENE.shadow.half;
  shadowCamera.right = shadowCamera.top = SCENE.shadow.half;
  shadowCamera.near = SCENE.shadow.near;
  shadowCamera.far = SCENE.shadow.near + SCENE.shadow.depth;
  shadowCamera.updateProjectionMatrix();
  sun.shadow.normalBias = SCENE.shadow.normalBias;
  sun.shadow.bias = SCENE.shadow.bias;
  scene.add(sun);
  const point = SCENE.pointLight;
  const flash = new THREE.PointLight(point.color, point.intensity, point.distance, point.decay);
  flash.position.set(...point.position);
  scene.add(flash);
  const byName = new Map<string, THREE.Object3D[]>();
  function build(object: ObjectSpec): THREE.Mesh {
    const geometry = threeGeometry(object.geometry);
    const material = threeMaterial(object.material);
    let mesh: THREE.Mesh;
    if (object.instances) {
      const instanced = new THREE.InstancedMesh(geometry, material, object.instances.length);
      object.instances.forEach((offset, i) =>
        instanced.setMatrixAt(i, new THREE.Matrix4().makeTranslation(...offset)),
      );
      mesh = instanced;
    } else {
      mesh = new THREE.Mesh(geometry, material);
    }
    mesh.position.set(...object.position);
    mesh.rotation.copy(euler(object.rotation));
    mesh.castShadow = object.castShadow ?? false;
    mesh.receiveShadow = object.receiveShadow ?? false;
    mesh.name = object.name;
    for (const child of object.children ?? []) {
      mesh.add(build(child));
    }
    return mesh;
  }
  for (const object of SCENE.objects) {
    const mesh = build(object);
    const copies = [mesh];
    for (const offset of object.copies ?? []) {
      const copy = mesh.clone();
      copy.position.add(new THREE.Vector3(...offset));
      copies.push(copy);
    }
    byName.set(object.name, copies);
    scene.add(...copies);
  }
  const waterSpec = SCENE.water;
  const water = new WaterSurface(
    new THREE.PlaneGeometry(waterSpec.width, waterSpec.depth),
    "harbor",
    waterSpec.height,
  );
  water.position.x = waterSpec.center[0];
  water.position.z = waterSpec.center[1];
  scene.add(water);
  const camera = new THREE.PerspectiveCamera(
    SCENE.camera.fov,
    threeCanvas.width / threeCanvas.height,
    SCENE.camera.near,
    SCENE.camera.far,
  );
  camera.position.set(...SCENE.camera.position);
  camera.lookAt(...SCENE.camera.target);
  await Promise.all(loads);
  return {
    renderer,
    frame(time: number) {
      labTime.value = time;
      water.update(time);
      renderer.render(scene, camera);
    },
    setCamera(position: Vec3, target: Vec3) {
      camera.position.set(...position);
      camera.lookAt(...target);
    },
    poseJoint(name: string, copy: number, joint: string, yaw: number) {
      const node = byName.get(name)?.[copy]?.getObjectByName(joint);
      if (node) node.rotation.y += yaw;
    },
    resize(width: number, height: number) {
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    },
  };
}

// ------------------------------------------------------------ Rust side

async function createRust() {
  await init({ module_or_path: wasmUrl });
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
    const [compiled, remaining] = lab.prepare_step(4);
    status.textContent = `Compiling pipelines… ${remaining} left`;
    if (remaining === 0) break;
    if (compiled === 0) throw new Error("Pipeline preparation made no progress");
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  await waitFor(() => lab.textures_pending() === 0);
  lab.warm_up();
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
  await prepareRust(rust);
  let time = frozen ?? 0;
  const start = performance.now();
  function drawBoth(t: number) {
    rust.frame(t);
    three.frame(t);
  }
  /** Draw both at the same time and diff them in the same task. */
  function compare(t = time) {
    drawBoth(t);
    const a = pixels(rustCanvas);
    const b = pixels(threeCanvas);
    const diff = new ImageData(a.width, a.height);
    const grid = 4;
    const cells = Array.from({ length: grid * grid }, () => ({ sum: 0, count: 0 }));
    let sum = 0;
    let max = 0;
    const mean = [
      [0, 0, 0],
      [0, 0, 0],
    ];
    for (let i = 0; i < a.data.length; i += 4) {
      let pixel = 0;
      for (let c = 0; c < 3; c++) {
        const d = Math.abs(a.data[i + c] - b.data[i + c]);
        pixel += d;
        mean[0][c] += a.data[i + c];
        mean[1][c] += b.data[i + c];
        diff.data[i + c] = Math.min(255, d * 4);
      }
      diff.data[i + 3] = 255;
      pixel /= 3;
      sum += pixel;
      max = Math.max(max, pixel);
      const p = i / 4;
      const x = Math.floor(((p % a.width) / a.width) * grid);
      const y = Math.floor((Math.floor(p / a.width) / a.height) * grid);
      cells[y * grid + x].sum += pixel;
      cells[y * grid + x].count++;
    }
    diffCanvas.getContext("2d")!.putImageData(diff, 0, 0);
    const count = a.data.length / 4;
    return {
      time: t,
      meanAbsDiff: sum / count,
      maxAbsDiff: max,
      rustMeanRgb: mean[0].map((v) => v / count),
      threeMeanRgb: mean[1].map((v) => v / count),
      cellMeanDiff: cells.map((cell) => Math.round((cell.sum / cell.count) * 10) / 10),
    };
  }
  const api = {
    compare,
    stats: () => JSON.parse(rust.stats()),
    error: () => rust.error() ?? null,
    textureFailures: () => rust.texture_failures(),
    /** Reload the scene as a new round: round resources are released first. */
    reload() {
      rust.load_scene(JSON.stringify(SCENE));
      rust.frame(time);
      return api.stats();
    },
    setCamera(position: Vec3, target: Vec3) {
      rust.set_camera(new Float32Array(position), new Float32Array(target));
      three.setCamera(position, target);
    },
    setOpacity: (name: string, opacity: number) => rust.set_opacity(name, opacity),
    /** Traverse a joint (for example a tank turret) in both renderers. */
    poseJoint(name: string, copy: number, joint: string, yaw: number) {
      if (!rust.pose_joint(name, copy, joint, yaw)) throw new Error(`No joint ${name}/${joint}`);
      three.poseJoint(name, copy, joint, yaw);
    },
    /** Resize both drawing buffers (the CSS size stays fixed). */
    resize(width: number, height: number) {
      for (const canvas of [rustCanvas, threeCanvas, diffCanvas]) {
        canvas.width = width;
        canvas.height = height;
      }
      rust.resize(width, height);
      three.resize(width, height);
    },
    pick: (x: number, y: number, height: number) => Array.from(rust.pick(x, y, height)),
  };
  Object.assign(window, { renderLab: api });
  const report = compare();
  statsView.textContent = JSON.stringify({ ...report, renderer: api.stats() }, null, 1);
  document.body.dataset.state = "ready";
  status.textContent = frozen === undefined ? "Live" : `Frozen at t = ${frozen} s`;
  if (frozen === undefined) {
    const loop = (now: number) => {
      time = (now - start) / 1000;
      try {
        drawBoth(time);
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
