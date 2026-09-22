import * as THREE from "three/webgpu";
import {
  attribute,
  uniform,
  modelViewMatrix,
  cameraProjectionMatrix,
  positionGeometry,
  viewportSize,
  vec3,
  vec4,
  uv,
  smoothstep,
  sin,
} from "three/tsl";
import type { Cover } from "./types";

/** Bounded GPU-animated chimney wisps; instanced quads also work on WebGPU. */
export class VillageAtmosphere {
  private clock = uniform(0);
  private positions = new Float32Array(192 * 3);
  private geometry = new THREE.InstancedBufferGeometry();
  readonly mesh: THREE.Mesh<THREE.InstancedBufferGeometry>;
  constructor() {
    const plane = new THREE.PlaneGeometry(1, 1);
    this.geometry.index = plane.index;
    this.geometry.attributes = plane.attributes;
    this.geometry.setAttribute(
      "smokeOrigin",
      new THREE.InstancedBufferAttribute(this.positions, 3),
    );
    this.geometry.setAttribute(
      "phase",
      new THREE.InstancedBufferAttribute(
        Float32Array.from({ length: 192 }, (_, i) => (i % 8) / 8 + Math.floor(i / 8) * 0.013),
        1,
      ),
    );
    const phase = attribute("phase", "float" as const);
    const t = this.clock.mul(0.065).add(phase).fract();
    const p = attribute("smokeOrigin", "vec3" as const).add(
      vec3(
        t.mul(1.5).add(
          sin(this.clock.mul(0.55).add(phase.mul(20)))
            .mul(t)
            .mul(0.25),
        ),
        t.mul(4.2),
        t.mul(0.45),
      ),
    );
    const view = modelViewMatrix.mul(vec4(p, 1));
    const clip = cameraProjectionMatrix.mul(view);
    const size = t.mul(1.6).add(0.22).mul(720).div(view.z.negate()).clamp(1, 65);
    const material = new THREE.MeshBasicNodeMaterial({
      transparent: true,
      depthWrite: false,
      fog: false,
    });
    material.vertexNode = vec4(
      clip.xy.add(positionGeometry.xy.mul(size).mul(2).div(viewportSize).mul(clip.w)),
      clip.zw,
    );
    material.colorNode = vec3(0.72, 0.75, 0.7);
    material.opacityNode = smoothstep(0.2, 1, uv().sub(0.5).length().mul(2))
      .oneMinus()
      .mul(smoothstep(0, 0.15, t))
      .mul(t.oneMinus().pow(1.5))
      .mul(0.2);
    this.mesh = new THREE.Mesh(this.geometry, material);
    this.mesh.name = "village-chimney-smoke";
    this.mesh.frustumCulled = false;
    this.geometry.instanceCount = 0;
  }
  setCovers(covers: readonly Cover[]) {
    const sources = [
      { x: -32, y: 11.5, z: -71 },
      ...covers
        .filter((c) => c.kind === "house" && !c.destructible)
        .map((c) => ({ x: c.x - c.w * 0.25, y: c.h + 0.3, z: c.z - c.d * 0.2 })),
    ];
    sources.slice(0, 24).forEach((p, i) => {
      for (let j = 0; j < 8; j++) {
        this.positions.set([p.x, p.y, p.z], (i * 8 + j) * 3);
      }
    });
    this.geometry.getAttribute("smokeOrigin").needsUpdate = true;
    this.geometry.instanceCount = Math.min(24, sources.length) * 8;
  }
  update(time: number) {
    this.clock.value = time;
  }
}
