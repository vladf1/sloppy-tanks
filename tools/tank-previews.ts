// Tank selection previews rendered from the game's own vehicle models by the engine's
// renderer (the labs build's `RenderLab`): one 640×400 transparent image per team and
// chassis. `scripts/generate-previews.mjs` packs them into `public/previews/tanks.webp`;
// `tools/tank-surface-check.html` shows them at card and gameplay scale.
//
// The lighting and framing are the former Three.js card renderer's: a hemisphere light,
// a warm key light from the front left and a cool rim light from behind (a distant point
// light without falloff, which lights like a directional one), no shadows, ACES tone
// mapping and a 7 m wide orthographic view from (8, 6, 6). The renderer has no
// orthographic camera, so a 0.6° perspective camera 400 m away frames the same view.
// Transparency comes from difference matting: each tank is drawn over black and over
// white, and the two frames give every pixel's coverage and its unblended color.
import { pixels } from "./lab-references";
import { loadLabsEngine, prepareLab } from "./labs-engine";

export const PREVIEW_WIDTH = 640;
export const PREVIEW_HEIGHT = 400;
export const PREVIEW_KINDS = ["scout", "balanced", "heavy"] as const;
/** Each preview renders at this multiple of its size and is downsampled, so the
 * tank's edges stay smooth on the cards' dark background. */
const SUPERSAMPLE = 2;
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

const SCENE = {
  background: 0x000000,
  hemisphere: { sky: 0xdcedff, ground: 0x4c6075, intensity: 2.4 },
  sun: { color: 0xfff1d5, intensity: 3.5, position: [-3, 7, 5], target: [0, 0, 0] },
  shadow: {
    enabled: false,
    mapSize: 512,
    half: 5,
    near: 0.5,
    depth: 20,
    bias: 0,
    normalBias: 0,
  },
  pointLight: {
    color: 0x91cfff,
    intensity: 2,
    distance: 0,
    decay: 0,
    position: rim.map((value) => (value / rimLength) * RIM_DISTANCE),
  },
  exposure: 1,
  // The game's sky reflection, so card paint and steel shade as they do in a round.
  reflections: 1,
  camera: camera(),
  objects: [],
};

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
