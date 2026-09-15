import RAPIER from "@dimforge/rapier3d-compat";
import { GROUP, Random } from "./data";
import { DEBRIS_MATERIALS, trackDebrisContacts, type DebrisMaterial } from "./debris-physics";
import type { Simulation } from "./simulation";
import { TOWER_BASE } from "./tower-layout";
import type { Cover, Fragment } from "./types";

/** Authored major components only. Dust, foliage and chips remain presentation particles.
 * Dimensions are shared by simple colliders and instanced unit geometry. */
export function breakScenery(sim: Simulation, cover: Cover): boolean {
  const legacyCount =
    cover.kind === "tower"
      ? 10
      : cover.kind === "tree"
        ? 9
        : cover.kind === "timber"
          ? 7
          : cover.kind === "cargo" || cover.kind === "drum"
            ? 3
            : 0;
  if (legacyCount === 0) {
    return false;
  }
  // The old fragment path consumed 3 placement/size draws and 8 body/lifetime
  // draws per piece. Preserve that stream so cosmetic authoring cannot reshuffle
  // seeded combat and bot decisions. New piece motion uses its own stream below.
  for (let i = 0; i < legacyCount * 11; i++) {
    sim.rng.next();
  }
  const rng = new Random(cover.id * 73856093 + sim.seed);
  const piece = (
    shape: NonNullable<Fragment["shape"]>,
    x: number,
    y: number,
    z: number,
    w: number,
    h: number,
    d: number,
    color = cover.color,
    material: DebrisMaterial = "wood",
  ) => {
    sim.reserveFragments(1);
    const id = sim.nextId++;
    const body = sim.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(cover.x + x, y, cover.z + z)
        .setLinearDamping(0.12)
        .setAngularDamping(0.25)
        .setCanSleep(true),
    );
    const round = shape === "log" || shape === "drum-shell";
    const collider = sim.world.createCollider(
      (round
        ? RAPIER.ColliderDesc.cylinder(h / 2, w / 2)
        : RAPIER.ColliderDesc.cuboid(w / 2, h / 2, d / 2)
      )
        .setCollisionGroups(GROUP.fragment)
        .setMass(Math.max(0.18, w * h * d * (material === "metal" ? 0.65 : 0.35)))
        .setFriction(DEBRIS_MATERIALS[material].friction)
        .setRestitution(DEBRIS_MATERIALS[material].restitution),
      body,
    );
    const angle = rng.range(0, Math.PI * 2);
    const outward = Math.hypot(x, z);
    const nx = outward > 0.1 ? x / outward : Math.cos(angle);
    const nz = outward > 0.1 ? z / outward : Math.sin(angle);
    const speed = rng.range(2, 5);
    const mass = body.mass();
    body.applyImpulseAtPoint(
      { x: nx * speed * mass, y: rng.range(3, 7) * mass, z: nz * speed * mass },
      { x: cover.x + x + w * 0.15, y: y - h * 0.2, z: cover.z + z + d * 0.15 },
      true,
    );
    trackDebrisContacts(body, collider, id, material);
    sim.fragments.push({
      id,
      body,
      shape,
      color,
      size: 1,
      dimensions: { x: w, y: h, z: d },
      material,
      sourceKind: cover.kind,
      life: 10,
      expiresAt: sim.elapsed + 18,
    });
  };
  if (cover.kind === "cargo") {
    // Two broad crate sides, a lid, and one broken frame beam (four bodies).
    for (const side of [-1, 1]) {
      piece("panel", 0, cover.h / 2, (side * cover.d) / 2, cover.w, cover.h - 0.22, 0.12);
    }
    piece("panel", 0, cover.h, 0, cover.w, 0.12, cover.d);
    piece("beam", -cover.w * 0.34, 0.2, 0, 0.25, 0.22, cover.d, 0x805336);
  } else if (cover.kind === "timber") {
    const along = cover.w > cover.d;
    for (const row of [0, 3, 6]) {
      piece(
        "beam",
        0,
        0.2 + row * 0.39,
        0,
        along ? cover.w - 0.04 : 0.64,
        0.38,
        along ? 0.64 : cover.d - 0.04,
        row === 3 ? 0x94613e : cover.color,
      );
    }
  } else if (cover.kind === "tree") {
    const radius = Math.max(0.22, Math.min(0.48, cover.w * 0.14));
    const length = Math.min(4.5, cover.h * 0.62);
    piece("log", 0, 0.65 + length / 2, 0, radius * 2, length, radius * 2, 0x98734f);
    piece("beam", radius, length * 0.8, 0, radius * 0.65, length * 0.45, radius * 0.65, 0x825333);
  } else if (cover.kind === "drum") {
    piece(
      "drum-shell",
      0,
      cover.h * 0.4,
      0,
      cover.w * 0.9,
      cover.h * 0.7,
      cover.w * 0.9,
      cover.color,
      "metal",
    );
    piece("drum-lid", 0, cover.h, 0, cover.w * 0.95, 0.08, cover.w * 0.95, 0x574e3e, "metal");
  } else if (cover.kind === "tower") {
    // Split deck and two structural posts; foundations still use the existing rubble.
    for (const side of [-1, 1]) {
      piece("panel", side * 1.5, 5, 0, 2.9, 0.35, 5, 0x887d59);
      piece(
        "beam",
        side * TOWER_BASE.offset,
        TOWER_BASE.height + 2.15,
        -TOWER_BASE.postZ,
        0.35,
        4.3,
        0.35,
        0x887454,
      );
    }
  } else {
    return false;
  }
  return true;
}
