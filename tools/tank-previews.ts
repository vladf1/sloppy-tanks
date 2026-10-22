// Tank selection previews rendered from the game's own vehicle models by the engine's
// renderer (the labs build's `RenderLab`): one 640×400 transparent image per team and
// chassis. `scripts/generate-previews.mjs` packs them into `public/previews/tanks.webp`;
// `tools/tank-surface-check.html` shows them at card and gameplay scale.
//
// The lighting is a garage card's: a dim warm hemisphere fill over a dark floor, a
// warm, shadow-casting key light from the front left (out of the decks' specular
// reflection, which would wash them pink) and a lowered exposure that keeps the
// red paint below ACES's shoulder (it turns bright red pink), and a strong, near
// white rim light from behind on the right that lifts the side the card shows off
// the navy card and edges the silhouette (a distant point light without falloff,
// which lights like a directional one; a rim from the left or above lands in the
// decks' specular and washes them pink), ACES tone mapping and a 7 m wide
// orthographic view from (8, 6, 6). The renderer has no orthographic camera, so a 0.6° perspective
// camera 400 m away frames the same view.
// Transparency comes from difference matting: each tank is drawn over black and over
// white, and the two frames give every pixel's coverage and its unblended color. A soft
// contact shadow, the tracks' footprint projected through the same view and blurred, is
// painted under each tank so it sits on the card instead of floating over it.
import { pixels } from "./lab-references";
import { loadLabsEngine, prepareLab } from "./labs-engine";

export const PREVIEW_WIDTH = 640;
export const PREVIEW_HEIGHT = 400;
export const PREVIEW_KINDS = ["scout", "balanced", "heavy"] as const;
/** Each preview renders at this multiple of its size. One: the paint's edge wear
 * resolves by on-screen size, and drawn finer than the card shows it, every edge
 * would chip into a pale outline. */
const SUPERSAMPLE = 1;
const PREPARE_BUDGET = 8;
/** Half the orthographic view's height (metres), and its look-at pose. */
const HALF_HEIGHT = 2.1875;
const CAMERA_FROM = [8, 6, 6];
const CAMERA_TARGET = [0, 0.6, 0.65];
/** How far the near-orthographic camera stands back. */
const CAMERA_DISTANCE = 400;
/** The rim light stands this far out along its direction, so its rays are parallel. */
const RIM_DISTANCE = 10000;

type Vec3 = [number, number, number];

function camera() {
  const direction = CAMERA_FROM.map((value, i) => value - CAMERA_TARGET[i]);
  const length = Math.hypot(...direction);
  const position = CAMERA_TARGET.map(
    (value, i) => value + (direction[i] / length) * CAMERA_DISTANCE,
  ) as Vec3;
  return {
    fov: (2 * Math.atan(HALF_HEIGHT / CAMERA_DISTANCE) * 180) / Math.PI,
    near: CAMERA_DISTANCE - 20,
    far: CAMERA_DISTANCE + 20,
    position,
    target: CAMERA_TARGET as Vec3,
  };
}

const rim = [4, 3, -4];
const rimLength = Math.hypot(...rim);

/** Each chassis' track footprint (width, length in metres, the vehicle scale applied;
 * `tank_dimensions`), its ground height, and the contact shadow's look. */
const FOOTPRINTS: Record<(typeof PREVIEW_KINDS)[number], [number, number]> = {
  scout: [1.91, 3.72],
  balanced: [1.95, 4.27],
  heavy: [2.14, 4.7],
};
const GROUND_Y = -0.17;
const SHADOW_GROW = 0.35;
const SHADOW_BLUR = 11;
const SHADOW_OPACITY = 0.82;

const SCENE = {
  background: 0x000000,
  hemisphere: { sky: 0xfff2e4, ground: 0x3a3f47, intensity: 0.6 },
  sun: { color: 0xfff1de, intensity: 3.4, position: [-3, 7, 5], target: [0, 0, 0] },
  // The key casts shadows, so the turret, mantlet and stowage shade the deck and
  // the skirts shade the running gear: the card tanks read as solid models.
  shadow: {
    enabled: true,
    mapSize: 2048,
    half: 3.6,
    near: 0.5,
    depth: 20,
    bias: -0.0002,
    normalBias: 0.02,
  },
  pointLight: {
    color: 0xd6e3ff,
    intensity: 11,
    distance: 0,
    decay: 0,
    position: rim.map((value) => (value / rimLength) * RIM_DISTANCE),
  },
  exposure: 0.82,
  // The game's sky reflection, so card paint and steel shade as they do in a round.
  reflections: 1,
  camera: camera(),
  objects: [],
};

/** The canvas pixel (at preview size) of a world point through the near-orthographic
 * card camera. */
