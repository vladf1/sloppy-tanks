import * as THREE from "three";
import type { Cover } from "./types";

/** A bounded set of soft chimney wisps; no image assets or CPU particle simulation. */
export class VillageAtmosphere {
  private geometry = new THREE.BufferGeometry();
  private material = new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    uniforms: { time: { value: 0 } },
    vertexShader: `uniform float time; attribute float phase; varying float fade;
      void main() {
        float t=fract(time*.065+phase);
        vec3 p=position+vec3(t*1.5+sin(time*.55+phase*20.)*t*.25,t*4.2,t*.45);
        vec4 view=modelViewMatrix*vec4(p,1.);
        gl_Position=projectionMatrix*view;
        gl_PointSize=clamp((.22+t*1.6)*720./-view.z,1.,65.);
        fade=smoothstep(0.,.15,t)*pow(1.-t,1.5)*.2;
      }`,
    fragmentShader: `varying float fade; void main() {
      float r=length(gl_PointCoord-.5)*2.;
      float alpha=(1.-smoothstep(.2,1.,r))*fade;
      gl_FragColor=vec4(.72,.75,.70,alpha);
      #include <tonemapping_fragment>
      #include <colorspace_fragment>
    }`,
  });
  readonly mesh = new THREE.Points(this.geometry, this.material);
  private positions = new Float32Array(192 * 3);
  constructor() {
    this.mesh.name = "village-chimney-smoke";
    this.mesh.frustumCulled = false;
    this.geometry.setAttribute("position", new THREE.BufferAttribute(this.positions, 3));
    this.geometry.setAttribute(
      "phase",
      new THREE.Float32BufferAttribute(
        Array.from({ length: 192 }, (_, i) => (i % 8) / 8 + Math.floor(i / 8) * 0.013),
        1,
      ),
    );
  }
  setCovers(covers: readonly Cover[]) {
    const sources = [
      { x: -32, y: 11.5, z: -71 },
      ...covers
        .filter((c) => c.kind === "house" && !c.destructible)
        .map((c) => ({ x: c.x - c.w * 0.25, y: c.h + 0.3, z: c.z - c.d * 0.2 })),
    ];
    const count = Math.min(24, sources.length) * 8;
    sources.slice(0, 24).forEach((p, i) => {
      for (let j = 0; j < 8; j++) {
        this.positions.set([p.x, p.y, p.z], (i * 8 + j) * 3);
      }
    });
    this.geometry.getAttribute("position").needsUpdate = true;
    this.geometry.setDrawRange(0, count);
  }
  update(time: number) {
    this.material.uniforms.time.value = time;
  }
}
