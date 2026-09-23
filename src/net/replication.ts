import type { SimEvent, Shot } from "../game/types";
import type { RenderState } from "../game/render-state";
import {
  ENTITY_TYPES,
  entityReaders,
  sceneReader,
  eventReader,
  shotReader,
  projectScene,
  type Scene,
  type EntityType,
} from "./scene-codec";
import { array, id, number, object, record } from "./schema";

export interface TimedEvent {
  eventId: number;
  tick: number;
  event: SimEvent;
}
export interface ShotTrace {
  tick: number;
  endTick: number;
  shot: Shot;
  end: { x: number; z: number };
}
export interface Identity {
  roomEpoch: string;
  roundId: number;
}
export interface FullState extends Identity {
  type: "full";
  seq: number;
  tick: number;
  eventCursor: number;
  state: Scene;
}
export interface Change {
  kind: EntityType;
  id: number;
  set: Record<string, unknown>;
}
export interface Snapshot extends Identity {
  type: "snap";
  seq: number;
  tick: number;
  elapsed: number;
  match: Scene["match"];
  updates: Change[];
  removed: { kind: EntityType; id: number }[];
  events: TimedEvent[];
  traces: ShotTrace[];
}
export const timedEventReader = object<TimedEvent>({
  eventId: id,
  tick: number(0),
  event: eventReader,
});
export const traceReader = object<ShotTrace>({
  tick: number(0),
  endTick: number(0),
  shot: shotReader,
  end: object({ x: number(), z: number() }),
});
const key = (kind: EntityType, entityId: number) => kind + ":" + entityId;
function records(scene: Scene): Map<string, Record<string, unknown>> {
  const result = new Map<string, Record<string, unknown>>();
  for (const kind of ENTITY_TYPES) {
    for (const entity of scene.entities[kind]) {
      const name = key(kind, entity.id);
      if (result.has(name)) {
        throw new Error("Duplicate entity id");
      }
      result.set(name, entity as unknown as Record<string, unknown>);
    }
  }
  return result;
}
/** Static fields are sent at creation; updates contain only changed fields, including null deletions. */
export class StateStream {
  seq = 0;
  private previous = new Map<string, Record<string, unknown>>();
  constructor(private readonly identity: Identity) {}
  full(state: Scene, tick: number, eventCursor: number): FullState {
    if (this.seq === 0 && !this.previous.size) {
      this.previous = records(state);
    }
    return { ...this.identity, type: "full", seq: this.seq, tick, eventCursor, state };
  }
  snapshot(state: Scene, tick: number, events: TimedEvent[], traces: ShotTrace[]): Snapshot {
    const next = records(state);
    const updates: Change[] = [];
    const removed: Snapshot["removed"] = [];
    for (const kind of ENTITY_TYPES) {
      for (const entity of state.entities[kind]) {
        const name = key(kind, entity.id);
        const before = this.previous.get(name);
        const current = next.get(name)!;
        const set: Record<string, unknown> = {};
        for (const field of new Set([...Object.keys(current), ...Object.keys(before ?? {})])) {
          if (!before || JSON.stringify(before[field]) !== JSON.stringify(current[field])) {
            set[field] = current[field] ?? null;
          }
        }
        if (Object.keys(set).length) {
          updates.push({ kind, id: entity.id, set });
        }
      }
    }
    for (const name of this.previous.keys()) {
      if (!next.has(name)) {
        const [kind, entityId] = name.split(":");
        removed.push({ kind: kind as EntityType, id: Number(entityId) });
      }
    }
    this.previous = next;
    return {
      ...this.identity,
      type: "snap",
      seq: ++this.seq,
      tick,
      elapsed: state.elapsed,
      match: state.match,
      updates,
      removed,
      events,
      traces,
    };
  }
}

