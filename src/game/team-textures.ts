import * as THREE from "three";
const teamTextures = new Map<number, THREE.Texture>();
export function teamTexture(team: number) {
  let texture = teamTextures.get(team);
  if (!texture) {
    texture = new THREE.TextureLoader().load(
      `${import.meta.env.BASE_URL}textures/teams/${team === 0 ? "blue" : "red"}.png`,
    );
    teamTextures.set(team, texture);
  }
  return texture;
}
