import {
  coverPosition,
  coverRotation,
  tankPosition,
  tankVelocity,
  type RenderState,
  type RenderTank,
  type RenderCover,
  type RenderFragment,
  type RenderPosition,
  type RenderRotation,
  type RenderShot,
} from "../game/render-state";
import type { Simulation } from "../game/simulation";
import { DEBRIS_CLEANUP_SECONDS } from "../game/debris-cleanup";
import { FRAGMENT_CAPACITY } from "../game/simulation-rules";
import type { TimberJoin, TimberPart } from "../game/timber-layout";
import type { Cover, Fragment, Match, Mine, Pickup, SimEvent, Tank } from "../game/types";
import { MAP_IDS } from "../game/map-options";
import {
  array,
  boolean,
  enumeration,
  id,
  nullable,
  number,
  object,
  optional,
  string,
  type Reader,
} from "./schema";

export const ENTITY_TYPES = ["tanks", "covers", "fragments", "shots", "mines", "pickups"] as const;
export type EntityType = (typeof ENTITY_TYPES)[number];
/** Clients interpolate between snapshots, so the server's previous physics pose stays local. */
export type WireTank = Omit<RenderTank, "previous">;
export type WireCover = Omit<RenderCover, "hp" | "maxHp"> & {
  hp: number | null;
  maxHp: number | null;
};
export interface Entities {
  tanks: WireTank[];
  covers: WireCover[];
  fragments: RenderFragment[];
  shots: RenderShot[];
  mines: Mine[];
  pickups: Pickup[];
}
export interface Scene {
  entities: Entities;
  elapsed: number;
  match: Match;
  map: {
    theme: RenderState["mapTheme"];
    floor?: RenderState["mapFloor"];
    outerFloor?: RenderState["mapOuterFloor"];
    outerFloorExtent?: number;
    /** Omitted for full-size maps. */
    scale?: number;
  };
}
const n = number();
const pos = object({ x: n, y: n, z: n });
const point = object({ x: n, z: n });
const rotation = object({ x: number(-1, 1), y: number(-1, 1), z: number(-1, 1), w: number(-1, 1) });
export const team = enumeration(0, 1);
export const playerKind = enumeration("scout", "balanced", "heavy");
const kind = enumeration("scout", "balanced", "heavy", "humvee");
const weapon = enumeration("standard", "spread", "rocket", "ricochet", "piercing", "tow");
const coverKind = enumeration(
  "rock",
  "teeth",
  "hedgehog",
  "container",
  "cargo",
  "house",
  "tree",
  "timber",
  "concrete",
  "drum",
  "tower",
  "rubble",
  "boundary",
);
const material = enumeration("wood", "metal", "concrete");
/** Every map, extra levels included; a room may play any of them. */
export const mapMode = enumeration(...MAP_IDS);
/** Standard rooms field 12 tanks; the extra levels field 30. */
const MAX_SCENE_TANKS = 32;
export const difficulty = enumeration("easy", "normal", "hard");
const mark = object({
  x: n,
  y: n,
  face: enumeration("front", "back", "left", "right"),
  size: n,
  seed: number(-2147483648, 4294967295, true),
});
const part = object({
  kind: enumeration("beam", "post"),
  index: id,
  x: n,
  y: n,
  z: n,
  w: n,
  h: n,
  d: n,
  yaw: n,
  lean: n,
  color: id,
  damage: n,
  damageSeed: number(-2147483648, 4294967295, true),
  marks: array(mark, 32),
});
export const tankReader = object<WireTank>({
  id,
  life: id,
  name: string(64),
  kind,
  team,
  human: boolean,
  alive: boolean,
  position: pos,
  velocity: pos,
  heading: n,
  aim: n,
  hp: n,
  maxHp: n,
  xp: n,
  shield: n,
  shieldPoints: n,
  protection: n,
  laser: n,
  recoil: n,
  cooldown: n,
  mineCooldown: n,
  respawn: n,
  rapid: n,
  speed: n,
  selectedAmmo: weapon,
  ammo: object({ spread: id, rocket: id, ricochet: id, piercing: id }),
  kills: id,
  deaths: id,
  lastCombat: n,
});
export const coverReader = object<WireCover>({
  id,
  kind: coverKind,
  x: n,
  z: n,
  w: n,
  h: n,
  d: n,
  hp: nullable(n),
  maxHp: nullable(n),
  alive: boolean,
  destructible: boolean,
  color: id,
  debrisSeed: optional(id),
  position: pos,
  rotation,
  timberHits: optional(array(object({ x: n, y: n, z: n, size: n }), 32)),
  timberJoin: optional(
    object({ openMin: optional(boolean), openMax: optional(boolean), post: optional(boolean) }),
  ),
  motion: optional(object({ originX: n, originZ: n, w: n, d: n })),
});
export const fragmentReader = object<RenderFragment>({
  id,
  life: n,
  size: n,
  color: id,
  position: pos,
  rotation,
  shape: optional(
    enumeration(
      "armor",
      "wheel",
      "track",
      "shard",
      "wood",
      "panel",
      "beam",
      "log",
      "drum-shell",
      "drum-lid",
    ),
  ),
  dimensions: optional(pos),
  material: optional(material),
  sourceKind: optional(coverKind),
  timberPart: optional(part),
  treeCoverId: optional(id),
  treeCenterY: optional(n),
  createdAt: optional(n),
  expiresAt: optional(n),
  wreck: optional(kind),
  part: optional(enumeration("intact", "hull", "turret", "turret-barrel", "barrel")),
  team: optional(team),
});
export const shotReader = object<RenderShot>({
  id,
  x: n,
  z: n,
  y: optional(n),
  visualY: optional(n),
  team,
  vx: n,
  vz: n,
  weapon,
});
export const mineReader = object<Mine>({
  id,
  x: n,
  z: n,
  owner: id,
  ownerLife: optional(id),
  damage: optional(n),
  team,
  arm: n,
  life: n,
});
export const pickupReader = object<Pickup>({
  id,
  x: n,
  z: n,
  kind: enumeration(
    "spread",
    "rocket",
    "ricochet",
    "piercing",
    "rapid",
    "shield",
    "speed",
    "repair",
    "laser",
  ),
  available: boolean,
  cooldown: n,
  cooldownDuration: optional(n),
});
export const entityReaders: { [K in EntityType]: Reader<Entities[K][number]> } = {
  tanks: tankReader,
  covers: coverReader,
  fragments: fragmentReader,
  shots: shotReader,
  mines: mineReader,
  pickups: pickupReader,
};
export const entitiesReader = object<Entities>({
  tanks: array(tankReader, MAX_SCENE_TANKS),
  covers: array(coverReader, 1024),
  fragments: array(fragmentReader, FRAGMENT_CAPACITY),
  shots: array(shotReader, 512),
  mines: array(mineReader, 256),
  pickups: array(pickupReader, 64),
});
export const matchReader = object<Match>({
  phase: enumeration("ready", "playing", "paused", "results"),
  time: n,
  scores: {
    read(value) {
      const scores = array(id, 2).read(value);
      if (scores.length !== 2) {
        throw new Error("Invalid scores");
      }
      return [scores[0], scores[1]];
    },
  },
  overtime: boolean,
  endedEarly: optional(boolean),
  winner: nullable(team),
  round: id,
});
const ground = enumeration("dry-grass", "packed-dirt");
export const sceneReader = object<Scene>({
  entities: entitiesReader,
  elapsed: n,
  match: matchReader,
  map: object({
    // A themed map's theme is its id, and an extra level's plain yard is named by its id.
    theme: mapMode,
    floor: optional(ground),
    outerFloor: optional(ground),
    outerFloorExtent: optional(n),
    scale: optional(number(0.1, 1)),
  }),
});
export const eventReader = object<SimEvent>({
  type: enumeration(
    "debris-impact",
    "notice",
    "shot",
    "impact",
    "explosion",
    "destroy",
    "death",
    "pickup",
    "respawn",
    "hurt",
    "ricochet",
    "laser",
    "promotion",
  ),
  x: n,
  z: n,
  id: optional(id),
  owner: optional(number(-1, Number.MAX_SAFE_INTEGER, true)),
  ownerLife: optional(id),
  weapon: optional(weapon),
  team: optional(team),
  size: optional(n),
  label: optional(string(160)),
  color: optional(id),
  from: optional(pos),
  deathStyle: optional(enumeration("burnout")),
  material: optional(material),
  force: optional(n),
  coverKind: optional(coverKind),
  height: optional(n),
  damageSource: optional(
    object({
      cause: enumeration(
        "standard",
        "spread",
        "rocket",
        "ricochet",
        "piercing",
        "tow",
        "mine",
        "drum",
        "interception",
        "explosion",
      ),
      origin: point,
    }),
  ),
});

