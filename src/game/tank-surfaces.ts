import * as THREE from "three";
import { isMesh } from "./render-resources";

let wear: THREE.Texture | undefined;
const materials = new Map<THREE.MeshStandardMaterial, THREE.MeshStandardMaterial>();

/** Load before building previews; geometry-only tools never need image I/O. */
export async function loadTankSurface() {
  try {
    wear = await new THREE.TextureLoader().loadAsync(
      `${import.meta.env.BASE_URL}textures/tanks/armor-wear.png`,
    );
    wear.colorSpace = THREE.SRGBColorSpace;
    wear.wrapS = wear.wrapT = THREE.RepeatWrapping;
    wear.anisotropy = 4;
  } catch (error) {
    console.warn("Tank wear image unavailable; using plain team paint.", error);
  }
}

export function applyTankSurface(root: THREE.Group, colors: number[]): void {
  const texture = wear;
  if (!texture) {
    return;
  }
  root.traverse((object) => {
    if (!isMesh(object) || !(object.material instanceof THREE.MeshStandardMaterial)) {
      return;
    }
    const source = object.material;
    if (!colors.includes(source.color.getHex())) {
      return;
    }
    let painted = materials.get(source);
    if (!painted) {
      painted = source.clone();
      painted.map = texture;
      painted.bumpMap = texture;
      painted.bumpScale = 0.025;
      materials.set(source, painted);
    }
    object.material = painted;
  });
}
