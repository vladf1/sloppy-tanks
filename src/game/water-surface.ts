import * as THREE from "three";
import { Water } from "three/addons/objects/Water.js";

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

/** Three.js planar reflections, with a shared ripple tile and map-specific banks.
 * Geometry must lie in local XY: Water derives its mirror plane from the mesh rotation. */
export class WaterSurface extends Water {
  private sightline = new THREE.Ray();
  private intersection = new THREE.Vector3();
  private surfacePlane: THREE.Plane;

  constructor(
    geometry: THREE.BufferGeometry,
    private kind: "harbor" | "creek",
    height: number,
  ) {
    const harbor = kind === "harbor";
    super(geometry, {
      textureWidth: 512,
      textureHeight: 512,
      waterNormals: waterNormals(),
      waterColor: harbor ? 0x164956 : 0x244b3f,
      sunColor: harbor ? 0xffdcc0 : 0xffebce,
      sunDirection: new THREE.Vector3(-45, harbor ? 55 : 68, 25).normalize(),
      distortionScale: harbor ? 1.8 : 0.65,
      fog: true,
    });
    this.name = harbor ? "harbor-water" : "village-creek";
    this.rotation.x = -Math.PI / 2;
    this.position.y = height;
    this.surfacePlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), -height);
    const reflect = this.onBeforeRender.bind(this);
    this.onBeforeRender = (...args) => {
      // Both maps have dry terrain throughout this square. Avoid a second scene render
      // while the entire view is over the combat arena and the water is hidden beneath it.
      if (this.waterInView(args[2])) {
        reflect(...args);
      }
    };
    this.material.uniforms.size.value = harbor ? 4 : 7;
    this.material.uniforms.shoreColor = {
      value: new THREE.Color(harbor ? 0x3a807d : 0x638466),
    };
    this.material.defines.HARBOR_WATER = harbor ? 1 : 0;
    // Keep the add-on's mirror camera, clipping, lighting and fog; only tune its surface.
    this.material.vertexShader = `varying vec2 waterUv;\n${this.material.vertexShader}`.replace(
      "void main() {",
      "void main() { waterUv = uv;",
    );
    this.material.fragmentShader = `
      varying vec2 waterUv;
      uniform vec3 shoreColor;
      ${this.material.fragmentShader}`
      .replace(
        "vec3( 1.5, 1.0, 1.5 )",
        harbor ? "vec3( 1.3, 1.0, 1.3 )" : "vec3( 0.75, 1.0, 0.75 )",
      )
      // Preserve readable reflections from the game's steep overhead camera.
      .replace("float rf0 = 0.02;", "float rf0 = 0.18;")
      .replace(
        "vec3 scatter =",
        `
        #if HARBOR_WATER
          float shore = max(abs(worldPosition.x), abs(worldPosition.z)) - 62.0;
        #else
          float shore = (1.0 - abs(waterUv.x * 2.0 - 1.0)) * 6.5;
        #endif
        float shallow = exp(-max(shore, 0.0) * 0.55);
        vec3 bankColor = mix(waterColor, shoreColor, shallow * 0.65);
        vec3 scatter =`,
      )
      .replace("* waterColor;", "* bankColor;")
      .replace(
        "( sunColor * diffuseLight * 0.3 + scatter )",
        "( scatter * (vec3(0.85) + diffuseLight * 0.25) )",
      )
      .replace(
        "vec3 outgoingLight = albedo;",
        `
        float wash = shore + sin(time * 1.8 + worldPosition.x * 0.35 + worldPosition.z * 0.3) * 0.18;
        float breakup = texture2D(normalSampler, worldPosition.xz * 0.11 + time * 0.025).r;
        float foam = (1.0 - smoothstep(0.12, 0.85, wash)) * smoothstep(0.46, 0.64, breakup);
        vec3 outgoingLight = mix(albedo, vec3(0.54, 0.67, 0.61), foam * 0.5);`,
      );
  }

  update(time: number): void {
    this.material.uniforms.time.value = time * (this.kind === "harbor" ? 0.7 : 0.65);
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
