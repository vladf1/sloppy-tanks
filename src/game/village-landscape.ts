import * as THREE from "three";
import { batch, freezeStatic } from "./batching";
import { groundUVs } from "./ground-surfaces";
import { Random } from "./math";
import { put } from "./model-primitives";
import { treeModel } from "./tree-models";

const creek = new THREE.CatmullRomCurve3(
  [
    [150, -107],
    [85, -86],
    [15, -82],
    [-57, -85],
    [-81, -64],
    [-81, -12],
    [-86, 48],
    [-110, 145],
  ].map(([x, z]) => new THREE.Vector3(x, -2.65, z)),
);
const creekPoints = creek.getPoints(160);
export function creekDistance(x: number, z: number): number {
  let distance = Infinity;
  for (let i = 1; i < creekPoints.length; i++) {
    const a = creekPoints[i - 1];
    const b = creekPoints[i];
    const dx = b.x - a.x;
    const dz = b.z - a.z;
    const t = THREE.MathUtils.clamp(((x - a.x) * dx + (z - a.z) * dz) / (dx * dx + dz * dz), 0, 1);
    distance = Math.min(distance, Math.hypot(x - a.x - dx * t, z - a.z - dz * t));
  }
  return distance;
}
export function valleyHeight(x: number, z: number): number {
  const edge = Math.max(Math.abs(x), Math.abs(z));
  const hill =
    THREE.MathUtils.smoothstep(edge, 66, 145) *
    (4 + 3 * Math.sin(x * 0.047 + z * 0.028) + 2 * Math.sin(z * 0.069 - x * 0.021));
  const bank = THREE.MathUtils.smoothstep(creekDistance(x, z), 4.9, 9);
  return THREE.MathUtils.lerp(-4.2, -0.85 + hill, bank);
}

function mountainRise(x: number, z: number): number {
  if (z > -112) {
    return 0;
  }
  let highest = 0;
  let total = 0;
  for (const [cx, cz, height, width] of [
    [-146, -164, 38, 31],
    [-94, -174, 51, 36],
    [-32, -161, 48, 32],
    [28, -181, 61, 41],
    [83, -162, 40, 30],
    [145, -179, 54, 39],
  ]) {
    const dx = (x - cx) / width + Math.sin(z * 0.09 + cx) * 0.13;
    const dz = (z - cz) / (width * 0.8);
    const rise = height * Math.exp(-1.4 * (dx * dx + dz * dz));
    highest = Math.max(highest, rise);
    total += rise;
  }
  const crags = Math.sin(x * 0.29 + z * 0.14) * Math.cos(z * 0.21 - x * 0.13) * 2.4;
  return (
    Math.max(0, highest * 0.8 + total * 0.2 + crags * THREE.MathUtils.smoothstep(highest, 2, 12)) *
    THREE.MathUtils.smoothstep(-z, 112, 133)
  );
}

function streamGeometry() {
  const positions: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  for (let i = 0; i < creekPoints.length; i++) {
    const p = creekPoints[i];
    const tangent = creek.getTangent(i / (creekPoints.length - 1));
    const width = 6.5 + Math.sin(i * 0.12) * 0.55;
    for (const side of [-1, 1]) {
      positions.push(p.x - tangent.z * width * side, p.y, p.z + tangent.x * width * side);
      uvs.push((side + 1) / 2, i * 1.9);
    }
    if (i < creekPoints.length - 1) {
      const n = i * 2;
      indices.push(n, n + 1, n + 2, n + 1, n + 3, n + 2);
    }
  }
  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geo.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geo.setIndex(indices);
  geo.computeVertexNormals();
  return geo;
}

