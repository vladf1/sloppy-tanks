import * as THREE from "three";
import { PICKUPS, SHIELD_CAPACITY, TEAM_COLORS } from "./data";
import { isMesh } from "./render-resources";
import { SIMULATION_RULES } from "./simulation-rules";
import type { Tank } from "./types";
type HudMesh = THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicMaterial>;
type ProtectionMeter = { group: THREE.Group; fill: HudMesh };
export interface TankBar extends THREE.Group {
  userData: {
    fg: HudMesh;
    ranks: HudMesh[];
    shield: ProtectionMeter;
    spawn: ProtectionMeter;
  };
}

function protectionMeter(color: number, segmented: boolean): ProtectionMeter {
  const group = new THREE.Group();
  const paint = new THREE.MeshBasicMaterial({ color, depthTest: false, toneMapped: false });
  const dark = new THREE.MeshBasicMaterial({
    color: 0x07141f,
    depthTest: false,
    toneMapped: false,
  });
  const shape = new THREE.Shape();
  shape.moveTo(-0.13, 0.12);
  shape.lineTo(0.13, 0.12);
  shape.lineTo(0.11, -0.035);
  shape.lineTo(0, -0.14);
  shape.lineTo(-0.11, -0.035);
  shape.closePath();
  const badge = new THREE.Mesh(new THREE.ShapeGeometry(shape), paint);
  badge.position.x = -0.8;
  const rim = new THREE.Mesh(badge.geometry, dark);
  rim.position.copy(badge.position);
  rim.scale.setScalar(1.3);
  rim.renderOrder = 13;
  badge.renderOrder = 14;
  const track = new THREE.Mesh(new THREE.PlaneGeometry(1.42, 0.18), dark);
  track.position.x = 0.16;
  track.renderOrder = 13;
  const fill = new THREE.Mesh(new THREE.PlaneGeometry(1.34, 0.1).translate(0.67, 0, 0), paint);
  fill.position.x = -0.51;
  fill.renderOrder = 14;
  group.add(rim, badge, track, fill);
  // Three charge sections distinguish pickup armor from the continuous spawn timer.
  if (segmented) {
    for (const fraction of [1 / 3, 2 / 3]) {
      const divider = new THREE.Mesh(new THREE.PlaneGeometry(0.035, 0.12), dark);
      divider.position.x = -0.51 + 1.34 * fraction;
      divider.renderOrder = 15;
      group.add(divider);
    }
  }
  group.visible = false;
  return { group, fill };
}

/** Read live protection state so absorption, expiry, firing and respawn update immediately. */
export function updateTankProtection(bar: TankBar, tank: Tank): void {
  const { shield, spawn } = bar.userData;
  shield.group.visible = tank.alive && tank.shield > 0 && tank.shieldPoints > 0;
  shield.fill.scale.x = THREE.MathUtils.clamp(tank.shieldPoints / SHIELD_CAPACITY, 0, 1);
  shield.group.position.y = 0.38;
  spawn.group.visible = tank.alive && tank.protection > 0;
  spawn.fill.scale.x = THREE.MathUtils.clamp(
    tank.protection / SIMULATION_RULES.spawnProtectionSeconds,
    0,
    1,
  );
  spawn.group.position.y = shield.group.visible ? 0.72 : 0.38;
}

/** Billboard layers share the transparent pass so terrain cannot draw over them. */
export function createTankBar(team: number): TankBar {
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
  g.userData.fg = fg;
  g.userData.shield = protectionMeter(PICKUPS.shield.color, true);
  g.userData.spawn = protectionMeter(0xffdf86, false);
  g.add(g.userData.shield.group, g.userData.spawn.group);
  // One to three small gold chevrons beside the hull bar.
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
    if (isMesh(o)) {
      // Transparent terrain is drawn after opaque meshes regardless of their
      // renderOrder. Keep all world-space HUD layers in the later pass too.
      const mat = o.material as THREE.Material;
      mat.transparent = true;
      mat.depthWrite = false;
      o.geometry.userData.owned = true;
      (o.material as THREE.Material).userData.owned = true;
    }
  });
  return g;
}
