import * as THREE from "three";
import { WaterSurface } from "../src/game/water-surface";
import type { Presentation } from "../src/game/presentation";
import type { Simulation } from "../src/game/simulation";

/** Read a reflected probe from the actual framebuffer, not just a shader uniform. */
export function checkWater(view: Presentation, sim: Simulation): string {
  const name = sim.mapTheme === "harbor" ? "harbor-water" : "village-creek";
  const water = view.scene.getObjectByName(name);
  if (!(water instanceof WaterSurface)) throw new Error(`${name}: missing Three.js Water`);
  const { renderer, camera } = view;
  const normal = new THREE.Vector3(0, 0, 1).transformDirection(water.matrixWorld);
  if (normal.y < 0.999) throw new Error(`${name}: mirror plane is not horizontal`);
  const position = camera.position.clone();
  const rotation = camera.quaternion.clone();
  const distortion = water.material.uniforms.distortionScale.value;
  const reflect = water.onBeforeRender;
  const probe = new THREE.Mesh(
    new THREE.BoxGeometry(3, 3, 3),
    new THREE.MeshBasicMaterial({ color: 0xff0000 }),
  );
  const [x, z] = sim.mapTheme === "harbor" ? [0, 90] : [-81, -12];
  const level = water.position.y;
  const gl = renderer.getContext();
  const pixel = new THREE.Vector3(x, level - 3, z);
  const size = renderer.getDrawingBufferSize(new THREE.Vector2());
  try {
    probe.position.set(x, level + 3, z);
    view.scene.add(probe);
    camera.position.set(x, level + 14, z + 18);
    camera.lookAt(x, level, z);
    camera.updateMatrixWorld();
    pixel.project(camera);
    water.material.uniforms.distortionScale.value = 0;
    const sample = (color: number) => {
      probe.material.color.setHex(color);
      renderer.render(view.scene, camera);
      const rgba = new Uint8Array(4);
      gl.readPixels(
        Math.floor((pixel.x * 0.5 + 0.5) * size.x),
        Math.floor((pixel.y * 0.5 + 0.5) * size.y),
        1,
        1,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        rgba,
      );
      return rgba;
    };
    const red = sample(0xff0000);
    const green = sample(0x00ff00);
    if (red[0] - green[0] < 15 || green[1] - red[1] < 15) {
      throw new Error(`${name}: reflected probe did not change color (${red} / ${green})`);
    }
  } finally {
    view.scene.remove(probe);
    probe.geometry.dispose();
    probe.material.dispose();
    camera.position.copy(position);
    camera.quaternion.copy(rotation);
    water.material.uniforms.distortionScale.value = distortion;
  }
  view.render(sim, 1, 0, true);
  const reflected = renderer.info.render.calls;
  let cached: number;
  try {
    water.onBeforeRender = () => {};
    view.render(sim, 1, 0, true);
    cached = renderer.info.render.calls;
    if (reflected <= cached) throw new Error(`${name}: no reflection render pass`);
  } finally {
    water.onBeforeRender = reflect;
  }
  // Moving into the dry center should stop the extra pass without changing the water mesh.
  const zoom = view.zoom;
  sim.human.previous = { x: 0, z: 0 };
  sim.human.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  view.zoom = 23;
  try {
    view.render(sim, 1, 0);
    const dry = renderer.info.render.calls;
    water.onBeforeRender = () => {};
    view.render(sim, 1, 0);
    if (dry !== renderer.info.render.calls) throw new Error(`${name}: dry view still reflects`);
  } finally {
    water.onBeforeRender = reflect;
    view.zoom = zoom;
  }
  return `${sim.mapName}: reflection pixels + dry-view skip PASS · ${cached} → ${reflected} draw calls`;
}