const ANGLES = new Set(["aim", "heading", "yaw", "lean"]);
const VALUES = new Set([
  "hp",
  "maxHp",
  "xp",
  "shield",
  "shieldPoints",
  "protection",
  "laser",
  "recoil",
  "cooldown",
  "mineCooldown",
  "respawn",
  "rapid",
  "speed",
  "lastCombat",
  "life",
  "arm",
  "createdAt",
  "expiresAt",
  "cooldownDuration",
  "time",
]);
export const PRECISION = { position: 1000, rotation: 10000, value: 100 };
function wireNumber(value: number, scale: number): number {
  if (!Number.isFinite(value)) {
    throw new Error("Non-finite wire number");
  }
  return Math.round(value * scale) / scale || 0;
}
/** Rounds already-projected plain records (events, traces) by field name: rotation and
 * ANGLES fields to PRECISION.rotation, VALUES to PRECISION.value, others to PRECISION.position. */
export function rounded<T>(value: T): T {
  const visit = (item: unknown, key = "", parent = ""): unknown => {
    if (typeof item === "number") {
      return wireNumber(
        item,
        parent === "rotation" || ANGLES.has(key)
          ? PRECISION.rotation
          : VALUES.has(key)
            ? PRECISION.value
            : PRECISION.position,
      );
    }
    if (Array.isArray(item)) {
      return item.map((entry) => visit(entry, key));
    }
    if (item && typeof item === "object") {
      const result: Record<string, unknown> = {};
      for (const field in item) {
        const entry = (item as Record<string, unknown>)[field];
        if (entry !== undefined) {
          result[field] = visit(entry, field, key);
        }
      }
      return result;
    }
    return item;
  };
  return visit(value) as T;
}

