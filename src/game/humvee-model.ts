import * as THREE from "three";
import { TEAM_COLORS, VEHICLES } from "./data";
import { box, cylinder, material, put } from "./model-primitives";
import { applyTankSurface } from "./tank-surfaces";
import type { TankModel } from "./tank-model";
import type { Team } from "./types";

export const HUMVEE_BODY_LENGTH_SCALE = 1.16;

// Shared low-poly shells carry the silhouette; small fittings stay simple boxes.
function profileGeometry(points: [number, number][], width: number) {
  const profile = new THREE.Shape();
  points.forEach(([z, y], i) => (i ? profile.lineTo(z, y) : profile.moveTo(z, y)));
  profile.closePath();
  const geometry = new THREE.ExtrudeGeometry(profile, {
    depth: width,
    bevelEnabled: false,
    steps: 1,
  })
    .rotateY(-Math.PI / 2)
    .translate(width / 2, 0, 0);
  const positions = geometry.getAttribute("position");
  const normals = geometry.getAttribute("normal");
  const uv = geometry.getAttribute("uv");
  for (let i = 0; i < positions.count; i++) {
    const x = positions.getX(i);
    const y = positions.getY(i);
    const z = positions.getZ(i);
    const side = Math.abs(normals.getX(i)) > 0.5;
    const top = Math.abs(normals.getY(i)) > 0.5;
    uv.setXY(i, side ? z / 2 : x / width, top ? z / 2 : y);
  }
  return geometry;
}
const cabGeometry = profileGeometry(
  [
    [-2.02, 0.78],
    [1.06, 0.78],
    [0.74, 1.64],
    [-1.02, 1.64],
    [-2.02, 1.08],
  ],
  1.82,
);
const sideProfile: [number, number][] = [[-2.08, 0.3]];
for (const z of [-1.32, 1.3]) {
  for (let i = 0; i <= 8; i++) {
    const angle = Math.PI - (i * Math.PI) / 8;
    sideProfile.push([z + Math.cos(angle) * 0.57, 0.25 + Math.sin(angle) * 0.57]);
  }
}
sideProfile.push([2.06, 0.42], [2.06, 0.97], [1.04, 1.08], [-2.08, 0.98]);
const sideGeometry = profileGeometry(sideProfile, 0.22);
// The rear door's lower trailing corner clears the rear wheel arch.
const rearDoorBorder = profileGeometry(
  [
    [-0.42, -0.08],
    [-0.18, -0.47],
    [0.42, -0.47],
    [0.42, 0.47],
    [-0.42, 0.47],
  ],
  0.025,
);
const rearDoorPanel = profileGeometry(
  [
    [-0.39, -0.065],
    [-0.155, -0.44],
    [0.39, -0.44],
    [0.39, 0.44],
    [-0.39, 0.44],
  ],
  0.025,
);
const tireGeometry = new THREE.LatheGeometry(
  [
    [0.235, -0.14],
    [0.36, -0.16],
    [0.46, -0.12],
    [0.48, -0.065],
    [0.48, 0.065],
    [0.46, 0.12],
    [0.36, 0.16],
    [0.235, 0.14],
  ].map(([radius, axial]) => new THREE.Vector2(radius, axial)),
  20,
).rotateZ(Math.PI / 2);

