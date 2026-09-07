import * as THREE from "three";

/** Shared static albedo maps; mipmaps keep distant ground stable and cheap. */
export function groundMaterial(renderer: THREE.WebGLRenderer, kind: "dry-grass" | "packed-dirt") {
  const material = new THREE.MeshStandardMaterial({
    // Set the final tint before batching, which bakes it into road vertices.
    color: kind === "dry-grass" ? 0xe2e8d5 : 0xe5dbcc,
    roughness: 1,
  });
  const texture = new THREE.TextureLoader().load(
    `${import.meta.env.BASE_URL}textures/ground/${kind}.webp`,
    () => {
      material.map = texture;
      material.needsUpdate = true;
    },
  );
  texture.colorSpace = THREE.SRGBColorSpace;
  // Mirroring joins the generated edges without relying on perfect AI tiling.
  texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  texture.anisotropy = Math.min(4, renderer.capabilities.getMaxAnisotropy());
  // Mark it textured before static batching, even while the image is loading.
  material.map = texture;
  return material;
}

/** One tile per eight world meters, aligned across roads and intersections. */
export function groundUVs(geometry: THREE.BufferGeometry, x = 0, z = 0) {
  const positions = geometry.getAttribute("position"), uv = geometry.getAttribute("uv");
  for (let i = 0; i < positions.count; i++)
    uv.setXY(i, (positions.getX(i) + x) / 8, (positions.getZ(i) + z) / 8);
  uv.needsUpdate = true;
}

/** Narrow alpha shoulders soften road borders; the opaque center stays flat. */
export function roadGeometry(w: number, d: number, x: number, z: number) {
  const shoulder = 0.7;
  const xs = [-w / 2, -w / 2 + shoulder, w / 2 - shoulder, w / 2];
  const zs = [-d / 2, -d / 2 + shoulder, d / 2 - shoulder, d / 2];
  const positions: number[] = [], colors: number[] = [], uvs: number[] = [], indices: number[] = [];
  for (let row = 0; row < 4; row++) for (let col = 0; col < 4; col++) {
    positions.push(xs[col], 0, zs[row]);
    uvs.push((xs[col] + x) / 8, (zs[row] + z) / 8);
    colors.push(1, 1, 1, row === 0 || row === 3 || col === 0 || col === 3 ? 0 : 1);
  }
  for (let row = 0; row < 3; row++) for (let col = 0; col < 3; col++) {
    const i = row * 4 + col;
    indices.push(i, i + 4, i + 1, i + 1, i + 4, i + 5);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 4));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}
