import type { Simulation } from "../game/simulation";
import type { SimEvent } from "../game/types";

export const WIRE_PRECISION = { position: 1000, rotation: 10000, value: 100 } as const;
function rounded(value: number, precision: number = WIRE_PRECISION.position): number {
  if (!Number.isFinite(value)) {
    throw new Error("Non-finite wire value");
  }
  return Math.round(value * precision) / precision;
}
const value = (n: number) => rounded(n, WIRE_PRECISION.value);
const angle = (n: number) => rounded(n, WIRE_PRECISION.rotation);
const vector = (p: { x: number; y: number; z: number }) => ({
  x: rounded(p.x),
  y: rounded(p.y),
  z: rounded(p.z),
});
const quaternion = (q: { x: number; y: number; z: number; w: number }) => ({
  x: angle(q.x),
  y: angle(q.y),
  z: angle(q.z),
  w: angle(q.w),
});

/** Explicit projection for the M1 workload. No physics handles enter the protocol. */
export function experimentState(simulation: Simulation) {
  return {
    elapsed: value(simulation.elapsed),
    match: {
      phase: simulation.match.phase,
      time: value(simulation.match.time),
      scores: [...simulation.match.scores],
      winner: simulation.match.winner,
    },
    tanks: simulation.tanks.map((tank) => ({
      id: tank.id,
      life: tank.life,
      name: tank.name,
      team: tank.team,
      kind: tank.kind,
      alive: tank.alive,
      position: tank.alive
        ? vector(tank.body.translation())
        : vector({ ...tank.previous, y: 0.65 }),
      velocity: tank.alive ? vector(tank.body.linvel()) : { x: 0, y: 0, z: 0 },
      heading: angle(tank.heading),
      aim: angle(tank.aim),
      hp: value(tank.hp),
      maxHp: value(simulation.maxHealth(tank)),
      shield: value(tank.shield),
      shieldPoints: value(tank.shieldPoints),
      protection: value(tank.protection),
      respawn: value(tank.respawn),
      cooldown: value(tank.cooldown),
      mineCooldown: value(tank.mineCooldown),
      rapid: value(tank.rapid),
      speed: value(tank.speed),
      laser: value(tank.laser),
      recoil: value(tank.recoil),
      xp: value(tank.xp),
      selectedAmmo: tank.selectedAmmo,
      ammo: {
        spread: tank.ammo.spread,
        rocket: tank.ammo.rocket,
        ricochet: tank.ammo.ricochet,
        piercing: tank.ammo.piercing,
      },
      kills: tank.kills,
      deaths: tank.deaths,
    })),
    covers: simulation.covers.map((cover) => ({
      id: cover.id,
      kind: cover.kind,
      x: rounded(cover.x),
      z: rounded(cover.z),
      w: rounded(cover.w),
      h: rounded(cover.h),
      d: rounded(cover.d),
      alive: cover.alive,
      destructible: cover.destructible,
      indestructible: !Number.isFinite(cover.maxHp),
      hp: Number.isFinite(cover.hp) ? value(cover.hp) : null,
      maxHp: Number.isFinite(cover.maxHp) ? value(cover.maxHp) : null,
      color: cover.color,
      debrisSeed: cover.debrisSeed,
      timberHits: cover.timberHits,
      timberJoin: cover.timberJoin,
      motion: cover.motion && {
        originX: rounded(cover.motion.originX),
        originZ: rounded(cover.motion.originZ),
        w: rounded(cover.motion.w),
        d: rounded(cover.motion.d),
      },
      position: cover.body.isValid() ? vector(cover.body.translation()) : null,
      rotation: cover.body.isValid() ? quaternion(cover.body.rotation()) : null,
      sleeping: cover.body.isValid() ? cover.body.isSleeping() : true,
    })),
    shots: simulation.shots.map((shot) => ({
      id: shot.id,
      owner: shot.owner,
      ownerLife: shot.ownerLife,
      team: shot.team,
      weapon: shot.weapon,
      x: rounded(shot.x),
      z: rounded(shot.z),
      y: shot.y === undefined ? undefined : rounded(shot.y),
      visualY: shot.visualY === undefined ? undefined : rounded(shot.visualY),
      vx: rounded(shot.vx),
      vz: rounded(shot.vz),
      life: value(shot.life),
    })),
    mines: simulation.mines.map((mine) => ({
      id: mine.id,
      owner: mine.owner,
      ownerLife: mine.ownerLife,
      team: mine.team,
      x: rounded(mine.x),
      z: rounded(mine.z),
      arm: value(mine.arm),
      life: value(mine.life),
    })),
    pickups: simulation.pickups.map((pickup) => ({
      id: pickup.id,
      kind: pickup.kind,
      x: rounded(pickup.x),
      z: rounded(pickup.z),
      available: pickup.available,
      cooldown: value(pickup.cooldown),
      cooldownDuration:
        pickup.cooldownDuration === undefined ? undefined : value(pickup.cooldownDuration),
    })),
    fragments: simulation.fragments.map((fragment) => ({
      id: fragment.id,
      position: vector(fragment.body.translation()),
      rotation: quaternion(fragment.body.rotation()),
      sleeping: fragment.body.isSleeping(),
      life: value(fragment.life),
      size: rounded(fragment.size),
      color: fragment.color,
      shape: fragment.shape,
      dimensions: fragment.dimensions && vector(fragment.dimensions),
      material: fragment.material,
      sourceKind: fragment.sourceKind,
      timberPart: fragment.timberPart,
      treeCoverId: fragment.treeCoverId,
      treeCenterY: fragment.treeCenterY,
      createdAt: fragment.createdAt === undefined ? undefined : value(fragment.createdAt),
      expiresAt: fragment.expiresAt === undefined ? undefined : value(fragment.expiresAt),
      wreck: fragment.wreck,
      part: fragment.part,
      team: fragment.team,
    })),
  };
}
export type ExperimentState = ReturnType<typeof experimentState>;
export const ENTITY_TYPES = ["tanks", "covers", "shots", "mines", "pickups", "fragments"] as const;
export type EntityType = (typeof ENTITY_TYPES)[number];