/** Validation is transactional: malformed or skipped deltas never partially alter the mirror. */
export class StateMirror {
  state?: Scene;
  roomEpoch = "";
  roundId = 0;
  seq = 0;
  tick = 0;
  eventCursor = 0;
  needsFull = true;
  applyFull(value: unknown, identity: Identity): void {
    const data = record(value);
    if (
      data.type !== "full" ||
      data.roomEpoch !== identity.roomEpoch ||
      data.roundId !== identity.roundId
    ) {
      throw new Error("Wrong baseline identity");
    }
    const state = sceneReader.read(data.state);
    const seq = id.read(data.seq);
    const tick = id.read(data.tick);
    const cursor = id.read(data.eventCursor);
    records(state);
    // Validate quaternion semantics before committing the scene.
    projectScene(state, state.entities.tanks[0]?.id ?? -1);
    this.state = state;
    this.roomEpoch = identity.roomEpoch;
    this.roundId = identity.roundId;
    this.seq = seq;
    this.tick = tick;
    this.eventCursor = cursor;
    this.needsFull = false;
  }
  applySnapshot(value: unknown): { events: TimedEvent[]; traces: ShotTrace[] } | undefined {
    if (!this.state || this.needsFull) {
      return;
    }
    try {
      const data = record(value);
      if (
        data.type !== "snap" ||
        data.roomEpoch !== this.roomEpoch ||
        data.roundId !== this.roundId ||
        data.seq !== this.seq + 1
      ) {
        throw new Error("Snapshot gap");
      }
      const tick = id.read(data.tick);
      if (tick < this.tick) {
        throw new Error("Tick went backwards");
      }
      if (
        !Array.isArray(data.updates) ||
        data.updates.length > 2048 ||
        !Array.isArray(data.removed) ||
        data.removed.length > 2048
      ) {
        throw new Error("Invalid changes");
      }
      const next = records(this.state);
      const changed = new Set<string>();
      const entityKey = (value: unknown) => {
        const change = record(value);
        if (!ENTITY_TYPES.some((kind) => kind === change.kind)) {
          throw new Error("Invalid entity type");
        }
        const kind = change.kind as EntityType;
        const entityId = id.read(change.id);
        const name = key(kind, entityId);
        if (changed.has(name)) {
          throw new Error("Duplicate change");
        }
        changed.add(name);
        return { change, kind, entityId, name };
      };
      for (const raw of data.updates) {
        const { change, kind, entityId, name } = entityKey(raw);
        const set = record(change.set);
        const merged = { ...next.get(name), ...set };
        for (const field of Object.keys(set)) {
          if (
            set[field] === null &&
            !(kind === "covers" && (field === "hp" || field === "maxHp"))
          ) {
            delete merged[field];
          }
        }
        const entity = entityReaders[kind].read(merged);
        if (entity.id !== entityId) {
          throw new Error("Entity identity changed");
        }
        next.set(name, entity as unknown as Record<string, unknown>);
      }
      for (const raw of data.removed) {
        const { name } = entityKey(raw);
        if (!next.delete(name)) {
          throw new Error("Unknown removal");
        }
      }
      const entities = Object.fromEntries(
        ENTITY_TYPES.map((kind) => [
          kind,
          [...next.entries()]
            .filter(([name]) => name.startsWith(kind + ":"))
            .map(([, entity]) => entity),
        ]),
      );
      const state = sceneReader.read({
        ...this.state,
        entities,
        elapsed: data.elapsed,
        match: data.match,
      });
      projectScene(state, state.entities.tanks[0]?.id ?? -1);
      const events = array(timedEventReader, 2048)
        .read(data.events)
        .filter((event) => event.eventId > this.eventCursor);
      const traces = array(traceReader, 2048).read(data.traces);
      let cursor = this.eventCursor;
      for (const event of events) {
        if (event.eventId !== cursor + 1 || event.tick > tick) {
          throw new Error("Event gap");
        }
        cursor = event.eventId;
      }
      for (const trace of traces) {
        if (trace.tick > trace.endTick || trace.endTick > tick) {
          throw new Error("Invalid projectile timeline");
        }
      }
      this.state = state;
      this.tick = tick;
      this.seq++;
      this.eventCursor = cursor;
      return { events, traces };
    } catch {
      this.needsFull = true;
      return;
    }
  }
  render(viewerId: number): RenderState {
    if (!this.state) {
      throw new Error("No baseline");
    }
    return projectScene(this.state, viewerId);
  }
}
