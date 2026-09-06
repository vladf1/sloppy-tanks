import * as THREE from "three";

// Shared procedural surface tiles: readable seams and restrained grain, no image downloads.
const textures = new Map<string, THREE.DataTexture>();
const materials = new Map<string, THREE.MeshStandardMaterial>();
const geometries = new Map<string, THREE.BufferGeometry>();
const TILE_METRES = 2.56;
function surfaceMaterial(kind: "siding" | "shingles", color: number) {
  const key = `${kind}/${color}`;
  const cached = materials.get(key); if (cached) return cached;
  let texture = textures.get(kind);
  if (!texture) {
    const size = 256, pixels = new Uint8Array(size * size * 4);
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
      const row = Math.floor(y / 32), offset = y % 32;
      const width = kind === "shingles" ? 64 : 128;
      const column = Math.floor((x + (row % 2) * width / 2) / width);
      const seam = (x + (row % 2) * width / 2) % width;
      const variation = ((column * 37 + row * 19) % 23) - 11;
      const grain = kind === "siding"
        ? Math.sin(x * 0.12 + Math.sin(y * 0.7) * 2) * 4
        : ((x * 13 + y * 23) % 9) - 4;
      let value = 225 + variation + grain;
      if (offset < 3) value = 125;
      else if (offset < 5) value = 250;
      else if (offset > 28) value -= 24;
      if (seam < 2) value -= kind === "shingles" ? 65 : 25;
      const i = (y * size + x) * 4;
      pixels[i] = pixels[i + 1] = pixels[i + 2] = Math.max(0, Math.min(255, value));
      pixels[i + 3] = 255;
    }
    texture = new THREE.DataTexture(pixels, size, size);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.magFilter = THREE.LinearFilter;
    texture.minFilter = THREE.LinearMipmapLinearFilter;
    texture.generateMipmaps = true; texture.anisotropy = 4; texture.needsUpdate = true;
    textures.set(kind, texture);
  }
  const mat = new THREE.MeshStandardMaterial({ color, map: texture,
    bumpMap: texture, bumpScale: kind === "shingles" ? 0.035 : 0.02,
    roughness: 0.88, side: THREE.DoubleSide });
  materials.set(key, mat); return mat;
}

export function sidingBox(w: number, h: number, d: number, color: number) {
  const key = `wall/${w}/${h}/${d}`;
  let geo = geometries.get(key);
  if (!geo) {
    geo = new THREE.BoxGeometry(w, h, d);
    const uv = geo.getAttribute("uv");
    for (let i = 0; i < uv.count; i++) {
      const face = Math.floor(i / 4);
      uv.setXY(i, uv.getX(i) * (face < 2 ? d : w) / TILE_METRES,
        uv.getY(i) * (face === 2 || face === 3 ? d : h) / TILE_METRES);
    }
    geometries.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, surfaceMaterial("siding", color));
  mesh.castShadow = mesh.receiveShadow = true; return mesh;
}

export function shingleRoof(w: number, h: number, d: number, color: number) {
  const key = `roof/${w}/${h}/${d}`;
  let geo = geometries.get(key);
  if (!geo) {
    const positions: number[] = [], uvs: number[] = [];
    const slope = Math.hypot(w / 2, h);
    for (const side of [-1, 1]) {
      const points = [[0, h + 0.018, -d / 2], [side * w / 2, 0.018, -d / 2],
        [side * w / 2, 0.018, d / 2], [0, h + 0.018, d / 2]];
      const coords = [[0, slope], [0, 0], [d, 0], [d, slope]];
      for (const i of [0, 1, 2, 0, 2, 3]) {
        positions.push(...points[i]); uvs.push(coords[i][0] / TILE_METRES, coords[i][1] / TILE_METRES);
      }
    }
    geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geo.computeVertexNormals(); geometries.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, surfaceMaterial("shingles", color));
  mesh.castShadow = mesh.receiveShadow = true; return mesh;
}
