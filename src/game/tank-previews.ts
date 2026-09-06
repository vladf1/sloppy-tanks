import * as THREE from "three";
import { tankModel } from "./models";
import { VEHICLES } from "./data";
import type { Team, VehicleKind } from "./types";

const previews = new Map<string, string>();

/** Render the same models used in play once, then reuse lightweight card images. */
export function tankPreview(kind: VehicleKind, team: Team): string {
  if (!previews.size) {
    const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
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
    camera.position.set(6, 5, 8);
    camera.lookAt(0, 0.65, 0.35);
    try {
      for (const color of [0, 1] as Team[]) {
        for (const vehicle of Object.keys(VEHICLES) as VehicleKind[]) {
          const tank = tankModel(vehicle, color);
          scene.add(tank);
          renderer.render(scene, camera);
          previews.set(`${color}/${vehicle}`, renderer.domElement.toDataURL("image/png"));
          scene.remove(tank);
        }
      }
    } finally {
      // Model geometry/materials belong to the shared game cache.
      renderer.dispose();
      renderer.forceContextLoss();
    }
  }
  return previews.get(`${team}/${kind}`)!;
}
