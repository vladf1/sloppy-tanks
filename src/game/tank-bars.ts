import * as THREE from "three";
import { TEAM_COLORS } from "./data";
import { isMesh } from "./render-resources";
import { teamTexture } from "./team-textures";
type HudMesh = THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicMaterial>;
export interface TankBar extends THREE.Group {
  userData: { fg: HudMesh; ammo: HudMesh; ranks: HudMesh[] };
}

/** Billboard layers share the transparent pass so terrain cannot draw over them. */
export function createTankBar(team: number, human: boolean): TankBar {
  const g = new THREE.Group() as TankBar;
  const border = new THREE.Mesh(
    new THREE.PlaneGeometry(1.87, 0.28),
    new THREE.MeshBasicMaterial({ color: 0x9eb8ab, depthTest: false, toneMapped: false }),
  );
  const bg = new THREE.Mesh(
    new THREE.PlaneGeometry(1.81, 0.22),
    new THREE.MeshBasicMaterial({ color: 0x010504, depthTest: false, toneMapped: false }),
  );
  const fg = new THREE.Mesh(
    new THREE.PlaneGeometry(1.75, 0.16).translate(1.75 / 2, 0, 0),
    new THREE.MeshBasicMaterial({
      color: TEAM_COLORS[team],
      depthTest: false,
      toneMapped: false,
    }),
  );
  bg.position.z = 0.005;
  fg.position.set(-1.75 / 2, 0, 0.01);
  border.renderOrder = 10;
  bg.renderOrder = 11;
  fg.renderOrder = 12;
  g.add(border, bg, fg);
  if (!human) {
    const icon = new THREE.Sprite(
      new THREE.SpriteMaterial({
        toneMapped: false,
        map: teamTexture(team),
        depthTest: false,
      }),
    );
    icon.scale.set(0.8, 0.4, 1);
    icon.position.y = 0.35;
    g.add(icon);
  }
  g.userData.fg = fg;
  const ammo = new THREE.Mesh(
    new THREE.PlaneGeometry(0.5, 0.045),
    new THREE.MeshBasicMaterial({ color: 0xf1d286, depthTest: false }),
  );
  ammo.position.y = -0.21;
  ammo.renderOrder = 13;
  g.add(ammo);
  g.userData.ammo = ammo;
  // One to three small gold chevrons beside the hull bar; keep team icons clear.
  const shape = new THREE.Shape();
  shape.moveTo(-0.16, -0.015);
  shape.lineTo(0, 0.075);
  shape.lineTo(0.16, -0.015);
  shape.lineTo(0.16, -0.075);
  shape.lineTo(0, 0.015);
  shape.lineTo(-0.16, -0.075);
  shape.closePath();
  const rankGeometry = new THREE.ShapeGeometry(shape);
  const rankMaterial = new THREE.MeshBasicMaterial({
    color: 0xffd477,
    depthTest: false,
    toneMapped: false,
  });
  g.userData.ranks = Array.from({ length: 3 }, (_, i) => {
    const chevron = new THREE.Mesh(rankGeometry, rankMaterial);
    chevron.position.set(-1.18, 0.15 - i * 0.15, 0.015);
    chevron.renderOrder = 13;
    chevron.visible = false;
    g.add(chevron);
    return chevron;
  });
  g.traverse((o) => {
    if (isMesh(o) || o instanceof THREE.Sprite) {
      // Transparent terrain is drawn after opaque meshes regardless of their
      // renderOrder. Keep all world-space HUD layers in the later pass too.
      const mat = o.material as THREE.Material;
      mat.transparent = true;
      mat.depthWrite = false;
      if (o instanceof THREE.Sprite) {
        o.renderOrder = 13;
      }
    }
    if (isMesh(o)) {
      o.geometry.userData.owned = true;
      (o.material as THREE.Material).userData.owned = true;
    }
    if (o instanceof THREE.Sprite) {
      (o.material as THREE.Material).userData.owned = true;
    }
  });
  return g;
}
