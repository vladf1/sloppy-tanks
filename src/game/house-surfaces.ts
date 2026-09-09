import * as THREE from "three";

// Shared offline-generated tiles; drawing code lives in scripts/generate-textures.ts.
const textures = new Map<string, THREE.Texture>();
const materials = new Map<string, THREE.MeshStandardMaterial>();
const geometries = new Map<string, THREE.BufferGeometry>();
const TILE_METRES = 2.56;
function surfaceMaterial(kind: "siding" | "shingles", color: number) {
  const key = `${kind}/${color}`;
  const cached = materials.get(key);
  if (cached) {
    return cached;
  }
  let texture = textures.get(kind);
  if (!texture) {
    texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/houses/${kind}.png`,
    );
    // Preserve the row orientation of the original DataTexture.
    texture.flipY = false;
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.magFilter = THREE.LinearFilter;
    texture.minFilter = THREE.LinearMipmapLinearFilter;
    texture.generateMipmaps = true;
    texture.anisotropy = 4;
    texture.needsUpdate = true;
    textures.set(kind, texture);
  }
  const mat = new THREE.MeshStandardMaterial({
    color,
    map: texture,
    bumpMap: texture,
    bumpScale: kind === "shingles" ? 0.035 : 0.02,
    roughness: 0.88,
    side: THREE.DoubleSide,
  });
  materials.set(key, mat);
  return mat;
}

export function sidingBox(w: number, h: number, d: number, color: number) {
  const key = `wall/${w}/${h}/${d}`;
  let geo = geometries.get(key);
  if (!geo) {
    geo = new THREE.BoxGeometry(w, h, d);
    const uv = geo.getAttribute("uv");
    for (let i = 0; i < uv.count; i++) {
      const face = Math.floor(i / 4);
      uv.setXY(
        i,
        (uv.getX(i) * (face < 2 ? d : w)) / TILE_METRES,
        (uv.getY(i) * (face === 2 || face === 3 ? d : h)) / TILE_METRES,
      );
    }
    geometries.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, surfaceMaterial("siding", color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

/** Triangular roof body with horizontal boards at the same scale as the walls. */
export function sidingGable(w: number, h: number, d: number, color: number) {
  const key = `gable/${w}/${h}/${d}`;
  let geo = geometries.get(key);
  if (!geo) {
    const profile = new THREE.Shape();
    profile.moveTo(-w / 2, 0);
    profile.lineTo(0, h);
    profile.lineTo(w / 2, 0);
    profile.closePath();
    geo = new THREE.ExtrudeGeometry(profile, { depth: d, bevelEnabled: false }).translate(
      0,
      0,
      -d / 2,
    );
    const positions = geo.getAttribute("position");
    const uv = geo.getAttribute("uv");
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(i, (positions.getX(i) + w / 2) / TILE_METRES, positions.getY(i) / TILE_METRES);
    }
    geometries.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, surfaceMaterial("siding", color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

export function shingleRoof(w: number, h: number, d: number, color: number) {
  const key = `roof/${w}/${h}/${d}`;
  let geo = geometries.get(key);
  if (!geo) {
    const positions: number[] = [];
    const uvs: number[] = [];
    const slope = Math.hypot(w / 2, h);
    for (const side of [-1, 1]) {
      const points = [
        [0, h + 0.018, -d / 2],
        [(side * w) / 2, 0.018, -d / 2],
        [(side * w) / 2, 0.018, d / 2],
        [0, h + 0.018, d / 2],
      ];
      const coords = [
        [0, slope],
        [0, 0],
        [d, 0],
        [d, slope],
      ];
      for (const i of [0, 1, 2, 0, 2, 3]) {
        positions.push(...points[i]);
        uvs.push(coords[i][0] / TILE_METRES, coords[i][1] / TILE_METRES);
      }
    }
    geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geo.computeVertexNormals();
    geometries.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, surfaceMaterial("shingles", color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