export class VillageLandscape {
  readonly group = new THREE.Group();
  private water = new THREE.ShaderMaterial({
    side: THREE.DoubleSide,
    uniforms: { time: { value: 0 } },
    vertexShader: `varying vec2 flow; varying vec3 world; void main() {
      flow = uv; world = (modelMatrix * vec4(position, 1.)).xyz;
      gl_Position = projectionMatrix * viewMatrix * vec4(world, 1.);
    }`,
    fragmentShader: `uniform float time; varying vec2 flow; varying vec3 world;
      float hash(vec2 p) { return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453); }
      float noise(vec2 p) {
        vec2 i=floor(p),f=fract(p),u=f*f*(3.-2.*f);
        return mix(mix(hash(i),hash(i+vec2(1,0)),u.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1,1)),u.x),u.y);
      }
      float ripple(vec2 p) { float n=noise(p*.6); return n*.7+noise(p*1.7+n)*.22+noise(p*4.)*.08; }
      void main() {
        vec2 p=vec2(flow.x*12.,flow.y-time*.85);
        float r=ripple(p);
        vec3 normal=normalize(vec3((r-ripple(p+vec2(.13,0)))*4.,1.,(r-ripple(p+vec2(0,.13)))*4.));
        vec3 eye=normalize(cameraPosition-world);
        float fresnel=.045+.5*pow(1.-max(dot(normal,eye),0.),4.);
        float bank = pow(abs(flow.x * 2. - 1.), 2.);
        vec3 color = mix(vec3(.022,.10,.105), vec3(.09,.23,.15), bank)*(.86+r*.28);
        color=mix(color,vec3(.35,.48,.40),fresnel);
        float glint=pow(max(dot(normal,normalize(eye+vec3(-.48,.76,.3))),0.),120.);
        color += vec3(.7,.76,.54)*glint*.4;
        float foam = smoothstep(.67,.97,bank) * smoothstep(.55,.78,r);
        color = mix(color,vec3(.42,.59,.43),foam*.45);
        color = mix(color,vec3(.40,.59,.54),smoothstep(210.,380.,distance(cameraPosition,world)));
        gl_FragColor=vec4(color,1.);
        #include <tonemapping_fragment>
        #include <colorspace_fragment>
      }`,
  });
  constructor(grass: THREE.MeshStandardMaterial) {
    this.group.name = "pine-valley-landscape";
    const geo = new THREE.PlaneGeometry(340, 340, 112, 112).rotateX(-Math.PI / 2);
    const p = geo.getAttribute("position");
    const colors: number[] = [];
    for (let i = 0; i < p.count; i++) {
      const x = p.getX(i);
      const z = p.getZ(i);
      const river = creekDistance(x, z);
      p.setY(i, valleyHeight(x, z));
      const patch =
        0.5 + 0.25 * Math.sin(x * 0.064 + z * 0.03) + 0.25 * Math.sin(z * 0.1 - x * 0.05);
      const color = new THREE.Color().setRGB(
        0.48 + patch * 0.35,
        0.64 + patch * 0.3,
        0.34 + patch * 0.26,
      );
      if (river < 8) {
        color.lerp(new THREE.Color(0x8c967f), (8 - river) / 8);
      }
      colors.push(color.r, color.g, color.b);
    }
    geo.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geo.computeVertexNormals();
    groundUVs(geo);
    const terrain = new THREE.Mesh(geo, grass);
    terrain.receiveShadow = true;
    this.group.add(terrain);
    const stream = new THREE.Mesh(streamGeometry(), this.water);
    stream.name = "village-creek";
    this.group.add(stream);
    this.backdrop();
  }
  private backdrop() {
    const rng = new Random(8274);
    const forest = new THREE.Group();
    const rocks = new THREE.Group();
    const stone = new THREE.IcosahedronGeometry(1, 0);
    const stoneMaterials = [0x879087, 0xa4aa92, 0x717f76].map(
      (color) => new THREE.MeshStandardMaterial({ color, roughness: 0.96, flatShading: true }),
    );
    for (let i = 0; i < 140; i++) {
      const t = rng.next();
      const p = creek.getPoint(t);
      const tangent = creek.getTangent(t);
      const side = i % 2 ? -1 : 1;
      const offset = rng.range(5.5, 9.5) * side;
      const x = p.x - tangent.z * offset;
      const z = p.z + tangent.x * offset;
      const rock = new THREE.Mesh(stone, stoneMaterials[i % 3]);
      const size = rng.range(0.3, 1.4);
      rock.scale.set(size, size * 0.4, size * rng.range(0.7, 1.2));
      rock.rotation.set(rng.next(), rng.next() * 6, 0);
      put(rocks, rock, x, valleyHeight(x, z) + size * 0.08, z);
    }
    for (let i = 0; i < 420; i++) {
      const x = rng.range(-148, 148);
      const z = rng.range(-145, 105);
      if (
        (Math.abs(x) < 69 && z > -99) ||
        (z > 57 && Math.abs(x) < 98) ||
        creekDistance(x, z) < 11
      ) {
        continue;
      }
      if (
        (Math.abs(x + 35) < 15 && Math.abs(z + 69) < 17) ||
        (Math.abs(x) < 12 && Math.abs(z + 82) < 18)
      ) {
        continue;
      }
      if (mountainRise(x, z) > 22) {
        continue;
      }
      const height = rng.range(6, 13);
      const span = rng.range(3.3, 5.6);
      const tree = treeModel({ x, z, w: span, d: span, h: height }, "background");
      tree.position.y = valleyHeight(x, z) + mountainRise(x, z);
      tree.updateMatrix();
      for (const child of [...tree.children]) {
        child.applyMatrix4(tree.matrix);
        forest.add(child);
      }
    }
    for (const [x, z] of [
      [-59, -66],
      [-52, -67],
      [-19, -67],
      [17, -68],
      [23, -68],
      [45, -67],
      [54, -66],
      [61, -67],
      [-67, -48],
      [-67, -6],
      [-68, 10],
      [-70, 54],
      [67, -42],
      [68, 4],
      [67, 42],
    ]) {
      const tree = treeModel({ x, z, w: 4.4, d: 4.4, h: 8 + rng.next() * 2 }, "background");
      tree.position.y = valleyHeight(x, z) + mountainRise(x, z);
      tree.updateMatrix();
      for (const child of [...tree.children]) {
        child.applyMatrix4(tree.matrix);
        forest.add(child);
      }
    }
    // A continuous craggy ridge blends into the foothills, with snow following the terrain.
    const ridge = new THREE.PlaneGeometry(360, 95, 90, 26)
      .rotateX(-Math.PI / 2)
      .translate(0, 0, -159.5);
    const points = ridge.getAttribute("position");
    const shades: number[] = [];
    for (let i = 0; i < points.count; i++) {
      const x = points.getX(i);
      const z = points.getZ(i);
      const y = valleyHeight(x, z) + mountainRise(x, z);
      points.setY(i, y);
      const snowline = 30 + Math.sin(x * 0.095 + z * 0.08) * 5;
      const color = new THREE.Color(0x82948a).lerp(
        new THREE.Color(0xe7eadb),
        THREE.MathUtils.smoothstep(y, snowline, snowline + 6),
      );
      if (y < 15) {
        color.lerp(new THREE.Color(0x768d63), (15 - y) / 15);
      }
      shades.push(color.r, color.g, color.b);
    }
    ridge.setAttribute("color", new THREE.Float32BufferAttribute(shades, 3));
    ridge.computeVertexNormals();
    const mountains = new THREE.Mesh(
      ridge,
      new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 1, flatShading: true }),
    );
    mountains.name = "pine-mountain-ridge";
    this.group.add(mountains);
    batch(rocks);
    batch(forest);
    freezeStatic(rocks);
    freezeStatic(forest);
    this.group.add(rocks, forest);
  }
  update(time: number) {
    this.water.uniforms.time.value = time;
  }
}
