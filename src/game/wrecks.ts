import RAPIER from "@dimforge/rapier3d-compat";
import { ARENA, GROUP, VEHICLES } from "./data";
import type { Simulation } from "./simulation";
import type { Tank, Vec2, WreckPart } from "./types";

/** Cosmetic bodies use the same gravity as combat, but cannot hit living tanks. */
export function breakTank(s: Simulation, tank: Tank) {
  const origin = { ...tank.body.translation() };
  const scale = VEHICLES[tank.kind].scale;
  const detached = s.rng.next() < 0.4;
  const pieces: WreckPart[] = detached
    ? ["hull", "turret", "barrel"]
    : ["hull", "turret-barrel"];
  const halfSeparation = s.rng.range(7, 14) * scale;
  const angle = s.rng.range(-0.45, 0.45);
  const high = s.rng.next() < 0.35;
  const view = s.wreckView;
  // Only on-screen explosions use view bounds; off-screen combat stays local.
  const onScreen =
    view &&
    origin.x > view.minX &&
    origin.x < view.maxX &&
    origin.z > view.minZ &&
    origin.z < view.maxZ;
  const bounds = onScreen
    ? view
    : {
        minX: origin.x - 18,
        maxX: origin.x + 18,
        minZ: origin.z - 12,
        maxZ: origin.z + 12,
      };
  const clamp = (n: number, low: number, high: number) =>
    Math.max(low, Math.min(high, n));
  const minX = Math.max(-ARENA + 3, bounds.minX),
    maxX = Math.min(ARENA - 3, bounds.maxX);
  const minZ = Math.max(-ARENA + 3, bounds.minZ),
    maxZ = Math.min(ARENA - 3, bounds.maxZ);
  // Shift the landing pair inward together at arena/view edges, preserving separation.
  const center = {
    x: clamp(
      origin.x,
      minX + Math.min(halfSeparation, (maxX - minX) / 2),
      maxX - Math.min(halfSeparation, (maxX - minX) / 2),
    ),
    z: clamp(origin.z, minZ + 2, maxZ - 2),
  };
  s.world.removeRigidBody(tank.body);
  for (const [index, part] of pieces.entries()) {
    s.reserveFragments(1);
    const side = index === 0 ? -1 : 1;
    let landing: Vec2 = {
      x: clamp(center.x + side * halfSeparation * Math.cos(angle), minX, maxX),
      z: clamp(
        center.z +
          side * halfSeparation * Math.sin(angle) +
          (index === 2 ? -5 : 0),
        minZ,
        maxZ,
      ),
    };
    const blocked = (p: Vec2) =>
      s.covers.some(
        (c) =>
          c.alive &&
          c.kind !== "boundary" &&
          Math.abs(p.x - c.x) < c.w / 2 + 1.5 &&
          Math.abs(p.z - c.z) < c.d / 2 + 1.5,
      );
    for (let attempt = 0; blocked(landing) && attempt < 12; attempt++)
      landing = {
        x: clamp(landing.x + s.rng.range(-3, 3), minX, maxX),
        z: clamp(landing.z + s.rng.range(-3, 3), minZ, maxZ),
      };
    const y = origin.y + (part === "hull" ? 0 : 0.75 * scale);
    const peak =
      high && part !== "hull" ? s.rng.range(11, 15) : s.rng.range(4.5, 8);
    const vy = Math.sqrt(44 * peak),
      flight = (vy + Math.sqrt(vy * vy + 44 * Math.max(0, y - 0.3))) / 22;
    const body = s.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(origin.x, y, origin.z)
        .setLinvel(
          (landing.x - origin.x) / flight,
          vy,
          (landing.z - origin.z) / flight,
        )
        .setAngvel({
          x: s.rng.range(-5, 5),
          y: s.rng.range(5, 10) * side,
          z: s.rng.range(-5, 5),
        })
        .setCcdEnabled(true),
    );
    const yaw = part === "hull" ? tank.heading : tank.aim;
    body.setRotation(
      { x: 0, y: Math.sin(yaw / 2), z: 0, w: Math.cos(yaw / 2) },
      true,
    );
    const size =
      part === "hull"
        ? [1.22, 0.42, 1.45]
        : part === "barrel"
          ? [0.2, 0.2, 0.95]
          : [0.95, 0.45, part === "turret-barrel" ? 1.55 : 1];
    s.world.createCollider(
      RAPIER.ColliderDesc.cuboid(
        size[0] * scale,
        size[1] * scale,
        size[2] * scale,
      )
        .setCollisionGroups(GROUP.fragment)
        .setMass(part === "hull" ? 1.2 : 0.5)
        .setFriction(0.95)
        .setRestitution(0.12),
      body,
    );
    s.fragments.push({
      id: s.nextId++,
      body,
      life: flight + 1.8,
      size: 1,
      color: 0x46534c,
      wreck: tank.kind,
      team: tank.team,
      part,
      cleanup: s.rng.next() < 0.5 ? "shrink" : "fade",
    });
  }
}
