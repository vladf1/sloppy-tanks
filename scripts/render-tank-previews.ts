import * as THREE from "three/webgpu";
import { GameRenderer } from "../src/game/renderer";
import { VEHICLES } from "../src/game/data";
import { tankModel } from "../src/game/models";
import type { Team, VehicleKind } from "../src/game/types";

const previews = new Map<string, string>();

/** Render the same models used in play once, then reuse lightweight card images. */
export async function renderTankPreviews(): Promise<Record<string, string>> {
  if (!previews.size) {
    const renderer = new GameRenderer({ alpha: true, antialias: true });
    await renderer.init();
    renderer.setSize(640, 400);
    renderer.setClearColor(0x000000, 0);
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    const scene = new THREE.Scene();
    scene.add(new THREE.HemisphereLight(0xdcedff, 0x4c6075, 2.4));
    const key = new THREE.DirectionalLight(0xfff1d5, 3.5);
    key.position.set(-3, 7, 5);
    scene.add(key);
    const rim = new THREE.DirectionalLight(0x91cfff, 2);
    rim.position.set(4, 3, -4);
    scene.add(rim);
    const camera = new THREE.OrthographicCamera(-3.5, 3.5, 2.1875, -2.1875, 0.1, 40);
    camera.position.set(8, 6, 6);
    camera.lookAt(0, 0.6, 0.65);
    try {
      for (const color of [0, 1] as Team[]) {
        for (const vehicle of Object.keys(VEHICLES) as VehicleKind[]) {
          const tank = tankModel(vehicle, color);
          scene.add(tank);
          // Pipelines compile asynchronously and draws are skipped until they
          // are ready, so the first frame of each tank (and of the output pass)
          // is empty. Warm them, then capture a complete frame.
          renderer.render(scene, camera);
          await renderer.waitForPipelineCompilation();
          renderer.render(scene, camera);
          // The WebGPU canvas is cleared once presented: read it before yielding.
          previews.set(`${color}-${vehicle}`, renderer.domElement.toDataURL("image/webp", 0.88));
          scene.remove(tank);
        }
      }
    } finally {
      // Model geometry/materials belong to the shared game cache.
      renderer.dispose();
    }
  }
  return Object.fromEntries(previews);
}
