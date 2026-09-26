import type { Simulation } from "../src/game/simulation";
import type { Tank } from "../src/game/types";

/**
 * Empties the authored map so a test controls every obstacle: removes all
 * cover, pickups and every tank not in `keep`, and rebuilds navigation for the
 * open floor. Callers still place the kept tanks and step the world.
 */
export function clearArena<S extends Simulation>(s: S, keep: readonly Tank[] = []): S {
  for (const cover of s.covers) if (cover.body.isValid()) s.world.removeRigidBody(cover.body);
  for (const tank of s.tanks) if (!keep.includes(tank)) s.world.removeRigidBody(tank.body);
  s.covers = [];
  s.movableCovers = [];
  s.coverByCollider.clear();
  s.tanks = [...keep];
  s.pickups = [];
  s.nav.rebuild([]);
  return s;
}

/** Teleports a tank at rest, keeping interpolation and bot history on the new pose. */
export function placeTank(tank: Tank, x: number, z: number, heading = tank.heading): Tank {
  tank.heading = heading;
  tank.body.setTranslation({ x, y: 0.65, z }, true);
  tank.body.setRotation({ x: 0, y: Math.sin(heading / 2), z: 0, w: Math.cos(heading / 2) }, true);
  tank.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
  tank.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
  tank.previous = { x, z };
  tank.brain.last = { x, z };
  return tank;
}
