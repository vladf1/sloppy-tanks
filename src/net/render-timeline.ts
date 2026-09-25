import { angleDelta } from "../game/math";
import {
  renderState,
  type RenderState,
  type RenderTank,
  type RenderCover,
  type RenderFragment,
  type RenderRotation,
  type RenderShot,
} from "../game/render-state";
import type { Simulation } from "../game/simulation";
import type { SimEvent } from "../game/types";

export type HullPolicy = "latest" | "smooth" | "extrapolate";
export interface RenderSample {
  state: RenderState;
  events: SimEvent[];
  ack: number;
}
const MAX_SAMPLES = 32;
/** Longest a hull is carried past its newest authoritative pose, local or remote. */
export const MAX_EXTRAPOLATION_SECONDS = 0.1;
const CORRECTION_RATE = 20;

function interpolateRotation(a: RenderRotation, b: RenderRotation, alpha: number): RenderRotation {
  const sign = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w < 0 ? -1 : 1;
  const x = a.x + (b.x * sign - a.x) * alpha;
  const y = a.y + (b.y * sign - a.y) * alpha;
  const z = a.z + (b.z * sign - a.z) * alpha;
  const w = a.w + (b.w * sign - a.w) * alpha;
  const length = Math.hypot(x, y, z, w) || 1;
  return { x: x / length, y: y / length, z: z / length, w: w / length };
}

/** Used only by the dev latency experiment; normal single-player never captures scene copies. */
export function captureRenderState(simulation: Simulation, viewerId?: number): RenderState {
  const source = renderState(simulation, undefined, viewerId);
  const tanks = source.tanks.map((tank) => structuredClone({ ...tank }));
  return {
    viewerId: source.viewerId,
    viewer: tanks.find((tank) => tank.id === source.viewerId)!,
    tanks,
    covers: source.covers.map((cover) => structuredClone({ ...cover })),
    fragments: source.fragments.map((fragment) => structuredClone({ ...fragment })),
    shots: structuredClone(source.shots),
    mines: structuredClone(source.mines),
    pickups: structuredClone(source.pickups),
    elapsed: source.elapsed,
    match: structuredClone(source.match),
    mapTheme: source.mapTheme,
    mapFloor: source.mapFloor,
    mapOuterFloor: source.mapOuterFloor,
    mapOuterFloorExtent: source.mapOuterFloorExtent,
    customMap: source.customMap,
  };
}
type Mutable<T> = { -readonly [K in keyof T]: T[K] };

