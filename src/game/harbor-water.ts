import * as THREE from "three";

/** One opaque surface: intersecting swells, fine ripples, reflected sky/sun and quay foam.
 * No reflection render pass, per-frame mesh rebuild, or extra physics bodies. */
export class HarborWater {
  readonly mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>;
  constructor() {
    this.mesh = new THREE.Mesh(
      new THREE.PlaneGeometry(340, 340, 96, 96).rotateX(-Math.PI / 2),
      new THREE.ShaderMaterial({
        uniforms: { time: { value: 0 } },
        vertexShader: `
          uniform float time;
          varying vec3 waterPosition;
          void main() {
            vec3 p = position;
            p.y += sin(p.x * 0.19 + p.z * 0.11 + time * 0.7) * 0.12;
            p.y += sin(p.z * 0.28 - p.x * 0.09 - time * 0.55) * 0.08;
            waterPosition = (modelMatrix * vec4(p, 1.0)).xyz;
            gl_Position = projectionMatrix * viewMatrix * vec4(waterPosition, 1.0);
          }
        `,
        fragmentShader: `
          uniform float time;
          varying vec3 waterPosition;
          float hash(vec2 p) {
            return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
          }
          float noise(vec2 p) {
            vec2 i = floor(p), f = fract(p);
            vec2 u = f * f * (3.0 - 2.0 * f);
            return mix(mix(hash(i), hash(i + vec2(1,0)), u.x),
              mix(hash(i + vec2(0,1)), hash(i + vec2(1,1)), u.x), u.y);
          }
          float ripple(vec2 p) {
            p += vec2(time * 0.17, -time * 0.11);
            float broad = noise(p * 0.28);
            return broad * 0.65 + noise(p * 0.85 + broad * 1.5) * 0.25
              + noise(p * 2.1 - time * 0.13) * 0.1;
          }
          void main() {
            vec2 p = waterPosition.xz;
            float r = ripple(p);
            float dx = (ripple(p + vec2(0.13, 0)) - r) / 0.13;
            float dz = (ripple(p + vec2(0, 0.13)) - r) / 0.13;
            vec3 n = normalize(vec3(-dx * 0.85, 1.0, -dz * 0.85));
            vec3 eye = normalize(cameraPosition - waterPosition);
            float fresnel = 0.04 + 0.75 * pow(1.0 - max(dot(n, eye), 0.0), 4.0);
            // Pale sky reflects across ripples; dark water remains beneath the reflection.
            vec3 reflected = reflect(-eye, n);
            vec3 sky = mix(vec3(0.62, 0.39, 0.27), vec3(0.30, 0.52, 0.62),
              smoothstep(0.0, 0.85, reflected.y));
            float shore = max(abs(p.x), abs(p.y)) - 62.0;
            float shallow = exp(-max(shore, 0.0) * 0.11);
            vec3 sea = mix(vec3(0.022, 0.10, 0.145), vec3(0.035, 0.24, 0.245), shallow);
            sea *= 0.85 + r * 0.35;
            vec3 color = mix(sea, sky, fresnel);
            vec3 sun = normalize(vec3(-0.55, 0.65, 0.38));
            vec3 halfLight = normalize(sun + eye);
            float highlight = pow(max(dot(n, halfLight), 0.0), 100.0);
            float sheen = pow(max(dot(n, halfLight), 0.0), 12.0);
            color += vec3(1.0, 0.72, 0.39) * (highlight * 1.8 + sheen * 0.075);
            // Broken wash hugs the vertical quay, with a second thin receding ripple.
            float wash = shore + sin(time * 0.9 + p.x * 0.12 + p.y * 0.1) * 0.23;
            float foam = (1.0 - smoothstep(0.1, 0.9, wash)) * smoothstep(0.30, 0.65, r);
            foam += (1.0 - smoothstep(0.05, 0.17, abs(wash - 1.5)))
              * smoothstep(0.5, 0.78, r) * 0.38;
            color = mix(color, vec3(0.49, 0.72, 0.67), clamp(foam, 0.0, 0.65));
            float fog = smoothstep(150.0, 260.0, distance(cameraPosition, waterPosition));
            color = mix(color, vec3(0.48, 0.37, 0.35), fog);
            gl_FragColor = vec4(color, 1.0);
            #include <tonemapping_fragment>
            #include <colorspace_fragment>
          }
        `,
      }),
    );
    this.mesh.name = "harbor-water";
    this.mesh.position.y = -2.2;
  }
  update(time: number): void {
    this.mesh.material.uniforms.time.value = time;
  }
}
