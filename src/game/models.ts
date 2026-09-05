import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { TEAM_COLORS, VEHICLES } from "./data";
import type { VehicleKind, Team, Cover } from "./types";
const materials = new Map<string, THREE.MeshStandardMaterial>();
export function material(color: number, metalness = 0.05, roughness = 0.65) {
  const key = `${color}/${metalness}/${roughness}`;
  let m = materials.get(key);
  if (!m) {
    m = new THREE.MeshStandardMaterial({ color, metalness, roughness });
    if (TEAM_COLORS.includes(color)) {
      m.emissive.setHex(color);
      m.emissiveIntensity = 0.04;
      m.toneMapped = false;
    }
    materials.set(key, m);
  }
  return m;
}
const boxes = new Map<string, THREE.BufferGeometry>();
export function box(w: number, h: number, d: number, color: number, r = 0.06) {
  const key = [w, h, d, r].join("/");
  let geo = boxes.get(key);
  if (!geo) {
    geo =
      r > 0
        ? new RoundedBoxGeometry(w, h, d, 1, r)
        : new THREE.BoxGeometry(w, h, d);
    boxes.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
export function put(
  parent: THREE.Object3D,
  obj: THREE.Object3D,
  x = 0,
  y = 0,
  z = 0,
) {
  obj.position.set(x, y, z);
  parent.add(obj);
  return obj;
}
const cylinders = new Map<string, THREE.CylinderGeometry>();
export function cylinder(
  radius: number,
  height: number,
  color: number,
  sides = 12,
) {
  const key = `${radius}/${height}/${sides}`;
  let geo = cylinders.get(key);
  if (!geo) {
    geo = new THREE.CylinderGeometry(radius, radius, height, sides);
    cylinders.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color, 0.2));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
// Shared geometry keeps the more detailed silhouette inexpensive to instance/batch.
const armorGeometry = new Map<number, THREE.BufferGeometry>();
function armor(w: number, h: number, d: number, color: number, taper = 0.76) {
  let geo = armorGeometry.get(taper);
  if (!geo) {
    // Chamfered rectangular plates, with a recessed roof and sloping front glacis.
    const outline = [
      [-0.38, -0.5],
      [0.38, -0.5],
      [0.5, -0.36],
      [0.5, 0.32],
      [0.32, 0.5],
      [-0.32, 0.5],
      [-0.5, 0.32],
      [-0.5, -0.36],
    ];
    const vertices: number[] = [],
      indices: number[] = [];
    for (const top of [false, true])
      for (const [x, z] of outline)
        vertices.push(
          x * (top ? taper : 1),
          top ? 0.5 : -0.5,
          top ? z * taper - 0.04 : z,
        );
    for (let i = 0; i < 8; i++) {
      const next = (i + 1) % 8;
      indices.push(i, i + 8, next, next, i + 8, next + 8);
    }
    for (let i = 1; i < 7; i++) indices.push(0, i, i + 1, 8, i + 9, i + 8);
    const indexed = new THREE.BufferGeometry();
    indexed.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(vertices, 3),
    );
    indexed.setIndex(indices);
    geo = indexed.toNonIndexed();
    indexed.dispose();
    geo.computeVertexNormals();
    geo.setAttribute(
      "uv",
      new THREE.Float32BufferAttribute(
        new Float32Array(geo.getAttribute("position").count * 2),
        2,
      ),
    );
    armorGeometry.set(taper, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color, 0.18, 0.58));
  mesh.scale.set(w, h, d);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
const beltShape = new THREE.Shape();
beltShape.moveTo(-0.9, -0.33);
beltShape.lineTo(0.9, -0.33);
beltShape.absarc(0.9, 0, 0.33, -Math.PI / 2, Math.PI / 2, false);
beltShape.lineTo(-0.9, 0.33);
beltShape.absarc(-0.9, 0, 0.33, Math.PI / 2, Math.PI * 1.5, false);
const beltGeometry = new THREE.ExtrudeGeometry(beltShape, {
  depth: 0.54,
  bevelEnabled: false,
  curveSegments: 6,
})
  .translate(0, 0, -0.27)
  .rotateY(Math.PI / 2);
function trackBelt(color: number) {
  const mesh = new THREE.Mesh(beltGeometry, material(color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

const scoutTurretGeometry = new THREE.CylinderGeometry(0.54, 0.66, 0.72, 12);
export function tankModel(kind: VehicleKind, team: Team, wreck = false) {
  const root = new THREE.Group(),
    hull = new THREE.Group(),
    turret = new THREE.Group();
  const color = wreck ? 0x3c4650 : TEAM_COLORS[team];
  const dark = 0x132c3f,
    steel = wreck ? 0x37424c : 0x5c6b7c;
  const shade = wreck
    ? dark
    : new THREE.Color(color).multiplyScalar(0.65).getHex();
  const heavy = kind === "heavy",
    scout = kind === "scout";
  const width = heavy ? 2.14 : scout ? 1.8 : 2.02;
  root.add(hull);
  // Broad shoulder plates and a pronounced sloping nose, as in the reference silhouettes.
  put(hull, armor(width, 0.32, 2.75, shade, 0.94), 0, 0.22, 0);
  put(hull, armor(width, 0.56, 2.8, color, 0.77), 0, 0.51, 0);
  const tracks: THREE.Mesh[] = [];
  for (const side of [-1, 1]) {
    const belt = trackBelt(dark);
    belt.scale.set(1, 1.08, 1.19);
    put(hull, belt, side * 1.0, 0.17, 0);
    // Keep the running gear a clean dark silhouette, with restrained tread definition.
    for (let j = 0; j < 10; j++) {
      const tread = box(0.54, 0.025, 0.065, 0x294152, 0.005);
      put(hull, tread, side * 1, 0.53, -1.2 + j * 0.25);
      tracks.push(tread);
    }
    put(hull, box(0.51, 0.12, 2.65, color, 0.025), side * 0.98, 0.64, 0);
    if (!scout) {
      for (let j = 0; j < 3; j++)
        put(
          hull,
          box(0.35, 0.04, 0.48, steel, 0.008),
          side * 0.98,
          0.724,
          -0.72 + j * 0.67,
        );
    }
    put(hull, box(0.16, 0.13, 0.12, steel, 0.015), side * 0.66, 0.53, 1.31);
    put(hull, box(0.17, 0.11, 0.08, 0x91333b, 0.01), side * 0.64, 0.43, -1.4);
  }
  put(hull, box(width + 0.34, 0.14, 0.23, color, 0.015), 0, 0.47, 1.34);
  for (let i = 0; i < (scout ? 3 : 4); i++)
    put(
      hull,
      box(0.15, 0.035, 0.42, steel, 0.005),
      -0.4 + i * 0.24,
      0.77,
      -0.94,
    );

  put(turret, cylinder(scout ? 0.56 : 0.73, 0.1, steel, 20), 0, 0.79, -0.15);
  let roof: number;
  if (scout) {
    // A simple tall cast turret gives the light tank its own recognizable outline.
    const cast = new THREE.Mesh(scoutTurretGeometry, material(color));
    cast.scale.z = 1.12;
    cast.castShadow = cast.receiveShadow = true;
    put(turret, cast, 0, 1.15, -0.13);
    put(turret, armor(0.96, 0.45, 0.6, color, 0.83), 0, 1.21, -0.57);
    roof = 1.52;
  } else {
    // Large angular turrets, wide rear shoulders and a thick mantlet carry the heavier classes.
    put(
      turret,
      armor(
        heavy ? 2.03 : 1.78,
        0.76,
        heavy ? 1.96 : 1.9,
        color,
        heavy ? 0.62 : 0.78,
      ),
      0,
      1.19,
      -0.17,
    );
    roof = 1.58;
    for (const side of [-1, 1]) {
      const panel = box(0.055, 0.28, 0.63, steel, 0.01);
      panel.rotation.z = side * -0.3;
      put(turret, panel, side * (heavy ? 0.91 : 0.8), 1.18, -0.26);
      for (let j = 0; j < 2; j++)
        put(
          turret,
          box(0.24, 0.035, 0.43, steel, 0.005),
          side * (0.28 + j * 0.25),
          roof + 0.025,
          -0.61,
        );
    }
  }
  put(
    turret,
    cylinder(scout ? 0.17 : 0.2, 0.08, steel, 12),
    -0.2,
    roof + 0.04,
    -0.17,
  );
  if (!scout)
    put(turret, cylinder(0.17, 0.07, steel, 12), 0.19, roof + 0.035, -0.28);
  put(turret, box(0.23, 0.045, 0.11, steel, 0.008), 0.23, roof + 0.025, 0.11);
  const barrel = new THREE.Group(),
    gunY = scout ? 1.08 : 1.15;
  const tubeRadius = scout ? 0.13 : heavy ? 0.18 : 0.16;
  const mantlet = cylinder(tubeRadius * 1.65, 0.48, steel, 12);
  mantlet.rotation.x = Math.PI / 2;
  put(barrel, mantlet, 0, gunY, 0.63);
  const tube = cylinder(tubeRadius, 1.55, steel, 12);
  tube.rotation.x = Math.PI / 2;
  put(barrel, tube, 0, gunY, 1.5);
  if (!scout) {
    put(
      barrel,
      box(tubeRadius * 2.6, tubeRadius * 2.3, 0.45, steel, 0.025),
      0,
      gunY,
      2.25,
    );
    for (const side of [-1, 1])
      for (let j = 0; j < 3; j++)
        put(
          barrel,
          box(0.012, 0.17, 0.055, dark, 0.002),
          side * tubeRadius * 1.31,
          gunY,
          2.1 + j * 0.12,
        );
  }
  const bore = cylinder(tubeRadius * 0.72, 0.008, dark, 12);
  bore.rotation.x = Math.PI / 2;
  put(barrel, bore, 0, gunY, scout ? 2.28 : 2.48);
  turret.add(barrel);
  root.add(turret);
  root.scale.setScalar(VEHICLES[kind].scale);
  // Small team symbols preserve identification without dominating the gray armor details.
  if (team === 0) {
    const badge = box(0.18, 0.025, 0.18, 0xdce7ee, 0.005);
    badge.rotation.y = Math.PI / 4;
    put(turret, badge, -0.23, roof + 0.018, 0.22);
  } else
    for (const x of [-0.28, -0.15])
      put(
        turret,
        box(0.06, 0.025, 0.2, 0xdce7ee, 0.005),
        x,
        roof + 0.018,
        0.22,
      );
  root.userData = { hull, turret, barrel, tracks };
  return root;
}
const roofProfile = new THREE.Shape();
roofProfile.moveTo(-0.5, 0);
roofProfile.lineTo(0, 1);
roofProfile.lineTo(0.5, 0);
roofProfile.closePath();
const roofGeometry = new THREE.ExtrudeGeometry(roofProfile, {
  depth: 1,
  bevelEnabled: false,
}).translate(0, 0, -0.5);
const pineGeometry = new THREE.ConeGeometry(1, 1, 7);
function pitchedRoof(w: number, h: number, d: number, color: number) {
  const mesh = new THREE.Mesh(roofGeometry, material(color));
  mesh.scale.set(w, h, d);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
export function coverModel(
  c: Pick<Cover, "kind" | "x" | "z" | "w" | "d" | "h" | "color">,
) {
  const g = new THREE.Group();
  g.position.set(c.x, 0, c.z);
  if (c.kind === "house") {
    const wall = c.h * 0.68;
    put(g, box(c.w + 0.2, 0.22, c.d + 0.2, 0xa1977c), 0, 0.11, 0);
    put(g, box(c.w, wall, c.d, c.color, 0.025), 0, wall / 2, 0);
    for (let y = 0.45; y < wall; y += 0.48) {
      for (const side of [-1, 1]) {
        put(
          g,
          box(c.w + 0.12, 0.06, 0.09, 0x8b5837, 0.008),
          0,
          y,
          (side * c.d) / 2,
        );
        put(
          g,
          box(0.09, 0.06, c.d + 0.12, 0x8b5837, 0.008),
          (side * c.w) / 2,
          y,
          0,
        );
      }
    }
    for (const side of [-1, 1]) {
      for (const x of [-c.w * 0.29, c.w * 0.29]) {
        put(
          g,
          box(1.05, 0.97, 0.07, 0xffd94e, 0.025),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.045),
        );
        put(
          g,
          box(0.075, 0.97, 0.085, 0x875534),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.09),
        );
        put(
          g,
          box(1.05, 0.075, 0.085, 0x875534),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.09),
        );
      }
      put(
        g,
        box(0.07, 1.05, 1.1, 0xffd94e),
        side * (c.w / 2 + 0.05),
        wall * 0.58,
        0,
      );
    }
    put(g, box(0.82, 1.55, 0.1, 0x64452f), 0, 0.85, c.d / 2 + 0.06);
    put(g, box(0.1, 0.1, 0.12, 0xffd24a), 0.24, 0.83, c.d / 2 + 0.12);
    const roofColor = Math.abs(c.z) > 35 ? 0xcc493c : 0x167857;
    put(
      g,
      pitchedRoof(c.w + 0.6, c.h - wall, c.d + 0.6, roofColor),
      0,
      wall,
      0,
    );
    put(
      g,
      box(0.58, 1.0, 0.58, 0xbc5c3e, 0.02),
      -c.w * 0.25,
      c.h - 0.36,
      -c.d * 0.2,
    );
  } else if (c.kind === "tree") {
    put(g, cylinder(0.22, c.h * 0.55, 0x805034, 7), 0, c.h * 0.275, 0);
    for (const [y, radius, height, color] of [
      [0.43, 0.5, 0.56, 0x128458],
      [0.65, 0.39, 0.5, c.color],
      [0.85, 0.26, 0.3, 0x31b96c],
    ]) {
      const leaves = new THREE.Mesh(pineGeometry, material(color));
      leaves.scale.set(c.w * radius, c.h * height, c.d * radius);
      leaves.castShadow = leaves.receiveShadow = true;
      put(g, leaves, 0, c.h * y, 0);
    }
  } else if (c.kind === "fence") {
    const along = c.w > c.d,
      length = Math.max(c.w, c.d);
    for (let offset = -length / 2 + 0.12; offset <= length / 2; offset += 0.55)
      put(
        g,
        box(along ? 0.24 : 0.18, c.h, along ? 0.18 : 0.24, c.color, 0.018),
        along ? offset : 0,
        c.h / 2,
        along ? 0 : offset,
      );
    for (const y of [0.45, 1.12])
      put(
        g,
        box(along ? length : 0.2, 0.18, along ? 0.2 : length, 0x8f603a, 0.01),
        0,
        y,
        0,
      );
  } else if (c.kind === "drum") {
    put(g, cylinder(0.6, 1.6, c.color), 0, 0.8, 0);
    for (const y of [0.22, 1.35])
      put(g, cylinder(0.63, 0.1, 0x574e3e), 0, y, 0);
    put(g, cylinder(0.15, 0.05, 0x343c31), 0.25, 1.63, 0);
    const stripe = box(1.21, 0.3, 0.16, 0xf4d998);
    put(g, stripe, 0, 0.8, 0.5);
  } else if (c.kind === "tower") {
    for (const x of [-2.5, 2.5])
      for (const z of [-2, 2]) {
        put(g, box(0.35, 5.5, 0.35, 0x766f56), x, 2.75, z);
        put(g, box(0.85, 0.3, 0.85, 0xb5ad96), x, 0.15, z);
      }
    for (const x of [-2.5, 2.5]) {
      const brace = box(0.2, 6, 0.22, c.color);
      brace.rotation.x = 0.65;
      put(g, brace, x, 2.7, 0);
    }
    put(g, box(6, 0.35, 5, 0x887d59), 0, 5, 0);
    put(g, box(5.7, 2.15, 4.7, c.color), 0, 6.15, 0);
    for (const z of [-2.4, 2.4])
      put(g, box(4, 0.65, 0.08, 0x164e79), 0, 6.4, z);
    put(g, pitchedRoof(6.5, 1.2, 5.5, 0x197451), 0, 7.25, 0);
    for (let i = 0; i < 9; i++)
      put(g, box(0.9, 0.08, 0.18, 0xe2cc93), 2.65, 0.4 + i * 0.55, 2.15);
  } else if (c.kind === "shed") {
    put(g, box(c.w, c.h, c.d, c.color), 0, c.h / 2, 0);
    const along = c.w > c.d;
    const length = along ? c.w : c.d;
    for (let i = -length / 2 + 0.2; i < length / 2; i += 0.52)
      put(
        g,
        box(
          along ? 0.035 : c.w + 0.04,
          c.h - 0.1,
          along ? c.d + 0.04 : 0.035,
          0x9c713e,
          0.005,
        ),
        along ? i : 0,
        c.h / 2,
        along ? 0 : i,
      );
    put(g, box(c.w + 0.14, 0.18, c.d + 0.14, 0x187fbe), 0, c.h, 0);
  } else {
    put(
      g,
      box(c.w, c.h, c.d, c.color, c.kind === "boundary" ? 0.06 : 0.16),
      0,
      c.h / 2,
      0,
    );
    if (c.kind === "concrete" || c.kind === "wall") {
      const along = c.w > c.d;
      for (let i = 0; i < Math.floor((along ? c.w : c.d) / 1.1); i++) {
        const mark = box(
          along ? 0.55 : 0.035,
          0.22,
          along ? 0.035 : 0.55,
          0x499ac7,
          0.01,
        );
        put(
          g,
          mark,
          along ? -c.w / 2 + 0.6 + i * 1.1 : c.w / 2 + 0.02,
          c.h * 0.7,
          along ? c.d / 2 + 0.02 : -c.d / 2 + 0.6 + i * 1.1,
        );
      }
    }
  }
  return g;
}
const textures = new Map<string, THREE.CanvasTexture>();
export function labelTexture(
  text: string,
  color = "#fff4cf",
  bg = "transparent",
) {
  const key = text + color + bg;
  const cached = textures.get(key);
  if (cached) return cached;
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 128;
  const c = canvas.getContext("2d")!;
  c.fillStyle = bg;
  c.fillRect(0, 0, 256, 128);
  c.fillStyle = color;
  c.font = "bold 76px sans-serif";
  c.textAlign = "center";
  c.textBaseline = "middle";
  c.fillText(text, 128, 64);
  const texture = new THREE.CanvasTexture(canvas);
  textures.set(key, texture);
  return texture;
}
