import type { Simulation } from "./simulation";
import type { Cover, Fragment, Mine, Pickup, Shot, Tank } from "./types";

export interface RenderPosition {
  x: number;
  y: number;
  z: number;
}
export interface RenderRotation extends RenderPosition {
  w: number;
}
export interface WreckView {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}
const TANK_FIELDS = [
  "id",
  "name",
  "kind",
  "team",
  "human",
  "alive",
  "previous",
  "heading",
  "aim",
  "hp",
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
  "selectedAmmo",
  "ammo",
  "kills",
  "deaths",
  "lastCombat",
] as const;
const COVER_FIELDS = [
  "id",
  "kind",
  "x",
  "z",
  "w",
  "h",
  "d",
  "hp",
  "maxHp",
  "alive",
  "destructible",
  "color",
  "debrisSeed",
  "timberHits",
  "timberJoin",
  "motion",
] as const;
const FRAGMENT_FIELDS = [
  "id",
  "life",
  "size",
  "color",
  "shape",
  "dimensions",
  "material",
  "sourceKind",
  "timberPart",
  "treeCoverId",
  "treeCenterY",
  "createdAt",
  "expiresAt",
  "wreck",
  "part",
  "team",
] as const;
export type RenderTank = Readonly<
  Pick<Tank, (typeof TANK_FIELDS)[number]> & {
    position: RenderPosition;
    velocity: RenderPosition;
    maxHp: number;
    life: number;
  }
>;
export type RenderCover = Readonly<
  Pick<Cover, (typeof COVER_FIELDS)[number]> & {
    position: RenderPosition;
    rotation: RenderRotation;
  }
>;
export type RenderFragment = Readonly<
  Pick<Fragment, (typeof FRAGMENT_FIELDS)[number]> & {
    position: RenderPosition;
    rotation: RenderRotation;
  }
>;

/** Presentation reads values only. Network implementations contain no physics world or handles. */
export interface RenderState {
  readonly viewerId: number;
  readonly viewer: RenderTank;
  readonly tanks: readonly RenderTank[];
  readonly covers: readonly RenderCover[];
  readonly fragments: readonly RenderFragment[];
  readonly shots: readonly Shot[];
  readonly mines: readonly Mine[];
  readonly pickups: readonly Pickup[];
  readonly elapsed: number;
  readonly match: Readonly<Simulation["match"]>;
  readonly mapTheme: Simulation["mapTheme"];
  readonly mapFloor: Simulation["mapFloor"];
  readonly mapOuterFloor: Simulation["mapOuterFloor"];
  readonly mapOuterFloorExtent: Simulation["mapOuterFloorExtent"];
  readonly customMap?: Simulation["customMap"];
}

/** Stable getters avoid copying every entity every single-player frame. */
function fields<T extends object, K extends keyof T, E extends object>(
  source: T,
  keys: readonly K[],
  extra: E,
): Readonly<Pick<T, K> & E> {
  return Object.defineProperties(
    extra,
    Object.fromEntries(
      keys.map((key) => [
        key,
        {
          enumerable: true,
          get: () => source[key],
        },
      ]),
    ),
  ) as Readonly<Pick<T, K> & E>;
}
class EntityViews<T extends object, V> {
  private cache = new WeakMap<T, V>();
  private views: V[] = [];
  constructor(private readonly create: (entity: T) => V) {}
  get(entity: T): V {
    let view = this.cache.get(entity);
    if (!view) {
      view = this.create(entity);
      this.cache.set(entity, view);
    }
    return view;
  }
  read(entities: T[]): V[] {
    this.views.length = entities.length;
    for (let i = 0; i < entities.length; i++) {
      this.views[i] = this.get(entities[i]);
    }
    return this.views;
  }
}
class LocalRenderState implements RenderState {
  constructor(
    private readonly simulation: Simulation,
    private readonly tankId?: number,
  ) {}
  private tankViews = new EntityViews<Tank, RenderTank>((tank) => {
    const simulation = this.simulation;
    return fields(tank, TANK_FIELDS, {
      get position() {
        return tank.alive ? tank.body.translation() : { ...tank.previous, y: 0.65 };
      },
      get velocity() {
        return tank.alive ? tank.body.linvel() : { x: 0, y: 0, z: 0 };
      },
      get maxHp() {
        return simulation.maxHealth(tank);
      },
      get life() {
        return tank.life;
      },
    });
  });
  private coverViews = new EntityViews<Cover, RenderCover>((cover) =>
    fields(cover, COVER_FIELDS, {
      get position() {
        return cover.body.isValid() ? cover.body.translation() : { x: cover.x, y: 0, z: cover.z };
      },
      get rotation() {
        return cover.body.isValid() ? cover.body.rotation() : { x: 0, y: 0, z: 0, w: 1 };
      },
    }),
  );
  private fragmentViews = new EntityViews<Fragment, RenderFragment>((fragment) =>
    fields(fragment, FRAGMENT_FIELDS, {
      get position() {
        return fragment.body.translation();
      },
      get rotation() {
        return fragment.body.rotation();
      },
    }),
  );
  get viewerId() {
    return this.tankId ?? this.simulation.human.id;
  }
  get viewer() {
    const tank =
      this.tankId === undefined
        ? this.simulation.human
        : this.simulation.tanks.find((tank) => tank.id === this.tankId)!;
    return this.tankViews.get(tank);
  }
  get tanks() {
    return this.tankViews.read(this.simulation.tanks);
  }
  get covers() {
    return this.coverViews.read(this.simulation.covers);
  }
  get fragments() {
    return this.fragmentViews.read(this.simulation.fragments);
  }
  get shots() {
    return this.simulation.shots;
  }
  get mines() {
    return this.simulation.mines;
  }
  get pickups() {
    return this.simulation.pickups;
  }
  get elapsed() {
    return this.simulation.elapsed;
  }
  get match() {
    return this.simulation.match;
  }
  get mapTheme() {
    return this.simulation.mapTheme;
  }
  get mapFloor() {
    return this.simulation.mapFloor;
  }
  get mapOuterFloor() {
    return this.simulation.mapOuterFloor;
  }
  get mapOuterFloorExtent() {
    return this.simulation.mapOuterFloorExtent;
  }
  get customMap() {
    return this.simulation.customMap;
  }
}
const localViews = new WeakMap<Simulation, LocalRenderState>();
const viewerViews = new WeakMap<Tank, LocalRenderState>();
/** Compatibility at local call sites, including fixtures. The view itself stays read-only. */
export function renderState(
  source: Simulation | RenderState,
  wreckView?: WreckView,
  viewerId?: number,
): RenderState {
  if ("viewerId" in source) {
    return source;
  }
  if (wreckView && !source.multiplayer) {
    source.wreckView = wreckView;
  }
  if (viewerId !== undefined) {
    const tank = source.tanks.find((tank) => tank.id === viewerId);
    if (!tank) {
      throw new Error("Unknown viewer tank");
    }
    let view = viewerViews.get(tank);
    if (!view) {
      view = new LocalRenderState(source, viewerId);
      viewerViews.set(tank, view);
    }
    return view;
  }
  let view = localViews.get(source);
  if (!view) {
    view = new LocalRenderState(source);
    localViews.set(source, view);
  }
  return view;
}
