import * as THREE from "three";
import { WaterSurface } from "./water-surface";

/** Reused across rounds; one reflection target covers all harbor berths. */
export class HarborWater {
  readonly mesh = new WaterSurface(new THREE.PlaneGeometry(340, 340), "harbor", -2.2);

  update(time: number): void {
    this.mesh.update(time);
  }
}
