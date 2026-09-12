import * as THREE from "three";
import { batch, freezeStatic } from "./batching";
import { box, put } from "./model-primitives";
import { createTerrain } from "./scenery";
import { VillageLandscape, valleyHeight } from "./village-landscape";
import { villageLandmarks } from "./village-landmarks";
import { VillageVegetation } from "./village-vegetation";
import { VillageAtmosphere } from "./village-atmosphere";
import type { Cover } from "./types";

/** Pine Village scenery is retained across matches. */
export class VillageScenery extends THREE.Scene {
  private landscape: VillageLandscape;
  private vegetation = new VillageVegetation();
  private wheel: THREE.Group;
  private atmosphere = new VillageAtmosphere();
  constructor(renderer: THREE.WebGLRenderer) {
    super();
    this.name = "pine-village-scenery";
    const grass = createTerrain(this, renderer);
    this.landscape = new VillageLandscape(grass);
    this.add(this.landscape.group, this.vegetation.group, this.atmosphere.mesh);
    const landmarks = villageLandmarks();
    this.wheel = landmarks.wheel;
    this.add(landmarks.group);
    const details = new THREE.Group();
    // Stepping stones and a timber sign link the village road to its old footbridge.
    for (let i = 0; i < 6; i++) {
      const z = -63 - i * 1.35;
      const stone = box(1.1, 0.15, 0.85, 0xa4aa8c, 0.08);
      stone.rotation.y = Math.sin(i) * 0.3;
      put(details, stone, Math.sin(i * 0.6) * 0.4, valleyHeight(0, z) + 0.12, z);
    }
    for (const x of [-4.4, 4.4]) {
      put(details, box(0.26, 3.2, 0.26, 0x715534, 0), x, 0.75, -64.5);
    }
    put(details, box(9.2, 0.22, 0.34, 0x80613d, 0), 0, 2.35, -64.5);
    // Small hanging sign is a landmark, not an in-game overlay.
    const canvas = document.createElement("canvas");
    canvas.width = 512;
    canvas.height = 128;
    const context = canvas.getContext("2d")!;
    context.fillStyle = "#816b45";
    context.fillRect(0, 0, 512, 128);
    context.strokeStyle = "#c9b384";
    context.lineWidth = 5;
    context.strokeRect(8, 8, 496, 112);
    context.fillStyle = "#efe5c7";
    context.font = "bold 48px Georgia";
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText("PINE VILLAGE", 256, 68);
    const map = new THREE.CanvasTexture(canvas);
    map.colorSpace = THREE.SRGBColorSpace;
    const sign = new THREE.Mesh(
      new THREE.BoxGeometry(5.2, 1.3, 0.16),
      new THREE.MeshStandardMaterial({ map, roughness: 1 }),
    );
    put(details, sign, 0, 1.55, -64.4);
    batch(details);
    this.add(details);
    freezeStatic(details);
  }
  update(time: number) {
    this.wheel.rotation.z = -time * 0.16;
    this.landscape.update(time);
    this.vegetation.update(time);
    this.atmosphere.update(time);
  }
  setCovers(covers: readonly Cover[]) {
    this.atmosphere.setCovers(covers);
  }
}
