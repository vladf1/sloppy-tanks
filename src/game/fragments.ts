import RAPIER from "@dimforge/rapier3d-compat";
import { GROUP } from "./data";
import type { Simulation } from "./simulation";
import type { Fragment } from "./types";

/** Bounded cosmetic physics debris. Quantized sizes reuse model geometry. */
export function createFragment(
  simulation: Simulation,
  x: number,
  z: number,
  color: number,
  size = 0.5,
  shape: NonNullable<Fragment["shape"]> = "shard",
  lifetimeScale = 1,
): void {
  size = Math.round(size * 5) / 5;
  simulation.reserveFragments(1);
  const body = simulation.world.createRigidBody(
    RAPIER.RigidBodyDesc.dynamic()
      .setTranslation(x, simulation.rng.range(1, 3), z)
      .setLinvel(
        simulation.rng.range(-7, 7),
        simulation.rng.range(5, 14),
        simulation.rng.range(-7, 7),
      )
      .setAngvel({
        x: simulation.rng.range(-6, 6),
        y: simulation.rng.range(-6, 6),
        z: simulation.rng.range(-6, 6),
      }),
  );
  simulation.world.createCollider(
    RAPIER.ColliderDesc.cuboid(
      size / 2,
      size * (shape === "armor" || shape === "track" || shape === "wood" ? 0.12 : 0.4),
      size / 2,
    )
      .setCollisionGroups(GROUP.fragment)
      .setRestitution(0.25)
      .setMass(0.1),
    body,
  );
  simulation.fragments.push({
    id: simulation.nextId++,
    body,
    life: simulation.rng.range(1.6, 2.6) * lifetimeScale,
    shape,
    size,
    color,
  });
}