/*
 * The host writes each wire record directly instead of reading and rounding generic views
 * every frame: the fields and key order of its reader, with the precision `rounded` gives that
 * field name. Integer fields (ids, counts, colours, seeds, teams) are already exact. A new
 * wire field belongs in both its reader and its function here; tests/scene-capture.test.ts
 * holds the capture to the reader-and-rounded reference byte for byte, and clients still
 * validate everything they receive.
 */
type Writable<T> = { -readonly [K in keyof T]: T[K] };
const round = {
  position: (value: number) => wireNumber(value, PRECISION.position),
  rotation: (value: number) => wireNumber(value, PRECISION.rotation),
  value: (value: number) => wireNumber(value, PRECISION.value),
};
const vector = (v: RenderPosition): RenderPosition => ({
  x: round.position(v.x),
  y: round.position(v.y),
  z: round.position(v.z),
});
const quaternion = (q: RenderRotation): RenderRotation => ({
  x: round.rotation(q.x),
  y: round.rotation(q.y),
  z: round.rotation(q.z),
  w: round.rotation(q.w),
});
function wireTank(tank: Tank, maxHp: number): WireTank {
  return {
    id: tank.id,
    life: tank.life,
    name: tank.name,
    kind: tank.kind,
    team: tank.team,
    human: tank.human,
    alive: tank.alive,
    position: vector(tankPosition(tank)),
    velocity: vector(tankVelocity(tank)),
    heading: round.rotation(tank.heading),
    aim: round.rotation(tank.aim),
    hp: round.value(tank.hp),
    maxHp: round.value(maxHp),
    xp: round.value(tank.xp),
    shield: round.value(tank.shield),
    shieldPoints: round.value(tank.shieldPoints),
    protection: round.value(tank.protection),
    laser: round.value(tank.laser),
    recoil: round.value(tank.recoil),
    cooldown: round.value(tank.cooldown),
    mineCooldown: round.value(tank.mineCooldown),
    respawn: round.value(tank.respawn),
    rapid: round.value(tank.rapid),
    speed: round.value(tank.speed),
    selectedAmmo: tank.selectedAmmo,
    ammo: {
      spread: tank.ammo.spread,
      rocket: tank.ammo.rocket,
      ricochet: tank.ammo.ricochet,
      piercing: tank.ammo.piercing,
    },
    kills: tank.kills,
    deaths: tank.deaths,
    lastCombat: round.value(tank.lastCombat),
  };
}
function wireCover(cover: Cover): WireCover {
  const wire = {
    id: cover.id,
    kind: cover.kind,
    x: round.position(cover.x),
    z: round.position(cover.z),
    w: round.position(cover.w),
    h: round.position(cover.h),
    d: round.position(cover.d),
    hp: Number.isFinite(cover.hp) ? round.value(cover.hp) : null,
    maxHp: Number.isFinite(cover.maxHp) ? round.value(cover.maxHp) : null,
    alive: cover.alive,
    destructible: cover.destructible,
    color: cover.color,
  } as Writable<WireCover>;
  if (cover.debrisSeed !== undefined) {
    wire.debrisSeed = cover.debrisSeed;
  }
  wire.position = vector(coverPosition(cover));
  wire.rotation = quaternion(coverRotation(cover));
  if (cover.timberHits) {
    wire.timberHits = cover.timberHits.map((hit) => ({
      x: round.position(hit.x),
      y: round.position(hit.y),
      z: round.position(hit.z),
      size: round.position(hit.size),
    }));
  }
  if (cover.timberJoin) {
    const join: TimberJoin = {};
    if (cover.timberJoin.openMin !== undefined) {
      join.openMin = cover.timberJoin.openMin;
    }
    if (cover.timberJoin.openMax !== undefined) {
      join.openMax = cover.timberJoin.openMax;
    }
    if (cover.timberJoin.post !== undefined) {
      join.post = cover.timberJoin.post;
    }
    wire.timberJoin = join;
  }
  if (cover.motion) {
    wire.motion = {
      originX: round.position(cover.motion.originX),
      originZ: round.position(cover.motion.originZ),
      w: round.position(cover.motion.w),
      d: round.position(cover.motion.d),
    };
  }
  return wire;
}
function wireTimberPart(part: TimberPart): TimberPart {
  return {
    kind: part.kind,
    index: part.index,
    x: round.position(part.x),
    y: round.position(part.y),
    z: round.position(part.z),
    w: round.position(part.w),
    h: round.position(part.h),
    d: round.position(part.d),
    yaw: round.rotation(part.yaw),
    lean: round.rotation(part.lean),
    color: part.color,
    damage: round.position(part.damage),
    damageSeed: part.damageSeed,
    marks: part.marks.map((mark) => ({
      x: round.position(mark.x),
      y: round.position(mark.y),
      face: mark.face,
      size: round.position(mark.size),
      seed: mark.seed,
    })),
  };
}
function wireFragment(fragment: Fragment): RenderFragment {
  const wire = {
    id: fragment.id,
    // Clients read life only for the final fade, so a steady value until then keeps
    // every settled piece out of the per-frame deltas.
    life: round.value(Math.min(fragment.life, DEBRIS_CLEANUP_SECONDS)),
    size: round.position(fragment.size),
    color: fragment.color,
    position: vector(fragment.body.translation()),
    rotation: quaternion(fragment.body.rotation()),
  } as Writable<RenderFragment>;
  if (fragment.shape !== undefined) {
    wire.shape = fragment.shape;
  }
  if (fragment.dimensions) {
    wire.dimensions = vector(fragment.dimensions);
  }
  if (fragment.material !== undefined) {
    wire.material = fragment.material;
  }
  if (fragment.sourceKind !== undefined) {
    wire.sourceKind = fragment.sourceKind;
  }
  if (fragment.timberPart) {
    wire.timberPart = wireTimberPart(fragment.timberPart);
  }
  if (fragment.treeCoverId !== undefined) {
    wire.treeCoverId = fragment.treeCoverId;
  }
  if (fragment.treeCenterY !== undefined) {
    wire.treeCenterY = round.position(fragment.treeCenterY);
  }
  if (fragment.createdAt !== undefined) {
    wire.createdAt = round.value(fragment.createdAt);
  }
  if (fragment.expiresAt !== undefined) {
    wire.expiresAt = round.value(fragment.expiresAt);
  }
  if (fragment.wreck !== undefined) {
    wire.wreck = fragment.wreck;
  }
  if (fragment.part !== undefined) {
    wire.part = fragment.part;
  }
  if (fragment.team !== undefined) {
    wire.team = fragment.team;
  }
  return wire;
}
function wireShot(shot: RenderShot): RenderShot {
  const wire = { id: shot.id, x: round.position(shot.x), z: round.position(shot.z) } as RenderShot;
  if (shot.y !== undefined) {
    wire.y = round.position(shot.y);
  }
  if (shot.visualY !== undefined) {
    wire.visualY = round.position(shot.visualY);
  }
  wire.team = shot.team;
  wire.vx = round.position(shot.vx);
  wire.vz = round.position(shot.vz);
  wire.weapon = shot.weapon;
  return wire;
}
function wireMine(mine: Mine): Mine {
  const wire = {
    id: mine.id,
    x: round.position(mine.x),
    z: round.position(mine.z),
    owner: mine.owner,
  } as Mine;
  if (mine.ownerLife !== undefined) {
    wire.ownerLife = mine.ownerLife;
  }
  if (mine.damage !== undefined) {
    wire.damage = round.position(mine.damage);
  }
  wire.team = mine.team;
  wire.arm = round.value(mine.arm);
  wire.life = round.value(mine.life);
  return wire;
}
function wirePickup(pickup: Pickup): Pickup {
  const wire: Pickup = {
    id: pickup.id,
    x: round.position(pickup.x),
    z: round.position(pickup.z),
    kind: pickup.kind,
    available: pickup.available,
    cooldown: round.value(pickup.cooldown),
  };
  if (pickup.cooldownDuration !== undefined) {
    wire.cooldownDuration = round.value(pickup.cooldownDuration);
  }
  return wire;
}
function wireMatch(match: Match): Match {
  const wire = {
    phase: match.phase,
    time: round.value(match.time),
    scores: [match.scores[0], match.scores[1]],
    overtime: match.overtime,
  } as Match;
  if (match.endedEarly !== undefined) {
    wire.endedEarly = match.endedEarly;
  }
  wire.winner = match.winner;
  wire.round = match.round;
  return wire;
}
function wireMap(simulation: Simulation): Scene["map"] {
  const map: Scene["map"] = { theme: simulation.mapTheme };
  if (simulation.mapFloor !== undefined) {
    map.floor = simulation.mapFloor;
  }
  if (simulation.mapOuterFloor !== undefined) {
    map.outerFloor = simulation.mapOuterFloor;
  }
  if (simulation.mapOuterFloorExtent !== undefined) {
    map.outerFloorExtent = round.position(simulation.mapOuterFloorExtent);
  }
  if (simulation.mapScale !== 1) {
    map.scale = round.position(simulation.mapScale);
  }
  return map;
}
/** Reads the simulation's entities rather than the local render views, whose per-entity
 * getters cost more than the capture itself. */
