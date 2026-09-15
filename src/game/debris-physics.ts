import RAPIER from "@dimforge/rapier3d-compat";
import type { Simulation } from "./simulation";
import type { Cover, Shot, Vec2 } from "./types";

export type DebrisMaterial = "wood" | "metal" | "concrete";
export const DEBRIS_MATERIALS = {
  wood: { friction: 0.65, restitution: 0.16 },
  metal: { friction: 0.95, restitution: 0.12 },
  concrete: { friction: 1.15, restitution: 0.02 },
} as const;

/** Metadata lives on substantial bodies, allowing contacts to feed presentation later. */
export function trackDebrisContacts(
  body: RAPIER.RigidBody,
  collider: RAPIER.Collider,
  id: number,
  material: DebrisMaterial,
): void {
  body.userData = { id, material, impactAfter: 0 };
  collider.setActiveEvents(RAPIER.ActiveEvents.CONTACT_FORCE_EVENTS);
  // Ignore resting weight. This threshold scales with the body's mass.
  collider.setContactForceEventThreshold(body.mass() * 65);
}

/** Fixed-size fragment pool + a short movable-cover list; no all-pairs debris work.
 * Sleeping bodies are deliberately included, and this never consumes combat RNG. */
export function blastDebris(sim: Simulation, origin: Vec2, radius: number, power: number): void {
  for (const fragment of sim.fragments) {
    if (fragment.life <= 0.5) {
      continue;
    }
    const lever = fragment.wreck
      ? 0.35
      : fragment.dimensions
        ? Math.min(
            0.4,
            Math.max(fragment.dimensions.x, fragment.dimensions.y, fragment.dimensions.z) * 0.2,
          )
        : fragment.size * 0.25;
    if (blastBody(fragment.body, origin, radius, power, false, lever)) {
      // Let a second launch finish, but never extend life beyond the original deadline.
      if (fragment.expiresAt !== undefined) {
        fragment.life = Math.min(Math.max(fragment.life, 5), fragment.expiresAt - sim.elapsed);
      }
    }
  }
  for (const cover of sim.movableCovers) {
    if (cover.alive) {
      blastBody(cover.body, origin, radius, power, true, 0.35);
    }
  }
}

function blastBody(
  body: RAPIER.RigidBody,
  origin: Vec2,
  radius: number,
  power: number,
  heavy: boolean,
  lever: number,
): boolean {
  if (radius <= 0 || power <= 0) {
    return false;
  }
  const p = body.worldCom();
  const dx = p.x - origin.x;
  const dz = p.z - origin.z;
  const dy = Math.max(0, p.y - 0.75);
  const distance = Math.hypot(dx, dy, dz);
  if (distance >= radius) {
    return false;
  }
  const horizontal = Math.hypot(dx, dz);
  const nx = horizontal > 0.001 ? dx / horizontal : 1;
  const nz = horizontal > 0.001 ? dz / horizontal : 0;
  const falloff = (1 - distance / radius) ** 2;
  const strength = Math.min(1.8, power / 60) * falloff;
  // Cap per-blast velocity change for tiny chips; heavy barriers respond to force/mass.
  const impulse = heavy ? 100 * strength : body.mass() * 22 * strength;
  body.applyImpulseAtPoint(
    { x: nx * impulse, y: impulse * (heavy ? 0.65 : 0.85), z: nz * impulse },
    // Pressure catches a facing edge above the centre, producing real pitch and roll.
    {
      x: p.x - nx * lever + nz * lever * 0.5,
      y: p.y + lever * 0.5,
      z: p.z - nz * lever - nx * lever * 0.5,
    },
    true,
  );
  return true;
}

export function hitMovableCover(cover: Cover, shot: Shot): void {
  if (cover.kind !== "teeth" || !cover.motion) {
    return;
  }
  const speed = Math.hypot(shot.vx, shot.vz);
  if (speed === 0) {
    return;
  }
  const impulse = shot.weapon === "rocket" ? 40 : shot.weapon === "piercing" ? 32 : 24;
  cover.body.applyImpulseAtPoint(
    { x: (shot.vx / speed) * impulse, y: 0, z: (shot.vz / speed) * impulse },
    // Projectile collision queries run at y=1, independently of the rendered muzzle.
    { x: shot.x, y: 1, z: shot.z },
    true,
  );
}

/** Real Rapier contact forces, bounded and rate-limited per body. No effect allocation
 * for sleeping contacts; presentation may map material/force to dust or sound. */
export function drainDebrisContacts(sim: Simulation): void {
  let count = 0;
  sim.contactEvents.drainContactForceEvents((event) => {
    if (count >= 8) {
      return;
    }
    for (const handle of [event.collider1(), event.collider2()]) {
      const body = sim.world.getCollider(handle)?.parent();
      const data = body?.userData as
        { id: number; material: DebrisMaterial; impactAfter: number } | undefined;
      if (!body || !data || sim.elapsed < data.impactAfter) {
        continue;
      }
      data.impactAfter = sim.elapsed + 0.3;
      const p = body.translation();
      sim.events.push({
        type: "debris-impact",
        id: data.id,
        x: p.x,
        z: p.z,
        height: p.y,
        material: data.material,
        force: event.totalForceMagnitude(),
      });
      count++;
      break;
    }
  });
}
