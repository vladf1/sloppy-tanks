import * as THREE from "three";
import { Random } from "./math";

const noiseSeed = new Random(38012);
const soilNoise = Float32Array.from({ length: 256 * 256 }, () => noiseSeed.next());

/** Smooth deterministic value noise, shared by the macro soil and fine aggregate. */
function noise(x: number, z: number): number {
  const ix = Math.floor(x);
  const iz = Math.floor(z);
  const hash = (a: number, b: number) => soilNoise[(a & 255) + (b & 255) * 256];
  const fx = x - ix;
  const fz = z - iz;
  const u = fx * fx * (3 - 2 * fx);
  const v = fz * fz * (3 - 2 * fz);
  return THREE.MathUtils.lerp(
    THREE.MathUtils.lerp(hash(ix, iz), hash(ix + 1, iz), u),
    THREE.MathUtils.lerp(hash(ix, iz + 1), hash(ix + 1, iz + 1), u),
    v,
  );
}

/** A baked, metre-scaled work yard: compacted haul routes, wheel ruts and aggregate.
 * Generated once for the retained scenery, never during round reset or rendering. */
export function quarryTerrain(renderer: THREE.WebGLRenderer): THREE.Mesh {
  const size = 2048;
  const extent = 210;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const pixels = ctx.createImageData(size, size);
  const rng = new Random(7391);
  for (let row = 0; row < size; row++) {
    const z = (row / (size - 1) - 0.5) * extent;
    for (let col = 0; col < size; col++) {
      const x = (col / (size - 1) - 0.5) * extent;
      const macro = noise(x * 0.065, z * 0.065);
      const grit = noise(x * 0.75, z * 0.75);
      // Rounded outer haul loop and a gently wandering east/west crossing.
      const qx = Math.abs(x) - 39;
      const qz = Math.abs(z) - 39;
      const loop = Math.abs(
        Math.hypot(Math.max(qx, 0), Math.max(qz, 0)) + Math.min(Math.max(qx, qz), 0) - 12,
      );
      const crossing = Math.abs(z - Math.sin(x * 0.055) * 1.6);
      const distance = Math.min(loop, crossing);
      const edge = (noise(x * 0.42, z * 0.42) - 0.5) * 1.6;
      const road = 1 - THREE.MathUtils.smoothstep(distance + edge, 3.2, 7.8);
      const rut = Math.exp(-Math.pow((distance - 2.25) / 0.38, 2)) * road;
      const fine = rng.range(-5, 5);
      const aggregate = rng.next() > 0.975 ? rng.range(-22, 17) : 0;
      const shade = (macro - 0.5) * 27 + (grit - 0.5) * 13 + fine + aggregate - rut * 11;
      const i = (row * size + col) * 4;
      pixels.data[i] = 151 + road * 33 + shade;
      pixels.data[i + 1] = 130 + road * 34 + shade;
      pixels.data[i + 2] = 100 + road * 34 + shade;
      pixels.data[i + 3] = 255;
    }
  }
  ctx.putImageData(pixels, 0, 0);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
  const material = new THREE.MeshStandardMaterial({
    map: texture,
    bumpMap: texture,
    bumpScale: 0.065,
    roughness: 1,
  });
  const geometry = new THREE.PlaneGeometry(extent, extent, 140, 140).rotateX(-Math.PI / 2);
  const positions = geometry.getAttribute("position");
  for (let i = 0; i < positions.count; i++) {
    const outside = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i))) - 60;
    positions.setY(i, outside > 0 ? -Math.min(1.8, outside * 0.3) : 0);
  }
  geometry.computeVertexNormals();
  const floor = new THREE.Mesh(geometry, material);
  floor.name = "quarry-compacted-haul-roads";
  floor.position.y = 0.008;
  floor.receiveShadow = true;
  return floor;
}
