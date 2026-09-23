import * as THREE from "three";
import { harborBox } from "./harbor-surfaces";
import { Random } from "./math";
import { box, cylinder, put } from "./model-primitives";
import { spawnPositions } from "./arena";
import { type RubbleStone, sandstoneRubble } from "./quarry-surfaces";

function beam(group: THREE.Group, a: number[], b: number[], width: number, color: number) {
  const start = new THREE.Vector3(...a);
  const end = new THREE.Vector3(...b);
  const mesh = harborBox(width, start.distanceTo(end), width, color);
  mesh.position.copy(start).add(end).multiplyScalar(0.5);
  mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), end.sub(start).normalize());
  group.add(mesh);
}

function siteSign(group: THREE.Group) {
  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 256;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#d6cbb0";
  ctx.fillRect(0, 0, 512, 256);
  ctx.fillStyle = "#353e3c";
  ctx.fillRect(0, 0, 512, 62);
  ctx.fillStyle = "#eee4c9";
  ctx.font = "bold 30px sans-serif";
  ctx.fillText("DUSTY DIG / 03", 25, 42);
  ctx.fillStyle = "#353e3c";
  ctx.font = "bold 49px sans-serif";
  ctx.fillText("ACTIVE QUARRY", 24, 129);
  ctx.font = "24px sans-serif";
  ctx.fillText("HAUL ROAD   •   KEEP CLEAR", 25, 175);
  ctx.fillStyle = "#b58b39";
  ctx.fillRect(0, 211, 512, 45);
  for (let x = -40; x < 550; x += 64) {
    ctx.fillStyle = "#353e3c";
    ctx.beginPath();
    ctx.moveTo(x, 256);
    ctx.lineTo(x + 40, 211);
    ctx.lineTo(x + 65, 211);
    ctx.lineTo(x + 25, 256);
    ctx.fill();
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const face = new THREE.Mesh(
    new THREE.PlaneGeometry(5.8, 2.9),
    new THREE.MeshStandardMaterial({ map: texture, roughness: 0.95 }),
  );
  put(group, face, 49, 2.6, -66.9);
  for (const x of [46.7, 51.3]) {
    put(group, harborBox(0.14, 5, 0.14, 0x535953), x, 0.7, -67.1);
  }
}

/** Machinery, stockpiles and haul lanes on the apron that scrub never grows on. */
const SCRUB_KEEP_OUT: [number, number, number, number][] = [
  [-31, -16, -76, -60], // excavator
  [-66, -38, -74, -65], // screening conveyor and hopper
  [5, 27, -78, -59], // sentinel butte
  [23, 54, -76, -62], // site office, water tank, drums and sign
  [-55, -36, 63, 74], // cut stone stacks
  [62, 77, 8, 28], // haul truck bay
  [63, 86, 24, 70], // east haul ramp
];

/** All tall dressing is outside the playable wall; in-arena gravel is only 2–4cm high. */
export function quarrySiteDetails(equipment: THREE.Group, gravel: THREE.Group): void {
  const rng = new Random(62541);
  // Spilled haul loads leave tight clusters of flat gravel across the work floor,
  // with a scatter of strays between them. Spawn pads stay clean.
  const pads = [...spawnPositions(0), ...spawnPositions(1)];
  const stones: RubbleStone[] = [];
  const stone = (x: number, z: number, size: number) => {
    if (
      Math.max(Math.abs(x), Math.abs(z)) > 58.5 ||
      pads.some((p) => Math.hypot(x - p.x, z - p.z) < 3.4)
    ) {
      return;
    }
    stones.push({
      x,
      y: 0.008,
      z,
      w: size,
      h: rng.range(0.04, 0.07),
      d: size * rng.range(0.6, 1.1),
      rotY: rng.range(-Math.PI, Math.PI),
      shade: rng.range(0.55, 0.95),
    });
  };
  for (let cluster = 0; cluster < 40; cluster++) {
    const cx = rng.range(-56, 56);
    const cz = rng.range(-56, 56);
    const spread = rng.range(0.8, 2.6);
    const count = 10 + Math.floor(rng.range(0, 16));
    for (let i = 0; i < count; i++) {
      const angle = rng.range(0, Math.PI * 2);
      const r = spread * Math.sqrt(rng.next());
      stone(cx + Math.cos(angle) * r, cz + Math.sin(angle) * r, rng.range(0.1, 0.3));
    }
  }
  for (let i = 0; i < 240; i++) {
    stone(rng.range(-57, 57), rng.range(-57, 57), rng.range(0.12, 0.42));
  }
  // A few flat spalls knocked off the rock islands by earlier shelling.
  for (let i = 0; i < 30; i++) {
    stone(rng.range(-57, 57), rng.range(-57, 57), rng.range(0.45, 0.8));
  }
  gravel.add(sandstoneRubble(stones));
  // Dry scrub holds the undisturbed shoulders, clear of traffic, machinery and
  // the combat lanes: straw and sage grass tufts plus low rounded saltbush.
  const scrub: number[] = [];
  const tints: number[] = [];
  const sage = new THREE.Color(0x87866a);
  const straw = new THREE.Color(0xa99571);
  const tint = new THREE.Color();
  const vertex = new THREE.Vector3();
  for (let i = 0; i < 190; i++) {
    const side = i % 2 ? -1 : 1;
    const along = rng.range(-72, 72);
    const out = side * rng.range(64, 72.5);
    const [x, z] = i % 4 < 2 ? [along, out] : [out, along * 0.75];
    const y = -Math.min(1.8, (Math.max(Math.abs(x), Math.abs(z)) - 60) * 0.3) + 0.01;
    const blocked = SCRUB_KEEP_OUT.some(([x0, x1, z0, z1]) => x > x0 && x < x1 && z > z0 && z < z1);
    tint.lerpColors(sage, straw, rng.range(0, 1)).multiplyScalar(rng.range(0.8, 1.05));
    if (blocked) {
      continue;
    }
    if (i % 5 === 0) {
      // Saltbush: a squat, lumpy faceted clump, darker toward its underside.
      const radius = rng.range(0.35, 0.65);
      const bush = new THREE.IcosahedronGeometry(radius, 0);
      bush.rotateY(rng.range(0, Math.PI));
      const points = bush.getAttribute("position");
      // Shared corners move together so the clump stays closed.
      const lumps = Array.from({ length: 12 }, () => rng.range(0.78, 1.18));
      const corners: THREE.Vector3[] = [];
      for (let v = 0; v < points.count; v++) {
        vertex.fromBufferAttribute(points, v);
        let corner = corners.findIndex((c) => c.distanceToSquared(vertex) < 1e-6);
        if (corner < 0) {
          corner = corners.push(vertex.clone()) - 1;
        }
        vertex.multiplyScalar(lumps[corner % lumps.length]);
        const shade = 0.66 + 0.34 * Math.max(0, vertex.y / radius + 0.2);
        scrub.push(x + vertex.x, y + radius * 0.3 + vertex.y * 0.62, z + vertex.z);
        tints.push(tint.r * shade * 0.92, tint.g * shade, tint.b * shade * 0.94);
      }
      bush.dispose();
      continue;
    }
    for (let j = 0; j < 7; j++) {
      const angle = rng.range(0, Math.PI * 2);
      const dx = Math.cos(angle) * 0.05;
      const dz = Math.sin(angle) * 0.05;
      const lean = rng.range(0.15, 0.4);
      const tip = rng.range(0.22, 0.6);
      scrub.push(
        x - dz,
        y,
        z + dx,
        x + dz,
        y,
        z - dx,
        x + Math.cos(angle) * lean,
        y + tip,
        z + Math.sin(angle) * lean,
      );
      // Blades fade from shaded base to sunlit, straw-bleached tips.
      tints.push(
        tint.r * 0.6,
        tint.g * 0.6,
        tint.b * 0.6,
        tint.r * 0.6,
        tint.g * 0.6,
        tint.b * 0.6,
        tint.r * 1.15,
        tint.g * 1.1,
        tint.b,
      );
    }
  }
  const scrubGeometry = new THREE.BufferGeometry();
  scrubGeometry.setAttribute("position", new THREE.Float32BufferAttribute(scrub, 3));
  scrubGeometry.setAttribute("color", new THREE.Float32BufferAttribute(tints, 3));
  scrubGeometry.computeVertexNormals();
  equipment.add(
    new THREE.Mesh(
      scrubGeometry,
      new THREE.MeshStandardMaterial({
        roughness: 1,
        side: THREE.DoubleSide,
        vertexColors: true,
      }),
    ),
  );

  // Idle screening conveyor: rust-red chords, dusty truss and a faded feed hopper.
  for (const z of [-71.4, -68.6]) {
    beam(equipment, [-62, 0.3, z], [-42, 6.3, z], 0.24, 0x8a5136);
    beam(equipment, [-62, 1.6, z], [-42, 7.6, z], 0.14, 0xb08d46);
    for (let i = 0; i < 8; i++) {
      const x = -62 + i * 2.5;
      const y = 0.3 + i * 0.75;
      beam(equipment, [x, y, z], [x + 2.5, y + 2.05, z], 0.09, 0x6b5a48);
      beam(equipment, [x, y, z], [x, y + 1.3, z], 0.085, 0x6b5a48);
    }
  }
  // Muted teal drive motor and rust head drum mark the working head end.
  put(equipment, harborBox(1.2, 1.0, 1.1, 0x4e7d7c), -41.6, 6.9, -70);
  const drum = cylinder(0.5, 2.9, 0x8a4f2e, 12);
  drum.rotation.x = Math.PI / 2;
  put(equipment, drum, -42.1, 6.35, -70);
  const belt = box(21, 0.14, 2.5, 0x3d413b, 0);
  belt.rotation.z = Math.atan2(6, 20);
  put(equipment, belt, -52, 3.45, -70);
  for (let i = 0; i < 15; i++) {
    const roller = cylinder(0.18, 3.1, 0x5b625b, 8);
    roller.rotation.x = Math.PI / 2;
    put(equipment, roller, -62 + i * 1.4, 0.35 + i * 0.42, -70);
  }
  for (const z of [-71.3, -68.7]) {
    beam(equipment, [-46, -1.7, z], [-46, 5.4, z], 0.22, 0x68766e);
    beam(equipment, [-53, -1.7, z], [-46, 5.4, z], 0.17, 0x68766e);
  }
  put(equipment, harborBox(4.2, 2.1, 3.7, 0xb08d46), -63, 0.2, -70);
  put(equipment, box(3.7, 0.07, 3.2, 0x42483c, 0), -63, 1.29, -70);
  // Office access, air conditioner, water tank and stacked sawn blocks.
  put(equipment, harborBox(1.5, 2.7, 0.12, 0x515f59), 41.2, -0.3, -67.4);
  for (let i = 0; i < 3; i++) {
    put(equipment, harborBox(2.2, 0.22, 0.6, 0x8c9081), 41.2, -1.15 - i * 0.22, -66.9 + i * 0.6);
  }
  put(equipment, harborBox(1.7, 0.9, 0.65, 0xb3b0a0), 33.5, -0.6, -67.1);
  for (let i = 0; i < 5; i++) {
    put(equipment, box(1.4, 0.045, 0.04, 0x596058, 0), 33.5, -0.9 + i * 0.14, -66.75);
  }
  put(equipment, cylinder(1.55, 3.8, 0xa8aaa0, 20), 27, 0.1, -70);
  for (const y of [-1.2, 1.35]) {
    put(equipment, cylinder(1.6, 0.13, 0x7e4a2c, 20), 27, y, -70);
  }
  // Rust-skirted office, faded generator and a tight teal/rust drum cluster.
  put(equipment, harborBox(11.2, 0.5, 5.2, 0x7e4a2c), 37, -1.55, -70);
  put(equipment, harborBox(2.2, 1.4, 1.2, 0xb08d46), 30.5, -1.1, -66.6);
  put(equipment, harborBox(2.3, 0.18, 1.3, 0x4a4238), 30.5, -0.32, -66.6);
  const drums: [number, number, number][] = [
    [44.2, -65.2, 0x4e7d7c],
    [45.35, -66.5, 0x4e7d7c],
    [44.75, -65.75, 0x8a4f2e],
  ];
  for (const [x, z, color] of drums) {
    put(equipment, cylinder(0.55, 1.3, color, 12), x, -1.15, z);
  }
  for (let i = 0; i < 8; i++) {
    put(
      equipment,
      harborBox(3.2, 1.1, 2.2, 0xb9b09a),
      -50 + (i % 4) * 3.4,
      -1.2 + Math.floor(i / 4) * 1.15,
      69,
    );
  }
  siteSign(equipment);
  // Mobile lighting plants stand by for night shifts, giving the apron some height.
  lightTower(equipment, 57, -67.5, -0.4);
  lightTower(equipment, -67, 58, 2.2);
}

/** Towed mast light: trailer, outriggers, a tall mast and a lamp bar. */
function lightTower(group: THREE.Group, x: number, z: number, yaw: number) {
  const tower = new THREE.Group();
  put(tower, harborBox(2.6, 1.05, 1.35, 0xc69a4b), 0, 0.95, 0);
  put(tower, harborBox(2.7, 0.12, 1.45, 0x4a4238), 0, 0.42, 0);
  for (const side of [-1, 1]) {
    const wheel = cylinder(0.36, 0.26, 0x343431, 10);
    wheel.rotation.x = Math.PI / 2;
    put(tower, wheel, -0.3, 0.36, side * 0.8);
    beam(tower, [side * 1.2, 0.5, -0.6], [side * 1.55, 0, -1.1], 0.08, 0x5a564c);
    beam(tower, [side * 1.2, 0.5, 0.6], [side * 1.55, 0, 1.1], 0.08, 0x5a564c);
  }
  beam(tower, [1.3, 0.45, 0], [2.3, 0.35, 0], 0.1, 0x5a564c);
  put(tower, harborBox(0.2, 7.4, 0.2, 0x9a9a92), -0.9, 5.1, 0);
  put(tower, harborBox(0.12, 0.12, 2.1, 0x5a564c), -0.9, 8.8, 0);
  for (const dz of [-0.78, -0.26, 0.26, 0.78]) {
    const lamp = harborBox(0.34, 0.3, 0.42, 0x353e3c);
    lamp.rotation.z = -0.35;
    put(tower, lamp, -0.76, 8.62, dz);
    const lens = box(0.02, 0.24, 0.34, 0xe8e2c8, 0);
    lens.rotation.z = -0.35;
    put(tower, lens, -0.57, 8.55, dz);
  }
  tower.rotation.y = yaw;
  tower.position.set(x, -1.79, z);
  tower.updateMatrixWorld(true);
  for (const mesh of [...tower.children]) {
    mesh.applyMatrix4(tower.matrix);
    group.add(mesh);
  }
}
