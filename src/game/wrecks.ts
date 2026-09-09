import RAPIER from "@dimforge/rapier3d-compat";
import { ARENA, GROUP, VEHICLES } from "./data";
import type { Simulation } from "./simulation";
import { GRAVITY } from "./simulation-rules";
import type { Tank, Vec2, WreckPart } from "./types";

/** Cosmetic bodies use the same gravity as combat, but cannot hit living tanks. */
export function breakTank(simulation: Simulation, tank: Tank): void {
  const origin = { ...tank.body.translation() };
  const scale = VEHICLES[tank.kind].scale;
  const detached = simulation.rng.next() < 0.4;
  const pieces: WreckPart[] = detached ? ["hull", "turret", "barrel"] : ["hull", "turret-barrel"];
  // Explosion travel is in world units, independent of visual model scale.
  const halfSeparation = simulation.rng.range(7, 14);
  const angle = simulation.rng.range(-0.45, 0.45);
  const high = simulation.rng.next() < 0.25;
  const view = simulation.wreckView;
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
  const clamp = (n: number, low: number, high: number) => Math.max(low, Math.min(high, n));
  const minX = Math.max(-ARENA + 3, bounds.minX);
  const maxX = Math.min(ARENA - 3, bounds.maxX);
  const minZ = Math.max(-ARENA + 3, bounds.minZ);
  const maxZ = Math.min(ARENA - 3, bounds.maxZ);
  // Shift the landing pair inward together at arena/view edges, preserving separation.
  const center = {
    x: clamp(
      origin.x,
      minX + Math.min(halfSeparation, (maxX - minX) / 2),
      maxX - Math.min(halfSeparation, (maxX - minX) / 2),
    ),
    z: clamp(origin.z, minZ + 2, maxZ - 2),
  };
  simulation.world.removeRigidBody(tank.body);
  for (const [index, part] of pieces.entries()) {
    simulation.reserveFragments(1);
    const side = index === 0 ? -1 : 1;
    let landing: Vec2 = {
      x: clamp(center.x + side * halfSeparation * Math.cos(angle), minX, maxX),
      z: clamp(
        center.z + side * halfSeparation * Math.sin(angle) + (index === 2 ? -5 : 0),
        minZ,
        maxZ,
      ),
    };
    const blocked = (position: Vec2) =>
      simulation.covers.some(
        (cover) =>
          cover.alive &&
          cover.kind !== "boundary" &&
          Math.abs(position.x - cover.x) < cover.w / 2 + 1.5 &&
          Math.abs(position.z - cover.z) < cover.d / 2 + 1.5,
      );
    for (let attempt = 0; blocked(landing) && attempt < 12; attempt++) {
      landing = {
        x: clamp(landing.x + simulation.rng.range(-3, 3), minX, maxX),
        z: clamp(landing.z + simulation.rng.range(-3, 3), minZ, maxZ),
      };
    }
    const y = origin.y + (part === "hull" ? 0 : 0.75 * scale);
    const peak =
      high && part !== "hull" ? simulation.rng.range(20, 30) : simulation.rng.range(4.5, 8);
    const vy = Math.sqrt(2 * GRAVITY * peak);
    const flight = (vy + Math.sqrt(vy * vy + 2 * GRAVITY * Math.max(0, y - 0.3))) / GRAVITY;
    // Uniform directions on a sphere give cartwheels and barrel rolls as often
    // as yaw spins, with an independent axis and spin speed for every piece.
    const axisY = simulation.rng.range(-1, 1);
    const azimuth = simulation.rng.range(0, Math.PI * 2);
    const radius = Math.sqrt(1 - axisY * axisY);
    const spin = simulation.rng.range(7, 14);
    const body = simulation.world.createRigidBody(
      RAPIER.RigidBodyDesc.dynamic()
        .setTranslation(origin.x, y, origin.z)
        .setLinvel((landing.x - origin.x) / flight, vy, (landing.z - origin.z) / flight)
        .setAngvel({
          x: radius * Math.cos(azimuth) * spin,
          y: axisY * spin,
          z: radius * Math.sin(azimuth) * spin,
        })
        .setCcdEnabled(true),
    );
    const yaw = part === "hull" ? tank.heading : tank.aim;
    body.setRotation({ x: 0, y: Math.sin(yaw / 2), z: 0, w: Math.cos(yaw / 2) }, true);
    const size =
      part === "hull"
        ? [1.22, 0.42, 1.45]
        : part === "barrel"
          ? [0.2, 0.2, 0.95]
          : [0.95, 0.45, part === "turret-barrel" ? 1.55 : 1];
    simulation.world.createCollider(
      RAPIER.ColliderDesc.cuboid(
        size[0] * scale,
        size[1] * scale,
        size[2] * scale * (part === "hull" && tank.kind === "heavy" ? 1.18 : 1),
      )
        .setCollisionGroups(GROUP.fragment)
        .setMass(part === "hull" ? 1.2 : 0.5)
        .setFriction(0.95)
        .setRestitution(0.12),
      body,
    );
    simulation.fragments.push({
      id: simulation.nextId++,
      body,
      life: flight + 1.8,
      size: 1,
      color: 0x46534c,
      wreck: tank.kind,
      team: tank.team,
      part,
      cleanup: simulation.rng.next() < 0.5 ? "shrink" : "fade",
    });
  }
}
