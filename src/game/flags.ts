import * as THREE from "three";
import { spawnPositions } from "./arena";
import { TEAM_COLORS } from "./data";

function randomWind() {
  return { strength: 0.15 + Math.random() * 0.85, direction: (Math.random() - 0.5) * 1.3 };
}

/** Cloth stays attached to the pole while gusts carry ripples toward its free edge. */
export class Flags {
  group = new THREE.Group();
  private cloth: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshStandardMaterial>[] = [];
  private windFrom = randomWind();
  private windTo = randomWind();
  private windStart = 0;
  private windDuration = 3 + Math.random() * 4;

  constructor() {
    for (const team of [0, 1] as const) {
      const material = new THREE.MeshStandardMaterial({
        color: TEAM_COLORS[team],
        roughness: 1,
        side: THREE.DoubleSide,
      });
      for (const position of spawnPositions(team)) {
        const geometry = new THREE.PlaneGeometry(1.4, 0.9, 16, 6);
        (geometry.getAttribute("position") as THREE.BufferAttribute).setUsage(
          THREE.DynamicDrawUsage,
        );
        const mesh = new THREE.Mesh(geometry, material);
        mesh.position.set(team === 0 ? -62 : 62, 4.6, position.z);
        mesh.castShadow = mesh.receiveShadow = true;
        this.cloth.push(mesh);
        this.group.add(mesh);
      }
    }
    this.update(0);
  }

  update(time: number): void {
    // One breeze for the entire arena, easing toward a new random target every 3–7 seconds.
    while (time >= this.windStart + this.windDuration) {
      this.windStart += this.windDuration;
      this.windFrom = this.windTo;
      this.windTo = randomWind();
      this.windDuration = 3 + Math.random() * 4;
    }
    const progress = Math.max(0, (time - this.windStart) / this.windDuration);
    const blend = progress * progress * (3 - 2 * progress);
    const gust = THREE.MathUtils.lerp(this.windFrom.strength, this.windTo.strength, blend);
    const direction = THREE.MathUtils.lerp(this.windFrom.direction, this.windTo.direction, blend);
    const alongX = Math.sin(direction);
    const alongZ = Math.cos(direction);
    for (const mesh of this.cloth) {
      const geometry = mesh.geometry;
      const positions = geometry.getAttribute("position");
      const uv = geometry.getAttribute("uv");
      const phase = mesh.position.z * 0.12 + mesh.position.x * 0.04;
      for (let i = 0; i < positions.count; i++) {
        const u = uv.getX(i);
        const v = 1 - uv.getY(i);
        const ripple = Math.sin(u * 9 - time * 5 + phase + v * 1.8);
        const flutter = Math.sin(u * 19 - time * 8 + phase) * 0.035 * u * u;
        const reach = u * (1.05 + gust * 0.3);
        const sideways = u * (0.1 + gust * 0.09) * ripple + flutter;
        positions.setXYZ(
          i,
          reach * alongX + sideways * alongZ,
          -v * 0.9 - u * u * (0.45 - gust * 0.22) + u * 0.045 * ripple,
          reach * alongZ - sideways * alongX,
        );
      }
      positions.needsUpdate = true;
      geometry.computeVertexNormals();
      geometry.computeBoundingSphere();
    }
  }
}