function project([x, y, z]: Vec3): [number, number] {
  const { position, target } = SCENE.camera;
  const forward = target.map((value, i) => value - position[i]);
  const normalize = (v: number[]) => v.map((value) => value / Math.hypot(...v));
  const f = normalize(forward);
  const right = normalize([f[1] * 0 - f[2] * 1, f[2] * 0 - f[0] * 0, f[0] * 1 - f[1] * 0]);
  const up = [
    right[1] * f[2] - right[2] * f[1],
    right[2] * f[0] - right[0] * f[2],
    right[0] * f[1] - right[1] * f[0],
  ];
  const offset = [x - target[0], y - target[1], z - target[2]];
  const dot = (a: number[], b: number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const pixelsPerMetre = PREVIEW_HEIGHT / (2 * HALF_HEIGHT);
  return [
    PREVIEW_WIDTH / 2 + dot(offset, right) * pixelsPerMetre,
    PREVIEW_HEIGHT / 2 - dot(offset, up) * pixelsPerMetre,
  ];
}

/** A soft contact shadow under a chassis: its grown footprint, blurred. */
function contactShadow(context: CanvasRenderingContext2D, kind: (typeof PREVIEW_KINDS)[number]) {
  const [width, length] = FOOTPRINTS[kind];
  const [hx, hz] = [width / 2 + SHADOW_GROW, length / 2 + SHADOW_GROW];
  const corners: Vec3[] = [
    [-hx, GROUND_Y, -hz],
    [hx, GROUND_Y, -hz],
    [hx, GROUND_Y, hz],
    [-hx, GROUND_Y, hz],
  ];
  context.save();
  context.filter = `blur(${SHADOW_BLUR}px)`;
  context.fillStyle = `rgba(0, 0, 0, ${SHADOW_OPACITY})`;
  context.beginPath();
  for (const [i, corner] of corners.entries()) {
    const [px, py] = project(corner);
    if (i === 0) context.moveTo(px, py);
    else context.lineTo(px, py);
  }
  context.closePath();
  context.fill();
  context.restore();
}

/** Coverage and color from one frame over black and one over white. */
function matte(black: Uint8ClampedArray, white: Uint8ClampedArray): ImageData {
  const image = new ImageData(PREVIEW_WIDTH * SUPERSAMPLE, PREVIEW_HEIGHT * SUPERSAMPLE);
  // The backgrounds as drawn (tone mapping changes white), from a corner pixel.
  const backgroundBlack = [black[0], black[1], black[2]];
  const backgroundWhite = [white[0], white[1], white[2]];
  for (let i = 0; i < black.length; i += 4) {
    let transmitted = 0;
    for (let c = 0; c < 3; c++) {
      transmitted +=
        (white[i + c] - black[i + c]) / Math.max(1, backgroundWhite[c] - backgroundBlack[c]);
    }
    const alpha = Math.min(1, Math.max(0, 1 - transmitted / 3));
    for (let c = 0; c < 3; c++) {
      image.data[i + c] = alpha > 0 ? (black[i + c] - (1 - alpha) * backgroundBlack[c]) / alpha : 0;
    }
    image.data[i + 3] = Math.round(alpha * 255);
  }
  return image;
}

/** `{ "0-scout": dataURL, … }`: lossless PNG previews for both teams. */
export async function renderTankPreviews(): Promise<Record<string, string>> {
  const { RenderLab } = await loadLabsEngine();
  const canvas = document.createElement("canvas");
  canvas.width = PREVIEW_WIDTH * SUPERSAMPLE;
  canvas.height = PREVIEW_HEIGHT * SUPERSAMPLE;
  canvas.style.cssText = `width:${canvas.width}px;height:${canvas.height}px;position:fixed;left:-9999px`;
  document.body.append(canvas);
  const lab = await RenderLab.create(canvas, import.meta.env.BASE_URL);
  try {
    lab.load_scene(JSON.stringify(SCENE));
    lab.resize(canvas.width, canvas.height);
    const names: string[] = [];
    for (const team of [0, 1]) {
      for (const kind of PREVIEW_KINDS) {
        const name = `${team}-${kind}`;
        lab.add_vehicle(name, kind, team, 0, 0, 0, 0);
        names.push(name);
      }
    }
    await prepareLab(lab, PREPARE_BUDGET);
    const failures = lab.texture_failures();
    if (failures.length) throw new Error(`Textures failed: ${failures.join(", ")}`);
    const previews: Record<string, string> = {};
    const large = document.createElement("canvas");
    large.width = canvas.width;
    large.height = canvas.height;
    const output = document.createElement("canvas");
    output.width = PREVIEW_WIDTH;
    output.height = PREVIEW_HEIGHT;
    const context = output.getContext("2d")!;
    context.imageSmoothingQuality = "high";
    for (const name of names) {
      for (const other of names) lab.set_visible(other, other === name);
      lab.set_background(0x000000);
      lab.frame(0);
      const black = pixels(canvas).data;
      lab.set_background(0xffffff);
      lab.frame(0);
      const white = pixels(canvas).data;
      large.getContext("2d")!.putImageData(matte(black, white), 0, 0);
      context.clearRect(0, 0, PREVIEW_WIDTH, PREVIEW_HEIGHT);
      contactShadow(context, name.split("-")[1] as (typeof PREVIEW_KINDS)[number]);
      context.drawImage(large, 0, 0, PREVIEW_WIDTH, PREVIEW_HEIGHT);
      previews[name] = output.toDataURL("image/png");
    }
    const error = lab.error();
    if (error) throw new Error(error);
    return previews;
  } finally {
    lab.free();
    canvas.remove();
  }
}