/** Membership comes from the older sample, so removals wait for the display clock. */
export class RenderTimeline {
  private samples: RenderSample[] = [];
  private tankCache = new Map<number, Mutable<RenderTank>>();
  private coverCache = new Map<number, Mutable<RenderCover>>();
  private fragmentCache = new Map<number, Mutable<RenderFragment>>();
  private shotCache = new Map<number, RenderShot>();
  private tanks: RenderTank[] = [];
  private covers: RenderCover[] = [];
  private fragments: RenderFragment[] = [];
  private shots: RenderShot[] = [];
  private displayedEventTime = -Infinity;
  private local?: Mutable<RenderTank>;
  private output?: RenderState;
  latestAck = 0;
  reset(sample: RenderSample): void {
    this.samples = [sample];
    this.tankCache.clear();
    this.coverCache.clear();
    this.fragmentCache.clear();
    this.shotCache.clear();
    this.local = undefined;
    this.displayedEventTime = sample.state.elapsed;
    this.output = undefined;
  }
  push(sample: RenderSample): void {
    this.samples.push(sample);
    if (this.samples.length > MAX_SAMPLES) {
      this.samples.shift();
    }
  }
  read(
    time: number,
    localTime: number,
    dt: number,
    policy: HullPolicy,
    coherentLifecycle = false,
  ): { state: RenderState; events: SimEvent[] } {
    const newest = this.samples.at(-1)!;
    // A late packet carries remote hulls along their velocity briefly instead of freezing them.
    const overrun = Math.max(0, Math.min(MAX_EXTRAPOLATION_SECONDS, time - newest.state.elapsed));
    time = Math.min(time, newest.state.elapsed);
    let index = 0;
    while (index + 1 < this.samples.length && this.samples[index + 1].state.elapsed <= time) {
      index++;
    }
    const before = this.samples[index].state;
    const after = this.samples[index + 1]?.state ?? before;
    const fraction =
      after.elapsed > before.elapsed
        ? Math.max(0, Math.min(1, (time - before.elapsed) / (after.elapsed - before.elapsed)))
        : 0;
    const events: SimEvent[] = [];
    for (const sample of this.samples) {
      if (sample.state.elapsed > this.displayedEventTime && sample.state.elapsed <= time) {
        events.push(...sample.events);
      }
    }
    this.displayedEventTime = Math.max(this.displayedEventTime, time);
    const pose = <T extends { id: number; position: { x: number; y: number; z: number } }>(
      a: T,
      b: T | undefined,
      cache: Map<number, Mutable<T>>,
    ): Mutable<T> => {
      let out = cache.get(a.id);
      if (!out) {
        out = { ...a, position: { ...a.position } };
        cache.set(a.id, out);
      }
      const position = out.position;
      Object.assign(out, a);
      out.position = position;
      for (const axis of ["x", "y", "z"] as const) {
        position[axis] =
          a.position[axis] +
          ((b?.position[axis] ?? a.position[axis]) - a.position[axis]) * fraction;
      }
      return out;
    };
    this.tanks.length = 0;
    for (const tank of before.tanks) {
      const next = after.tanks.find(
        (candidate) =>
          candidate.id === tank.id &&
          candidate.life === tank.life &&
          candidate.alive === tank.alive,
      );
      const out = pose(tank, next, this.tankCache);
      out.heading =
        tank.heading + angleDelta(tank.heading, next?.heading ?? tank.heading) * fraction;
      out.aim = tank.aim + angleDelta(tank.aim, next?.aim ?? tank.aim) * fraction;
      if (overrun && tank.alive) {
        out.position.x += tank.velocity.x * overrun;
        out.position.z += tank.velocity.z * overrun;
      }
      out.previous = out.position;
      this.tanks.push(out);
    }
    this.covers.length = 0;
    for (const cover of before.covers) {
      const next = after.covers.find(
        (candidate) => candidate.id === cover.id && candidate.alive === cover.alive,
      );
      const out = pose(cover, next, this.coverCache);
      out.rotation = interpolateRotation(
        cover.rotation,
        next?.rotation ?? cover.rotation,
        fraction,
      );
      this.covers.push(out);
    }
    this.fragments.length = 0;
    for (const fragment of before.fragments) {
      const next = after.fragments.find((candidate) => candidate.id === fragment.id);
      const out = pose(fragment, next, this.fragmentCache);
      out.rotation = interpolateRotation(
        fragment.rotation,
        next?.rotation ?? fragment.rotation,
        fraction,
      );
      this.fragments.push(out);
    }
    for (const [id] of this.fragmentCache) {
      if (!before.fragments.some((fragment) => fragment.id === id)) {
        this.fragmentCache.delete(id);
      }
    }
    this.shots.length = 0;
    for (const shot of before.shots) {
      const next = after.shots.find((candidate) => candidate.id === shot.id);
      let out = this.shotCache.get(shot.id);
      if (!out) {
        out = { ...shot };
        this.shotCache.set(shot.id, out);
      }
      Object.assign(out, shot);
      const ahead = Math.max(0, Math.min(0.05, time + overrun - before.elapsed));
      out.x = next ? shot.x + (next.x - shot.x) * fraction : shot.x + shot.vx * ahead;
      out.z = next ? shot.z + (next.z - shot.z) * fraction : shot.z + shot.vz * ahead;
      this.shots.push(out);
    }
    for (const [id] of this.shotCache) {
      if (!before.shots.some((shot) => shot.id === id)) {
        this.shotCache.delete(id);
      }
    }
    const authoritative =
      coherentLifecycle &&
      (before.viewer.life !== newest.state.viewer.life ||
        before.viewer.alive !== newest.state.viewer.alive)
        ? before.viewer
        : newest.state.viewer;
    const previousPosition = this.local?.position;
    const continuous =
      this.local?.life === authoritative.life && this.local.alive === authoritative.alive;
    const target = { ...authoritative.position };
    if (policy === "extrapolate" && authoritative.alive) {
      const ahead = Math.max(
        0,
        Math.min(MAX_EXTRAPOLATION_SECONDS, localTime - newest.state.elapsed),
      );
      target.x += authoritative.velocity.x * ahead;
      target.z += authoritative.velocity.z * ahead;
    }
    const blend = policy === "latest" || !continuous ? 1 : 1 - Math.exp(-CORRECTION_RATE * dt);
    // Local translation and hull rotation need the same frame-rate smoothing.
    // Copying heading from authority here made only our own tank turn at packet Hz.
    const heading =
      continuous && this.local
        ? this.local.heading + angleDelta(this.local.heading, authoritative.heading) * blend
        : authoritative.heading;
    if (previousPosition && continuous) {
      target.x = previousPosition.x + (target.x - previousPosition.x) * blend;
      target.z = previousPosition.z + (target.z - previousPosition.z) * blend;
    }
    this.local ??= { ...authoritative };
    Object.assign(this.local, authoritative, { position: target, previous: target, heading });
    const viewerIndex = this.tanks.findIndex((tank) => tank.id === authoritative.id);
    if (viewerIndex >= 0) {
      this.tanks[viewerIndex] = this.local;
    }
    this.latestAck = newest.ack;
    this.output = {
      ...before,
      elapsed: Math.max(before.elapsed, time),
      viewer: this.local,
      tanks: this.tanks,
      covers: this.covers,
      fragments: this.fragments,
      shots: this.shots,
    };
    // Keep one predecessor for interpolation, with all delayed lifecycle data intact.
    if (index > 0) {
      this.samples.splice(0, index);
    }
    return { state: this.output, events };
  }
}
