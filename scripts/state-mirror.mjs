// A plain-JSON mirror of one room's replicated scene for the Node multiplayer checks,
// which watch a browser's or a bot's socket frames. It applies the wire format of
// `crates/core/src/net/replication.rs`: a `full` baseline (`{ roomEpoch, roundId, seq,
// tick, eventCursor, state: { entities: { tanks: [...], ... }, elapsed, match, map } }`),
// then one frame per captured tick with `seq` one higher, the changed fields of each
// record (`updates: { tanks: { "<id>": { field: value } } }`, a deleted optional field as
// `null`), removed ids and match changes, plus the shells in flight from the baseline's
// and frames' projectile `paths` (a launch or new path per shell, `{ id, end }` when it is
// gone). It checks the stream stays contiguous and
// keeps records as the wire sends them; the engine's own `StateMirror` does the
// transactional validation.

/** Fields whose `null` is a value rather than a deletion. */
const NULLABLE = { covers: ["hp", "maxHp"], match: ["winner"] };

function merge(target, fields, nullable = []) {
  for (const [key, value] of Object.entries(fields)) {
    if (value === null && !nullable.includes(key)) delete target[key];
    else target[key] = value;
  }
}

export class StateMirror {
  /** The scene as JSON; undefined until the first baseline. */
  state;
  /** Current path of each shell in flight, by id. */
  shots = new Map();
  roomEpoch = "";
  roundId = 0;
  seq = 0;
  tick = 0;
  eventCursor = 0;
  needsFull = true;

  /** Take a `full` baseline for `identity` (`{ roomEpoch, roundId }`). */
  applyFull(value, identity) {
    if (
      value?.type !== "full" ||
      value.roomEpoch !== identity.roomEpoch ||
      value.roundId !== identity.roundId
    ) {
      throw new Error("Wrong baseline identity");
    }
    if (!value.state?.entities || !value.state.match) throw new Error("Malformed baseline");
    this.state = structuredClone(value.state);
    this.shots = new Map((value.paths ?? []).map((path) => [path.id, path]));
    this.roomEpoch = identity.roomEpoch;
    this.roundId = identity.roundId;
    this.seq = value.seq;
    this.tick = value.tick;
    this.eventCursor = value.eventCursor;
    this.needsFull = false;
  }

  /** Apply the next frame; undefined (and a new baseline needed) on a gap or bad frame. */
  applySnapshot(value) {
    if (!this.state || this.needsFull) return undefined;
    if (value?.seq !== this.seq + 1 || !(value.tick >= this.tick)) {
      this.needsFull = true;
      return undefined;
    }
    const state = this.state;
    if (value.match) merge(state.match, value.match, NULLABLE.match);
    for (const [kind, byId] of Object.entries(value.updates ?? {})) {
      const records = (state.entities[kind] ??= []);
      for (const [id, fields] of Object.entries(byId)) {
        let record = records.find((entry) => entry.id === Number(id));
        if (!record) {
          record = { id: Number(id) };
          records.push(record);
        }
        merge(record, fields, NULLABLE[kind]);
      }
    }
    for (const [kind, ids] of Object.entries(value.removed ?? {})) {
      state.entities[kind] = (state.entities[kind] ?? []).filter(
        (record) => !ids.includes(record.id),
      );
    }
    for (const entry of value.paths ?? []) {
      if (typeof entry.end === "number") this.shots.delete(entry.id);
      else this.shots.set(entry.id, { ...this.shots.get(entry.id), ...entry });
    }
    if (typeof value.elapsed === "number") state.elapsed = value.elapsed;
    this.seq = value.seq;
    this.tick = value.tick;
    const events = value.events ?? [];
    if (events.length) this.eventCursor = events.at(-1).eventId ?? this.eventCursor;
    return { events, paths: value.paths ?? [] };
  }

  /** The viewer's tank as seen from seat `tankId`: `{ viewer: { position } }`. */
  render(tankId) {
    const tank = this.state?.entities.tanks.find((record) => record.id === tankId);
    if (!tank) throw new Error(`No tank ${tankId} in the mirror`);
    return { viewer: { ...tank } };
  }
}
