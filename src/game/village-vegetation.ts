import * as THREE from "three";
import { Random } from "./math";
import { creekDistance, valleyHeight } from "./village-landscape";

/** Low meadow detail stays below shell height; shared instances sway entirely on the GPU. */
export class VillageVegetation {
  readonly group = new THREE.Group();
  private wind = { value: 0 };
  constructor() {
    this.group.name = "village-meadow";
    const vertices: number[] = [];
    for (let i = 0; i < 4; i++) {
      const a = i * Math.PI * 0.5;
      const x = Math.cos(a);
      const z = Math.sin(a);
      vertices.push(
        -z * 0.11,
        0,
        x * 0.11,
        z * 0.11,
        0,
        -x * 0.11,
        x * 0.25,
        0.7 + (i % 2) * 0.3,
        z * 0.25,
      );
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
    geometry.computeVertexNormals();
    const grass = new THREE.MeshStandardMaterial({
      color: 0xffffff,
      roughness: 1,
      side: THREE.DoubleSide,
    });
    grass.onBeforeCompile = (shader) => {
      shader.uniforms.villageWind = this.wind;
      shader.vertexShader = "uniform float villageWind;\n" + shader.vertexShader;
      shader.vertexShader = shader.vertexShader.replace(
        "#include <begin_vertex>",
        `#include <begin_vertex>
        #ifdef USE_INSTANCING
          transformed.x += sin(villageWind*1.2+instanceMatrix[3].x*.7+instanceMatrix[3].z*.4)*position.y*.2;
          transformed.z += cos(villageWind*.8+instanceMatrix[3].z*.5)*position.y*.12;
        #endif`,
      );
    };
    const tufts = new THREE.InstancedMesh(geometry, grass, 3600);
    const flowers = new THREE.InstancedMesh(
      new THREE.OctahedronGeometry(1),
      new THREE.MeshStandardMaterial({ roughness: 0.9 }),
      1500,
    );
    const rng = new Random(91387);
    const dummy = new THREE.Object3D();
    let count = 0;
    let blooms = 0;
    for (let i = 0; i < 4200 && count < 3600; i++) {
      const x = i < 1600 ? rng.range(-45, 45) : rng.range(-120, 120);
      const z = i < 1600 ? rng.range(-56, 56) : rng.range(-125, 110);
      const inside = Math.abs(x) < 59 && Math.abs(z) < 59;
      if (
        inside &&
        (Math.abs(x) < 10.5 ||
          Math.abs(x) > 46 ||
          Math.abs(z) < 7.5 ||
          Math.abs(Math.abs(z) - 38) < 5.5)
      ) {
        continue;
      }
      const river = creekDistance(x, z);
      if (!inside && (river < 7 || (Math.abs(x) < 65 && Math.abs(z) < 65))) {
        continue;
      }
      if (Math.sin(x * 0.24) * Math.cos(z * 0.18) + rng.next() < 0.1) {
        continue;
      }
      const height = inside ? 0 : valleyHeight(x, z);
      const reed = !inside && river < 11;
      const size = reed ? rng.range(0.6, 1.05) : rng.range(0.24, 0.48);
      dummy.position.set(x, height + 0.025, z);
      dummy.rotation.set(0, rng.next() * 6.28, 0);
      dummy.scale.set(size * 0.8, size, size * 0.8);
      dummy.updateMatrix();
      tufts.setMatrixAt(count, dummy.matrix);
      tufts.setColorAt(count++, new THREE.Color([0x598245, 0x7b9d51, 0x9eaf6b, 0x477650][i % 4]));
      if (!reed && i % 3 === 0 && blooms < 1500) {
        dummy.position.y += size * 0.85;
        dummy.scale.setScalar(rng.range(0.07, 0.12));
        dummy.updateMatrix();
        flowers.setMatrixAt(blooms, dummy.matrix);
        flowers.setColorAt(
          blooms++,
          new THREE.Color([0xe8ddad, 0xbda6ca, 0xe5bb68, 0xf2e6d0][i % 4]),
        );
      }
    }
    tufts.count = count;
    flowers.count = blooms;
    tufts.receiveShadow = flowers.receiveShadow = true;
    tufts.computeBoundingSphere();
    flowers.computeBoundingSphere();
    this.group.add(tufts, flowers);
  }
  update(time: number) {
    this.wind.value = time;
  }
}
