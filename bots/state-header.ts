/**
 * The fixed header of a binary room-state message (`crates/core/src/net/replication.rs`):
 * a type byte, then LEB128 varints `roundId tick`, and for a snapshot `ack firstSeq count`.
 * The frames after it are deltas against the client's own copy of the state, so the bots,
 * which never mirror the scene, read only this far.
 */

/** `FULL_MESSAGE` and `SNAPSHOT_MESSAGE` in `crates/core/src/net/protocol.rs`. */
const FULL_MESSAGE = 1;
const SNAPSHOT_MESSAGE = 2;
/** Header varints stay safe integers; ten bytes would overflow 64 bits anyway. */
const MAX_VARINT_BYTES = 8;

export interface StateHeader {
  type: "full" | "snapshot";
  roundId: number;
  /** The baseline's tick, or the newest frame's in a snapshot. */
  tick: number;
  /** The newest input sequence the server applied (snapshots only). */
  ack?: number;
  /** How many frames the snapshot carries. */
  frames?: number;
}

/** The header of a binary state message; throws on a truncated or unknown one. */
export function readStateHeader(bytes: Uint8Array): StateHeader {
  let offset = 1;
  const varint = (): number => {
    let value = 0;
    let scale = 1;
    for (let count = 0; count < MAX_VARINT_BYTES; count++) {
      if (offset >= bytes.length) {
        throw new Error("Truncated state header");
      }
      const byte = bytes[offset++];
      value += (byte & 0x7f) * scale;
      if (byte < 0x80) {
        return value;
      }
      scale *= 0x80;
    }
    throw new Error("State header varint too long");
  };
  const kind = bytes[0];
  if (kind !== FULL_MESSAGE && kind !== SNAPSHOT_MESSAGE) {
    throw new Error(`Unknown state message ${kind}`);
  }
  const roundId = varint();
  const tick = varint();
  if (kind === FULL_MESSAGE) {
    return { type: "full", roundId, tick };
  }
  const ack = varint();
  varint(); // firstSeq
  return { type: "snapshot", roundId, tick, ack, frames: varint() };
}
