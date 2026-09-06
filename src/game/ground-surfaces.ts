import * as THREE from "three";

/** Shared static albedo maps; mipmaps keep distant ground stable and cheap. */
export function groundMaterial(renderer: THREE.WebGLRenderer, kind: "dry-grass" | "packed-dirt") {
  const material = new THREE.MeshStandardMaterial({
    color: kind === "dry-grass" ? 0xbbbf73 : 0xddbd80,
    roughness: 1,
  });
  const texture = new THREE.TextureLoader().load(
    `${import.meta.env.BASE_URL}textures/ground/${kind}.webp`,
    () => {
      material.map = texture;
      material.color.setHex(0xffffff);
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
