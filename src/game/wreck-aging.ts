import * as THREE from "three";

const original = new WeakMap<
  THREE.MeshStandardMaterial,
  { color: THREE.Color; emissive: THREE.Color }
>();

/** Per-wreck material clones preserve shared live paint and never compound the tint. */
export function ageWreckMaterial(material: THREE.Material, secondsSinceDeath: number): void {
  if (!(material instanceof THREE.MeshStandardMaterial)) {
    return;
  }
  let base = original.get(material);
  if (!base) {
    base = { color: material.color.clone(), emissive: material.emissive.clone() };
    original.set(material, base);
  }
  const brightness = 1 - 0.7 * THREE.MathUtils.clamp(secondsSinceDeath / 2.5, 0, 1);
  material.color.copy(base.color).multiplyScalar(brightness);
  material.emissive.copy(base.emissive).multiplyScalar(brightness);
}
