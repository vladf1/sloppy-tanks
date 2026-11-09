// Experiment: the Senda reference house, drawn
// by the game's renderer as the coded house and as the Blender-made model
// (public/models/houses/small-monterey.glb), standing or with chosen structure parts broken.
// Build the labs engine with `bun run wasm:labs`; a check drives it through
// `window.homeLab`: `show(kind, broken)` and `view(position, target)`, then screenshots.
import { fail, loadLabsEngine, prepareLab } from "./labs-engine";

type Vec3 = [number, number, number];
const canvas = document.querySelector<HTMLCanvasElement>("#lab")!;
const status = document.querySelector<HTMLElement>("#status")!;
const base = new URL("../", import.meta.url).href;

// Senda's light, roughly: a warm high sun and a pale sky.
const SCENE = {
  background: 0xb9d3ea,
  fog: { color: 0xb9d3ea, near: 60, far: 160 },
  hemisphere: { sky: 0xbdd5f5, ground: 0x8a8070, intensity: 1.6 },
  sun: {
    color: 0xffe2b8,
    intensity: 2.9,
    position: [-30, 55, 40] as Vec3,
    target: [0, 0, 0] as Vec3,
  },
  shadow: { mapSize: 2048, half: 16, near: 0.5, depth: 160, bias: -0.0002, normalBias: 0.05 },
  exposure: 1,
  camera: { fov: 40, near: 0.1, far: 300, position: [0, 2, 16] as Vec3, target: [0, 2, 0] as Vec3 },
  objects: [
    {
      name: "ground",
      geometry: { type: "ground", extent: 40 },
      material: { color: 0x9aa37a, roughness: 1 },
      position: [0, 0, 0] as Vec3,
      receiveShadow: true,
      static: true,
    },
  ],
};

async function main() {
  const { RenderLab } = await loadLabsEngine();
  const lab = await RenderLab.create(canvas, base);
  const glb = new Uint8Array(
    await (await fetch(new URL("models/houses/small-monterey.glb", base))).arrayBuffer(),
  );
  let time = 1.25;
  const homeLab = {
    async show(kind: "coded" | "imported", broken: number[] = []) {
      lab.load_scene(JSON.stringify(SCENE));
      const stats = JSON.parse(
        lab.add_home(
          "home",
          kind === "imported" ? glb : new Uint8Array(),
          new Uint32Array(broken),
          0,
          0,
        ),
      );
      await prepareLab(lab, 8);
      lab.frame(time);
      return { ...stats, renderer: JSON.parse(lab.stats()) };
    },
    /** Several houses of `style` side by side on lots of their own, from the model `model`
     * (`kind` "imported") or coded: `lots` as [left garage, width, ridge, x, z]. */
    async street(
      model: string,
      style: number,
      kind: "coded" | "imported",
      lots: [boolean, number, number, number, number][],
    ) {
      lab.load_scene(JSON.stringify(SCENE));
      const file = new Uint8Array(
        await (await fetch(new URL(`models/houses/${model}.glb`, base))).arrayBuffer(),
      );
      const stats = lots.map(([left, w, h, x, z], i) =>
        JSON.parse(
          lab.add_planned(
            `house-${i}`,
            model,
            kind === "imported" ? file : new Uint8Array(),
            style,
            left,
            w,
            h,
            x,
            z,
          ),
        ),
      );
      await prepareLab(lab, 8);
      lab.frame(time);
      return stats;
    },
    view(position: Vec3, target: Vec3) {
      lab.set_camera(new Float32Array(position), new Float32Array(target));
      lab.frame((time += 0.016));
      return lab.error() ?? null;
    },
  };
  (window as unknown as { homeLab: typeof homeLab }).homeLab = homeLab;
  document.body.dataset.state = "ready";
  status.textContent = "";
}

main().catch(fail);