export function humveeModel(team: Team, wreck = false): TankModel {
  const root = new THREE.Group() as TankModel;
  const hull = new THREE.Group();
  const turret = new THREE.Group();
  const barrel = new THREE.Group();
  const trackGroup = new THREE.Group();
  const accent = wreck ? 0x3c4650 : TEAM_COLORS[team];
  const base = accent;
  const shade = wreck ? 0x13232c : new THREE.Color(base).multiplyScalar(0.62).getHex();
  const steel = wreck ? 0x37424c : 0x637581;
  const rubber = wreck ? 0x1d252b : 0x17201d;
  const glass = wreck ? 0x1a2328 : 0x29444b;
  root.add(hull, turret);
  hull.add(trackGroup);
  hull.scale.z = HUMVEE_BODY_LENGTH_SCALE;
  // Wheels retain their circular section while the shell and wheelbase lengthen.
  trackGroup.scale.z = 1 / HUMVEE_BODY_LENGTH_SCALE;

  put(hull, box(1.4, 0.22, 3.82, shade, 0), 0, 0.42, -0.1);
  put(hull, box(1.65, 0.15, 3.74, base, 0), 0, 0.85, -0.05);
  const cab = new THREE.Mesh(cabGeometry, material(base));
  cab.castShadow = cab.receiveShadow = true;
  hull.add(cab);
  put(hull, box(1.94, 0.1, 1.84, base, 0.02), 0, 1.67, -0.1);
  const hood = box(1.9, 0.18, 1.13, base, 0.025);
  hood.rotation.x = 0.1;
  put(hull, hood, 0, 1.0, 1.48);
  put(hull, box(0.95, 0.025, 0.32, shade, 0), 0, 1.11, 1.18);
  for (let i = -4; i <= 4; i++) {
    put(hull, box(0.045, 0.025, 0.29, steel, 0), i * 0.1, 1.13, 1.18);
  }
  for (const x of [-0.4, 0.4]) {
    const windshield = box(0.72, 0.49, 0.025, glass, 0);
    windshield.rotation.x = -0.356;
    put(hull, windshield, x, 1.36, 0.86);
    const wiper = box(0.035, 0.34, 0.018, rubber, 0);
    wiper.rotation.set(-0.356, 0, -0.38);
    put(hull, wiper, x + 0.07, 1.29, 0.902);
  }

  for (const side of [-1, 1]) {
    const sidePanel = new THREE.Mesh(sideGeometry, material(base));
    sidePanel.castShadow = sidePanel.receiveShadow = true;
    put(hull, sidePanel, side * 0.97, 0, 0);
    for (const z of [-0.56, 0.34]) {
      const rear = z < 0;
      put(
        hull,
        rear ? new THREE.Mesh(rearDoorBorder, material(shade)) : box(0.025, 0.94, 0.84, shade, 0),
        side * 1.09,
        0.91,
        z,
      );
      put(
        hull,
        rear ? new THREE.Mesh(rearDoorPanel, material(base)) : box(0.025, 0.88, 0.78, base, 0),
        side * 1.105,
        0.91,
        z,
      );
      put(hull, box(0.026, 0.34, 0.62, glass, 0), side * 0.925, 1.39, z);
      put(hull, box(0.055, 0.055, 0.17, rubber, 0), side * 1.125, 1.12, z - 0.28);
      for (const tilt of [-0.65, 0.65]) {
        const rib = box(0.024, 0.045, rear ? 0.56 : 0.72, shade, 0);
        rib.rotation.x = tilt;
        put(hull, rib, side * 1.123, 0.76, z);
      }
      // Raised armored window frames and external door hinges.
      for (const y of [1.2, 1.58]) {
        put(hull, box(0.055, 0.045, 0.7, shade, 0), side * 0.94, y, z);
      }
      for (const edge of [-0.35, 0.35]) {
        put(hull, box(0.055, 0.4, 0.045, shade, 0), side * 0.94, 1.39, z + edge);
      }
      for (const y of [0.74, 1.08]) {
        put(hull, box(0.07, 0.07, 0.13, steel, 0), side * 1.13, y, z + 0.31);
      }
    }
    put(hull, box(0.05, 0.05, 0.3, steel, 0), side * 1.015, 1.26, 0.74);
    put(hull, box(0.09, 0.2, 0.14, shade, 0), side * 1.11, 1.32, 0.85);
    put(hull, box(0.18, 0.06, 1.2, steel, 0), side * 1.01, 0.44, -0.1);
    put(hull, box(0.06, 0.16, 0.08, rubber, 0), side * 0.975, 0.94, 1.7);
    put(hull, box(0.16, 0.08, 0.06, wreck ? shade : 0xdf923b, 0), side * 0.73, 0.99, 2.06);
    put(hull, box(0.13, 0.16, 0.035, wreck ? shade : 0x731c15, 0), side * 0.74, 0.85, -2.09);

    for (const z of [-1.32, 1.3]) {
      const wheel = new THREE.Mesh(tireGeometry, material(rubber, 0, 0.92));
      wheel.castShadow = wheel.receiveShadow = true;
      put(trackGroup, wheel, side, 0.25, z * HUMVEE_BODY_LENGTH_SCALE);
      const rim = cylinder(0.24, 0.028, shade, 16);
      rim.rotation.z = Math.PI / 2;
      put(trackGroup, rim, side * 1.145, 0.25, z * HUMVEE_BODY_LENGTH_SCALE);
      const hub = cylinder(0.13, 0.025, steel, 10);
      hub.rotation.z = Math.PI / 2;
      put(trackGroup, hub, side * 1.15, 0.25, z * HUMVEE_BODY_LENGTH_SCALE);
      for (let i = 0; i < 20; i++) {
        const angle = (i * Math.PI * 2) / 20;
        for (const offset of [-0.07, 0.07]) {
          const tread = box(0.115, 0.028, 0.095, rubber, 0);
          tread.rotation.set(angle, offset > 0 ? 0.3 : -0.3, 0);
          put(
            trackGroup,
            tread,
            side + offset,
            0.25 + Math.cos(angle) * 0.476,
            z * HUMVEE_BODY_LENGTH_SCALE + Math.sin(angle) * 0.476,
          );
        }
        if (i % 2 === 0) {
          const bolt = cylinder(0.016, 0.008, steel, 5);
          bolt.rotation.z = Math.PI / 2;
          put(
            trackGroup,
            bolt,
            side * 1.162,
            0.25 + Math.cos(angle) * 0.19,
            z * HUMVEE_BODY_LENGTH_SCALE + Math.sin(angle) * 0.19,
          );
        }
      }
      // Thin arch lip follows the opening instead of a rectangular shelf over the tire.
      for (let i = 0; i < 8; i++) {
        const angle = ((i + 0.5) * Math.PI) / 8;
        const lip = box(0.06, 0.06, 0.235, shade, 0);
        lip.rotation.x = Math.PI / 2 - angle;
        put(hull, lip, side * 1.105, 0.25 + Math.sin(angle) * 0.59, z + Math.cos(angle) * 0.59);
      }
    }
  }
  // Recessed vertical grille, circular lamps and a full-width bumper.
  put(hull, box(1.83, 0.34, 0.1, base, 0), 0, 0.79, 2.0);
  put(hull, box(0.99, 0.24, 0.025, rubber, 0), 0, 0.8, 2.06);
  for (let i = -3; i <= 3; i++) {
    put(hull, box(0.045, 0.25, 0.025, base, 0), i * 0.14, 0.8, 2.08);
  }
  for (const x of [-0.71, 0.71]) {
    const lamp = cylinder(0.115, 0.035, wreck ? shade : 0xe5ddbc, 12);
    lamp.rotation.x = Math.PI / 2;
    put(hull, lamp, x, 0.82, 2.07);
  }
  put(hull, box(2.1, 0.2, 0.18, shade, 0.02), 0, 0.44, 2.08);
  put(hull, box(1.8, 0.4, 0.12, base, 0.02), 0, 0.85, -2.01);
  // Slanted rear hatch, inset tail panel and bumper replace the open pickup-like tail.
  const rearHatch = box(1.45, 0.025, 0.9, shade, 0);
  rearHatch.rotation.x = -Math.atan2(0.56, 1);
  put(hull, rearHatch, 0, 1.375, -1.55);
  put(hull, box(1.38, 0.25, 0.025, shade, 0), 0, 0.79, -2.08);
  put(hull, box(2.08, 0.14, 0.14, shade, 0), 0, 0.38, -2.08);
  for (const side of [-1, 1]) {
    const lamp = cylinder(0.07, 0.03, wreck ? shade : 0x931e16, 10);
    lamp.rotation.x = Math.PI / 2;
    put(hull, lamp, side * 0.79, 0.83, -2.1);
    put(hull, box(0.27, 0.3, 0.035, rubber, 0), side, 0.22, -1.91);
  }
  // Raised intake, tie-downs and recovery eyes stay inside the existing hull footprint.
  put(hull, cylinder(0.065, 0.65, shade, 8), -0.82, 1.36, 0.9);
  put(hull, cylinder(0.095, 0.06, rubber, 8), -0.82, 1.71, 0.9);
  for (const x of [-0.7, 0.7]) {
    put(hull, box(0.08, 0.1, 0.08, steel, 0), x, 0.49, 2.12);
    put(hull, box(0.08, 0.06, 0.22, steel, 0), x, 1.75, -0.7);
  }
  put(hull, cylinder(0.018, 0.78, steel, 6), -0.82, 1.98, -1.92);

  const launcherY = 2.13;
  put(turret, cylinder(0.44, 0.1, shade, 16), 0, 1.78, 0);
  put(turret, box(0.22, 0.27, 0.36, steel, 0.02), 0, 1.93, 0);
  const tube = cylinder(0.15, 1.5, shade, 12);
  tube.rotation.x = Math.PI / 2;
  put(barrel, tube, 0, launcherY, 0.42);
  for (const z of [-0.3, 1.14]) {
    const collar = cylinder(0.18, 0.1, steel, 12);
    collar.rotation.x = Math.PI / 2;
    put(barrel, collar, 0, launcherY, z);
  }
  const opening = cylinder(0.13, 0.012, rubber, 12);
  opening.rotation.x = Math.PI / 2;
  put(barrel, opening, 0, launcherY, 1.196);
  put(turret, box(0.36, 0.35, 0.55, shade, 0.025), 0.36, launcherY, 0.32);
  for (const x of [0.28, 0.44]) {
    const lens = cylinder(0.067, 0.025, glass, 12);
    lens.rotation.x = Math.PI / 2;
    put(turret, lens, x, launcherY, 0.61);
  }
  for (const side of [-1, 1]) {
    put(turret, box(0.065, 0.32, 0.75, base, 0), side * 0.52, 1.96, -0.13);
  }
  put(turret, box(1.05, 0.32, 0.06, shade, 0), 0, 1.96, -0.5);
  const muzzle = new THREE.Object3D();
  muzzle.position.set(0, launcherY, 1.7);
  barrel.add(muzzle);
  turret.add(barrel);
  root.scale.setScalar(VEHICLES.humvee.scale);
  root.userData = { kind: "humvee", hull, turret, barrel, trackGroup, muzzle };
  applyTankSurface(root, [base, shade, steel]);
  return root;
}