export function experimentEvent(event: SimEvent, tick: number, id: number) {
  return {
    eventId: id,
    tick,
    type: event.type,
    x: rounded(event.x),
    z: rounded(event.z),
    id: event.id,
    owner: event.owner,
    weapon: event.weapon,
    team: event.team,
    size: event.size,
    label: event.label,
    color: event.color,
    height: event.height,
    deathStyle: event.deathStyle,
    material: event.material,
    force: event.force,
    coverKind: event.coverKind,
    from: event.from && vector(event.from),
    damageSource: event.damageSource && {
      cause: event.damageSource.cause,
      origin: { x: rounded(event.damageSource.origin.x), z: rounded(event.damageSource.origin.z) },
    },
  };
}

/** A single global baseline; obtaining a full state for a join does not advance it. */
export class ExperimentStream {
  private previous = new Map<string, string>();
  seq = 0;
  full(state: ExperimentState, tick: number, eventCursor: number) {
    return { type: "full" as const, seq: this.seq, tick, eventCursor, state };
  }
  snapshot(state: ExperimentState, tick: number, events: ReturnType<typeof experimentEvent>[]) {
    const next = new Map<string, string>();
    const updates: { kind: EntityType; entity: ExperimentState[EntityType][number] }[] = [];
    for (const kind of ENTITY_TYPES) {
      for (const entity of state[kind]) {
        const key = `${kind}:${entity.id}`;
        const encoded = JSON.stringify(entity);
        next.set(key, encoded);
        if (this.previous.get(key) !== encoded) {
          updates.push({ kind, entity });
        }
      }
    }
    const removed = [...this.previous.keys()].filter((key) => !next.has(key));
    this.previous = next;
    return {
      type: "snap" as const,
      seq: ++this.seq,
      tick,
      elapsed: state.elapsed,
      match: state.match,
      updates,
      removed,
      events,
    };
  }
}