export function captureScene(simulation: Simulation): Scene {
  return {
    entities: {
      tanks: simulation.tanks.map((tank) => wireTank(tank, simulation.maxHealth(tank))),
      covers: simulation.covers.map(wireCover),
      fragments: simulation.fragments.map(wireFragment),
      shots: simulation.shots.map(wireShot),
      mines: simulation.mines.map(wireMine),
      pickups: simulation.pickups.map(wirePickup),
    },
    elapsed: round.position(simulation.elapsed),
    match: wireMatch(simulation.match),
    map: wireMap(simulation),
  };
}
export function projectScene(scene: Scene, viewerId: number): RenderState {
  const tanks = scene.entities.tanks.map((tank) => ({
    ...tank,
    previous: { x: tank.position.x, z: tank.position.z },
  }));
  const viewer = tanks.find((tank) => tank.id === viewerId);
  if (!viewer) {
    throw new Error("Missing viewer");
  }
  const normalized = <T extends { rotation: { x: number; y: number; z: number; w: number } }>(
    entity: T,
  ): T => {
    const q = entity.rotation;
    const length = Math.hypot(q.x, q.y, q.z, q.w);
    if (length < 0.5) {
      throw new Error("Invalid rotation");
    }
    return {
      ...entity,
      rotation: { x: q.x / length, y: q.y / length, z: q.z / length, w: q.w / length },
    };
  };
  return {
    ...scene.entities,
    tanks,
    covers: scene.entities.covers.map((cover) =>
      normalized({ ...cover, hp: cover.hp ?? Infinity, maxHp: cover.maxHp ?? Infinity }),
    ),
    fragments: scene.entities.fragments.map(normalized),
    viewer,
    viewerId,
    elapsed: scene.elapsed,
    match: scene.match,
    mapTheme: scene.map.theme,
    mapFloor: scene.map.floor,
    mapOuterFloor: scene.map.outerFloor,
    mapOuterFloorExtent: scene.map.outerFloorExtent,
    mapScale: scene.map.scale ?? 1,
  };
}
