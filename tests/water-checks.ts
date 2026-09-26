import * as THREE from "three/webgpu";
import { WaterSurface } from "../src/game/water-surface";
import type { Presentation } from "../src/game/presentation";
import type { Simulation } from "../src/game/simulation";

/** Read a reflected probe from the actual framebuffer, not just a shader uniform. */
export async function checkWater(view: Presentation, sim: Simulation): Promise<string> {
  const name = sim.mapTheme === "harbor" ? "harbor-water" : "village-creek";
  const water = view.scene.getObjectByName(name);
  if (!(water instanceof WaterSurface)) throw new Error(`${name}: missing Three.js Water`);
  const { renderer, camera } = view;
  const normal = new THREE.Vector3(0, 0, 1).transformDirection(water.matrixWorld);
  if (normal.y < 0.999) throw new Error(`${name}: mirror plane is not horizontal`);
  const position = camera.position.clone();
  const rotation = camera.quaternion.clone();
  const distortion = water.distortionScale.value;
  const reflect = water.reflectionEnabled;
  const probe = new THREE.Mesh(
    new THREE.BoxGeometry(3, 3, 3),
    new THREE.MeshBasicMaterial({ color: 0xff0000 }),
  );
  const [x, z] = sim.mapTheme === "harbor" ? [0, 90] : [-81, -12];
  const level = water.position.y;

  const pixel = new THREE.Vector3(x, level - 3, z);
  const size = renderer.getDrawingBufferSize(new THREE.Vector2());
  const target = new THREE.RenderTarget(size.x, size.y, { type: THREE.UnsignedByteType });
  try {
    probe.position.set(x, level + 3, z);
    view.scene.add(probe);
    camera.position.set(x, level + 14, z + 18);
    camera.lookAt(x, level, z);
    camera.updateMatrixWorld();
    pixel.project(camera);
    water.distortionScale.value = 0;
    const sample = async (color: number) => {
      await new Promise(requestAnimationFrame);
      probe.material.color.setHex(color);
      renderer.setRenderTarget(target);
      renderer.render(view.scene, camera);
      renderer.setRenderTarget(null);
      const y = renderer.coordinateSystem === THREE.WebGPUCoordinateSystem ? -pixel.y : pixel.y;
      return renderer.readRenderTargetPixelsAsync(
        target,
        Math.floor((pixel.x * 0.5 + 0.5) * size.x),
        Math.floor((y * 0.5 + 0.5) * size.y),
        1,
        1,
      );
    };
    // A freshly reset map finishes its pipelines asynchronously, and the reflection
    // pass omits the new probe at first; allow a few frames before it must track it.
    let red: ArrayLike<number> = [];
    let green: ArrayLike<number> = [];
    for (let attempt = 0; attempt < 5; attempt++) {
      red = await sample(0xff0000);
      green = await sample(0x00ff00);
      if (red[0] - green[0] >= 15 && green[1] - red[1] >= 15) break;
    }
    if (red[0] - green[0] < 15 || green[1] - red[1] < 15) {
      throw new Error(`${name}: reflected probe did not change color (${red} / ${green})`);
    }
  } finally {
    renderer.setRenderTarget(null);
    target.dispose();
    view.scene.remove(probe);
    probe.geometry.dispose();
    probe.material.dispose();
    camera.position.copy(position);
    camera.quaternion.copy(rotation);
    water.distortionScale.value = distortion;
  }
  // Objects that newly come into view are drawn once their pipelines compile, so
  // compare draw counts only after they stop changing.
  const settledDrawCalls = async (overview: boolean) => {
    let previous = -1;
    for (let frame = 0; frame < 10; frame++) {
      await new Promise(requestAnimationFrame);
      view.render(sim, 1, 0, overview);
      const calls = renderer.info.render.drawCalls;
      if (calls === previous) break;
      previous = calls;
    }
    return previous;
  };
  const reflected = await settledDrawCalls(true);
  let cached: number;
  try {
    water.reflectionEnabled = false;
    cached = await settledDrawCalls(true);
    if (reflected <= cached) throw new Error(`${name}: no reflection render pass`);
  } finally {
    water.reflectionEnabled = reflect;
  }
  // Moving into the dry center should stop the extra pass without changing the water mesh.
  const zoom = view.zoom;
  sim.human.previous = { x: 0, z: 0 };
  sim.human.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  view.zoom = 23;
  try {
    const dry = await settledDrawCalls(false);
    water.reflectionEnabled = false;
    const unreflected = await settledDrawCalls(false);
    if (dry !== unreflected) {
      throw new Error(`${name}: dry view still reflects (${dry} vs ${unreflected} draw calls)`);
    }
  } finally {
    water.reflectionEnabled = reflect;
    view.zoom = zoom;
  }
  return `${sim.mapName}: reflection pixels + dry-view skip PASS · ${cached} → ${reflected} draw calls`;
}
