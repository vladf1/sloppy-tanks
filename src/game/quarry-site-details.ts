import * as THREE from "three";
import { harborBox } from "./harbor-surfaces";
import { Random } from "./math";
import { box, cylinder, put } from "./model-primitives";
import { sandstoneRock } from "./quarry-surfaces";

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

/** All tall dressing is outside the playable wall; in-arena chips are only 3–7cm high. */
export function quarrySiteDetails(equipment: THREE.Group, geology: THREE.Group): void {
  const rng = new Random(62541);
  // Scree gathers against the cut, with occasional flat spalls across the work floor.
  for (let i = 0; i < 700; i++) {
    const outer = i < 500;
    const side = i % 2 ? -1 : 1;
    const x = outer ? rng.range(-74, 74) : rng.range(-57, 57);
    const z = outer ? side * rng.range(71, 77) : rng.range(-57, 57);
    const scale = [0.23, 0.4, 0.65, 1.1][i % 4];
    const rock = sandstoneRock(
      scale,
      outer ? scale * 0.42 : 0.03 + (i % 3) * 0.02,
      scale * 0.8,
      i % 7,
    );
    rock.rotation.y = rng.range(-Math.PI, Math.PI);
    put(geology, rock, x, outer ? -1.75 : 0.008, z);
  }
  // Low scrub occupies undisturbed shoulders, away from the traffic and combat lanes.
  const grassMaterial = new THREE.MeshStandardMaterial({
    color: 0x797b50,
    roughness: 1,
    side: THREE.DoubleSide,
  });
  const blades: number[] = [];
  for (let i = 0; i < 140; i++) {
    const x = rng.range(-72, 72);
    const z = (i % 2 ? -1 : 1) * rng.range(63.5, 75);
    const y = -Math.min(1.8, (Math.abs(z) - 60) * 0.3) + 0.02;
    for (let j = 0; j < 6; j++) {
      const angle = rng.range(0, Math.PI * 2);
      const dx = Math.cos(angle) * 0.28;
      const dz = Math.sin(angle) * 0.28;
      blades.push(
        x - dx,
        y,
        z - dz,
        x + dx,
        y,
        z + dz,
        x + dx * 1.5,
        y + rng.range(0.25, 0.7),
        z + dz * 1.5,
      );
    }
  }
  const grassGeometry = new THREE.BufferGeometry();
  grassGeometry.setAttribute("position", new THREE.Float32BufferAttribute(blades, 3));
  grassGeometry.computeVertexNormals();
  equipment.add(new THREE.Mesh(grassGeometry, grassMaterial));

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
  for (let i = 0; i < 9; i++) {
    put(
      geology,
      sandstoneRock(3.5, 1.4 + (i % 3) * 0.4, 3, i),
      -42 + (i % 3) * 1.8,
      -1.75 + Math.floor(i / 3) * 0.3,
      -67 - Math.floor(i / 3) * 1.3,
    );
  }
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
}
