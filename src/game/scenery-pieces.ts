import RAPIER from "@dimforge/rapier3d-compat";
import { Quaternion, Vector3 } from "three";
import { GROUP, Random } from "./data";
import { DEBRIS_CLEANUP_SECONDS } from "./debris-cleanup";
import { DEBRIS_MATERIALS, trackDebrisContacts, type DebrisMaterial } from "./debris-physics";
import type { Simulation } from "./simulation";
import { TOWER_BASE } from "./tower-layout";
import type { Cover, Fragment } from "./types";
import { treeProportions } from "./tree-proportions";

/** Authored major components only. Dust, foliage and chips remain presentation particles.
 * Dimensions are shared by simple colliders and instanced unit geometry. */
export function breakScenery(
  sim: Simulation,
  cover: Cover,
  pose?: { position: RAPIER.Vector; rotation: RAPIER.Rotation },
): boolean {
  // Navigation bounds expand as a barrel tips; fragments keep its original dimensions.
  if (cover.motion) {
    cover = { ...cover, w: cover.motion.w, d: cover.motion.d };
  }
  const rotation =
    pose && new Quaternion(pose.rotation.x, pose.rotation.y, pose.rotation.z, pose.rotation.w);
  const pieces: Fragment[] = [];
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
    const round = shape === "log";
    const collider = sim.world.createCollider(
      (round
        ? RAPIER.ColliderDesc.cylinder(h / 2, w / 2)
        : RAPIER.ColliderDesc.cuboid(w / 2, h / 2, d / 2)
      )
        .setCollisionGroups(GROUP.pushableDebris)
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
    const fragment: Fragment = {
      id,
      body,
      shape,
      color,
      size: 1,
      dimensions: { x: w, y: h, z: d },
      material,
      sourceKind: cover.kind,
      life: 9.5 + DEBRIS_CLEANUP_SECONDS,
      expiresAt: sim.elapsed + 18,
    };
    sim.fragments.push(fragment);
    pieces.push(fragment);
    return fragment;
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
    const { family, height, radius, stumpHeight: stump } = treeProportions(cover);
    const length = height * (family < 3 ? 0.98 : 0.78) - stump;
    const center = stump + length / 2;
    const trunk = piece("log", 0, center, 0, radius * 2, length, radius * 2, 0x98734f);
    trunk.treeCoverId = cover.id;
    trunk.treeCenterY = center;
    // A light crown volume keeps foliage above the ground as the trunk rolls.
    sim.world.createCollider(
      RAPIER.ColliderDesc.ball(Math.min(cover.w, cover.d) * 0.34)
        .setTranslation(0, height * 0.72 - center, 0)
        .setCollisionGroups(GROUP.fragment)
        .setMass(0.12)
        .setFriction(0.9)
        .setRestitution(0.05),
      trunk.body,
    );
    // A small sideways lean initiates a gravity-driven fall instead of a launch.
    const angle = rng.range(0, Math.PI * 2);
    trunk.body.setLinvel({ x: Math.sin(angle) * 0.45, y: 0, z: Math.cos(angle) * 0.45 }, true);
    trunk.body.setAngvel({ x: Math.cos(angle) * 0.65, y: 0, z: -Math.sin(angle) * 0.65 }, true);
    piece("beam", radius, stump, 0, radius * 0.3, radius * 1.4, radius * 0.25, 0xb59a69);
  } else if (cover.kind === "drum") {
    // Internal pressure tears the thin wall into small curled sheets, not a
    // surviving cylinder. Offset each sheet so the blast spreads them radially.
    const phase = rng.range(0, Math.PI * 2);
    for (let i = 0; i < 3; i++) {
      const angle = phase + (i * Math.PI * 2) / 3;
      const scrap = piece(
        "drum-shell",
        Math.cos(angle) * cover.w * 0.3,
        cover.h * rng.range(0.3, 0.6),
        Math.sin(angle) * cover.d * 0.3,
        cover.w * rng.range(0.28, 0.4),
        cover.h * rng.range(0.25, 0.4),
        0.12,
        i === 1 ? 0x493e35 : 0x765443,
        "metal",
      );
      scrap.body.setRotation(
        { x: 0, y: Math.sin(-angle / 2), z: 0, w: Math.cos(-angle / 2) },
        true,
      );
    }
    piece("drum-lid", 0, cover.h, 0, cover.w * 0.65, 0.16, cover.w * 0.65, 0x574e3e, "metal");
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
  if (pose && rotation) {
    // Carry the authored breakup into the barrel's current world pose before the blast.
    for (const fragment of pieces) {
      const body = fragment.body;
      const p = body.translation();
      body.setTranslation(
        new Vector3(p.x - cover.x, p.y - cover.h / 2, p.z - cover.z)
          .applyQuaternion(rotation)
          .add(new Vector3(pose.position.x, pose.position.y, pose.position.z)),
        true,
      );
      const q = body.rotation();
      body.setRotation(rotation.clone().multiply(new Quaternion(q.x, q.y, q.z, q.w)), true);
      const velocity = body.linvel();
      const spin = body.angvel();
      body.setLinvel(
        new Vector3(velocity.x, velocity.y, velocity.z).applyQuaternion(rotation),
        true,
      );
      body.setAngvel(new Vector3(spin.x, spin.y, spin.z).applyQuaternion(rotation), true);
    }
  }
  return true;
}
