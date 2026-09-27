import type { SimEvent } from "../game/types";
import type { RenderShot, RenderState } from "../game/render-state";
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
import { array, id, number, object, optional, record } from "./schema";

export interface TimedEvent {
  eventId: number;
  tick: number;
  event: SimEvent;
}
export interface ShotTrace {
  tick: number;
  endTick: number;
  shot: RenderShot;
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
/** Changed fields of one record; null deletes an optional field. */
export type FieldChanges = Record<string, unknown>;
/**
 * One simulation frame after the previous one. The socket and the batch's roundId identify the
 * stream, so frames carry no identity of their own, and empty sections are omitted.
 */
export interface Snapshot {
  seq: number;
  tick: number;
  elapsed: number;
  match?: FieldChanges;
  /** Changed entity fields by kind, then by entity id. */
  updates?: Partial<Record<EntityType, Record<string, FieldChanges>>>;
  removed?: Partial<Record<EntityType, number[]>>;
  events?: TimedEvent[];
  traces?: ShotTrace[];
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
const MAX_CHANGES = 4096;
/** Fields whose null is a real value rather than a deletion. */
const COVER_NULLABLE = ["hp", "maxHp"];
const MATCH_NULLABLE = ["winner"];
/**
 * Equality of captured wire values, as their JSON would compare: finite numbers, strings,
 * booleans, null, arrays and plain records without undefined fields.
 */
function same(a: unknown, b: unknown): boolean {
  if (a === b) {
    return true;
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return false;
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) {
      return false;
    }
    for (let i = 0; i < a.length; i++) {
      if (!same(a[i], b[i])) {
        return false;
      }
    }
    return true;
  }
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  let fields = 0;
  for (const field in left) {
    if (!same(left[field], right[field])) {
      return false;
    }
    fields++;
  }
  // Every left field matched, so equal counts mean the right has no extra field.
  for (const _field in right) {
    fields--;
  }
  return fields === 0;
}
/** Current fields keep their order, followed by deleted fields as null. */
function changedFields(previous: object | undefined, next: object): FieldChanges | undefined {
  const before = previous as Record<string, unknown> | undefined;
  const current = next as Record<string, unknown>;
  if (!before) {
    return { ...current };
  }
  let changes: FieldChanges | undefined;
  for (const field in current) {
    if (!same(before[field], current[field])) {
      (changes ??= {})[field] = current[field];
    }
  }
  for (const field in before) {
    if (!Object.hasOwn(current, field)) {
      (changes ??= {})[field] = null;
    }
  }
  return changes;
}
function applyChanges(
  before: object | undefined,
  changes: FieldChanges,
  nullable: readonly string[],
): Record<string, unknown> {
  const merged = { ...before, ...changes };
  for (const field of Object.keys(changes)) {
    if (changes[field] === null && !nullable.includes(field)) {
      delete merged[field];
    }
  }
  return merged;
}
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
/** Each kind's records by entity id, so the host diffs frames without composite keys. */
type EntityIndex = { [K in EntityType]: Map<number, object> };
function indexEntities(scene: Scene): EntityIndex {
  const index = {} as EntityIndex;
  for (const kind of ENTITY_TYPES) {
    const byId = (index[kind] = new Map<number, object>());
    for (const entity of scene.entities[kind]) {
      if (byId.has(entity.id)) {
        throw new Error("Duplicate entity id");
      }
      byId.set(entity.id, entity);
    }
  }
  return index;
}
/** Static fields are sent at creation; updates contain only changed fields, including null deletions. */
export class StateStream {
  seq = 0;
  private previous?: EntityIndex;
  private previousMatch?: Scene["match"];
  constructor(private readonly identity: Identity) {}
  full(state: Scene, tick: number, eventCursor: number): FullState {
    if (!this.previous) {
      this.previous = indexEntities(state);
      this.previousMatch = state.match;
    }
    return { ...this.identity, type: "full", seq: this.seq, tick, eventCursor, state };
  }
  snapshot(state: Scene, tick: number, events: TimedEvent[], traces: ShotTrace[]): Snapshot {
    const next = indexEntities(state);
    const updates: NonNullable<Snapshot["updates"]> = {};
    const removed: NonNullable<Snapshot["removed"]> = {};
    for (const kind of ENTITY_TYPES) {
      const before = this.previous?.[kind];
      for (const entity of state.entities[kind]) {
        const changes = changedFields(before?.get(entity.id), entity);
        if (changes) {
          (updates[kind] ??= {})[entity.id] = changes;
        }
      }
      for (const entityId of before?.keys() ?? []) {
        if (!next[kind].has(entityId)) {
          (removed[kind] ??= []).push(entityId);
        }
      }
    }
    const match = changedFields(this.previousMatch, state.match);
    this.previous = next;
    this.previousMatch = state.match;
    const snapshot: Snapshot = { seq: ++this.seq, tick, elapsed: state.elapsed };
    if (match) {
      snapshot.match = match;
    }
    if (Object.keys(updates).length) {
      snapshot.updates = updates;
    }
    if (Object.keys(removed).length) {
      snapshot.removed = removed;
    }
    if (events.length) {
      snapshot.events = events;
    }
    if (traces.length) {
      snapshot.traces = traces;
    }
    return snapshot;
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
      if (data.seq !== this.seq + 1) {
        throw new Error("Snapshot gap");
      }
      const tick = id.read(data.tick);
      if (tick < this.tick) {
        throw new Error("Tick went backwards");
      }
      const next = records(this.state);
      const changed = new Set<string>();
      const entityKind = (kind: string) => {
        if (!ENTITY_TYPES.some((candidate) => candidate === kind)) {
          throw new Error("Invalid entity type");
        }
        return kind as EntityType;
      };
      const claim = (kind: EntityType, entityId: number) => {
        const name = key(kind, entityId);
        if (changed.has(name)) {
          throw new Error("Duplicate change");
        }
        if (changed.size >= MAX_CHANGES) {
          throw new Error("Invalid changes");
        }
        changed.add(name);
        return name;
      };
      for (const [kindName, byId] of Object.entries(record(data.updates ?? {}))) {
        const kind = entityKind(kindName);
        for (const [idText, changes] of Object.entries(record(byId))) {
          const entityId = id.read(Number(idText));
          if (String(entityId) !== idText) {
            throw new Error("Invalid entity id");
          }
          const name = claim(kind, entityId);
          const merged = applyChanges(
            next.get(name),
            record(changes),
            kind === "covers" ? COVER_NULLABLE : [],
          );
          const entity = entityReaders[kind].read(merged);
          if (entity.id !== entityId) {
            throw new Error("Entity identity changed");
          }
          next.set(name, entity as unknown as Record<string, unknown>);
        }
      }
      for (const [kindName, ids] of Object.entries(record(data.removed ?? {}))) {
        const kind = entityKind(kindName);
        for (const entityId of array(id, MAX_CHANGES).read(ids)) {
          if (!next.delete(claim(kind, entityId))) {
            throw new Error("Unknown removal");
          }
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
        match: applyChanges(this.state.match, record(data.match ?? {}), MATCH_NULLABLE),
      });
      projectScene(state, state.entities.tanks[0]?.id ?? -1);
      const events = (optional(array(timedEventReader, 2048)).read(data.events) ?? []).filter(
        (event) => event.eventId > this.eventCursor,
      );
      const traces = optional(array(traceReader, 2048)).read(data.traces) ?? [];
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
