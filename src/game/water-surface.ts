import * as THREE from "three/webgpu";
import {
  uniform,
  texture,
  positionWorld,
  cameraPosition,
  vec2,
  vec3,
  float,
  max,
  mix,
  reflect,
  reflector,
  uv,
  smoothstep,
  sin,
} from "three/tsl";
import { HUD_LAYER } from "./view-settings";

let normals: THREE.Texture | undefined;
function waterNormals(): THREE.Texture {
  if (!normals) {
    normals = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/water/normals.webp`,
    );
    normals.wrapS = normals.wrapT = THREE.RepeatWrapping;
    normals.anisotropy = 4;
  }
  return normals;
}

/** Planar reflections and ripples use one TSL material on both supported backends. */
export class WaterSurface extends THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicNodeMaterial> {
  private clock = uniform(0);
  readonly distortionScale;
  reflectionEnabled = true;
  private sightline = new THREE.Ray();
  private intersection = new THREE.Vector3();
  private surfacePlane: THREE.Plane;
  constructor(
    geometry: THREE.BufferGeometry,
    private kind: "harbor" | "creek",
    height: number,
  ) {
    const material = new THREE.MeshBasicNodeMaterial();
    super(geometry, material);
    const harbor = kind === "harbor";
    this.name = harbor ? "harbor-water" : "village-creek";
    this.rotation.x = -Math.PI / 2;
    this.position.y = height;
    this.surfacePlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), -height);
    this.distortionScale = uniform(harbor ? 1.8 : 0.65);
    const normalMap = texture(waterNormals());
    const t = this.clock;
    const p = positionWorld.xz.mul(harbor ? 4 : 7);
    const noise = normalMap
      .sample(p.div(103).add(vec2(t.div(17), t.div(29))))
      .add(normalMap.sample(p.div(107).sub(vec2(t.div(-19), t.div(31)))))
      .add(normalMap.sample(p.div(vec2(8907, 9803)).add(vec2(t.div(101), t.div(97)))))
      .add(normalMap.sample(p.div(vec2(1091, 1027)).sub(vec2(t.div(109), t.div(-113)))))
      .mul(0.5)
      .sub(1);
    const normal = noise.xzy.mul(vec3(harbor ? 1.3 : 0.75, 1, harbor ? 1.3 : 0.75)).normalize();
    const worldToEye = cameraPosition.sub(positionWorld);
    const eye = worldToEye.normalize();
    const sunDirection = uniform(new THREE.Vector3(-45, harbor ? 55 : 68, 25).normalize());
    const sunColor = uniform(new THREE.Color(harbor ? 0xffdcc0 : 0xffebce));
    const specular = max(0, eye.dot(reflect(sunDirection.negate(), normal).normalize()))
      .pow(100)
      .mul(sunColor)
      .mul(2);
    const diffuse = max(sunDirection.dot(normal), 0).mul(sunColor).mul(0.5);
    const distortion = normal.xz
      .mul(float(0.001).add(float(1).div(worldToEye.length())))
      .mul(this.distortionScale);
    const mirror = reflector({ samples: 4 });
    // Match the original Water renderer's fixed 512 × 512 reflection target.
    // r185 exposes only viewport-relative sizing; isolate its sizing hook here.
    // The target also matches the main view's 4× MSAA linear frame buffer, so
    // reflected meshes reuse its pipelines instead of compiling a second set.
    Object.assign(mirror.reflector, {
      _updateResolution: (target: THREE.RenderTarget) => {
        target.texture.colorSpace = THREE.LinearSRGBColorSpace;
        target.setSize(512, 512);
      },
    });
    mirror.uvNode = mirror.uvNode!.add(distortion);
    this.add(mirror.target);
    const renderReflection = mirror.reflector.updateBefore.bind(mirror.reflector);
    mirror.reflector.updateBefore = (frame) => {
      if (
        frame.renderer &&
        frame.camera &&
        this.reflectionEnabled &&
        this.waterInView(frame.camera)
      ) {
        // The mirror camera clones the main camera's layers; keep the HUD out.
        mirror.reflector.getVirtualCamera(frame.camera).layers.disable(HUD_LAYER);
        return renderReflection(frame);
      }
      return false;
    };
    const theta = max(eye.dot(normal), 0);
    const reflectance = theta.oneMinus().pow(5).mul(0.82).add(0.18);
    const shore = harbor
      ? max(positionWorld.x.abs(), positionWorld.z.abs()).sub(62)
      : uv().x.mul(2).sub(1).abs().oneMinus().mul(6.5);
    const shallow = max(shore, 0).mul(-0.55).exp();
    const bank = mix(
      uniform(new THREE.Color(harbor ? 0x164956 : 0x244b3f)),
      uniform(new THREE.Color(harbor ? 0x3a807d : 0x638466)),
      shallow.mul(0.65),
    );
    const scatter = max(0, normal.dot(eye)).mul(bank);
    const albedo = mix(
      scatter.mul(vec3(0.85).add(diffuse.mul(0.25))),
      mirror.rgb.add(specular),
      reflectance,
    );
    const wash = shore.add(
      sin(t.mul(1.8).add(positionWorld.x.mul(0.35)).add(positionWorld.z.mul(0.3))).mul(0.18),
    );
    const breakup = normalMap.sample(positionWorld.xz.mul(0.11).add(t.mul(0.025))).r;
    const foam = smoothstep(0.12, 0.85, wash)
      .oneMinus()
      .mul(smoothstep(0.46, 0.64, breakup));
    material.colorNode = mix(albedo, vec3(0.54, 0.67, 0.61), foam.mul(0.5));
  }
  update(time: number): void {
    this.clock.value = time * (this.kind === "harbor" ? 0.7 : 0.65);
  }

  private waterInView(camera: THREE.Camera): boolean {
    this.sightline.origin.setFromMatrixPosition(camera.matrixWorld);
    for (let i = 0; i < 4; i++) {
      this.sightline.direction
        .set(i % 2 ? 1 : -1, i < 2 ? -1 : 1, 0.5)
        .unproject(camera)
        .sub(this.sightline.origin)
        .normalize();
      if (
        !this.sightline.intersectPlane(this.surfacePlane, this.intersection) ||
        Math.max(Math.abs(this.intersection.x), Math.abs(this.intersection.z)) > 62
      ) {
        return true;
      }
    }
    return false;
  }
}
