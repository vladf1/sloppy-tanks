import {
  renderState,
  type RenderState,
  type RenderTank,
  type RenderCover,
  type RenderFragment,
} from "../game/render-state";
import type { Simulation } from "../game/simulation";
import type { Match, Shot, Mine, Pickup, SimEvent } from "../game/types";
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
export type WireCover = Omit<RenderCover, "hp" | "maxHp"> & {
  hp: number | null;
  maxHp: number | null;
};
export interface Entities {
  tanks: RenderTank[];
  covers: WireCover[];
  fragments: RenderFragment[];
  shots: Shot[];
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
export const mapMode = enumeration("village", "harbor", "quarry");
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
export const tankReader = object<RenderTank>({
  id,
  life: id,
  name: string(64),
  kind,
  team,
  human: boolean,
  alive: boolean,
  previous: point,
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
  motion: optional(
    object({ originX: n, originZ: n, w: n, d: n, x: n, z: n, navW: n, navD: n, checkAt: n }),
  ),
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
export const shotReader = object<Shot>({
  id,
  x: n,
  z: n,
  y: optional(n),
  visualY: optional(n),
  owner: id,
  ownerLife: optional(id),
  team,
  vx: n,
  vz: n,
  damage: n,
  bounces: id,
  life: n,
  weapon,
  piercing: id,
  targetId: optional(id),
  targetLife: optional(id),
  recapHit: optional(boolean),
  piercedShot: optional(id),
  laserCheckedBy: optional(array(id, 12)),
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
  tanks: array(tankReader, 12),
  covers: array(coverReader, 1024),
  fragments: array(fragmentReader, 128),
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
    theme: mapMode,
    floor: optional(ground),
    outerFloor: optional(ground),
    outerFloorExtent: optional(n),
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
/** Only already-projected plain records enter this rounding pass. */
export function rounded<T>(value: T): T {
  const visit = (item: unknown, key = "", parent = ""): unknown => {
    if (typeof item === "number") {
      if (!Number.isFinite(item)) {
        throw new Error("Non-finite wire number");
      }
      const scale =
        parent === "rotation" || ANGLES.has(key)
          ? PRECISION.rotation
          : VALUES.has(key)
            ? PRECISION.value
            : PRECISION.position;
      return Math.round(item * scale) / scale || 0;
    }
    if (Array.isArray(item)) {
      return item.map((entry) => visit(entry, key));
    }
    if (item && typeof item === "object") {
      return Object.fromEntries(
        Object.entries(item)
          .filter(([, entry]) => entry !== undefined)
          .map(([field, entry]) => [field, visit(entry, field, key)]),
      );
    }
    return item;
  };
  return visit(value) as T;
}
export function captureScene(simulation: Simulation): Scene {
  const view = renderState(simulation, undefined, simulation.tanks[0].id);
  return rounded(
    sceneReader.read({
      entities: {
        tanks: view.tanks,
        covers: view.covers.map((cover) => ({
          ...cover,
          hp: Number.isFinite(cover.hp) ? cover.hp : null,
          maxHp: Number.isFinite(cover.maxHp) ? cover.maxHp : null,
        })),
        fragments: view.fragments,
        shots: view.shots,
        mines: view.mines,
        pickups: view.pickups,
      },
      elapsed: view.elapsed,
      match: view.match,
      map: {
        theme: view.mapTheme,
        floor: view.mapFloor,
        outerFloor: view.mapOuterFloor,
        outerFloorExtent: view.mapOuterFloorExtent,
      },
    }),
  );
}
export function projectScene(scene: Scene, viewerId: number): RenderState {
  const viewer = scene.entities.tanks.find((tank) => tank.id === viewerId);
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
  };
}
