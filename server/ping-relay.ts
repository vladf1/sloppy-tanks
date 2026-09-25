const MAX_PENDING_PINGS = 16;
// Both ends serialize with JSON.stringify and a leading `type`, so a prefix test
// skips parsing inputs, snapshots and lobby traffic.
const PING_PREFIX = '{"type":"ping"';
const PONG_PREFIX = '{"type":"pong"';
type RelaySocket = Pick<WebSocket, "addEventListener" | "send" | "close">;

/** Diagnostic relay: adds one-clock Worker-to-room RTT to gameplay pongs. */
export function relayRoomSocket(
  client: RelaySocket,
  room: RelaySocket,
  now: () => number = () => performance.now(),
): void {
  const pending: { t: number; started: number }[] = [];
  let closed = false;
  const close = (code: number, reason: string) => {
    if (closed) return;
    closed = true;
    // Reserved codes describe a missing close frame and cannot be sent.
    const wireCode = [1005, 1006, 1015].includes(code) ? 1011 : code;
    for (const socket of [client, room]) {
      try {
        socket.close(wireCode, reason);
      } catch {
        // The peer may have already completed its close handshake.
      }
    }
  };
  // Size, type and rate validation stay in the room, which already enforces them.
  client.addEventListener("message", (event) => {
    if (closed) return;
    const data = event.data;
    if (typeof data === "string" && data.startsWith(PING_PREFIX)) {
      const started = now();
      try {
        const t = JSON.parse(data).t;
        if (Number.isFinite(t)) {
          if (pending.length === MAX_PENDING_PINGS) pending.shift();
          pending.push({ t, started });
        }
      } catch {
        // The room answers malformed pings with its protocol error.
      }
    }
    try {
      room.send(data);
    } catch {
      close(1011, "Room relay failed");
    }
  });
  room.addEventListener("message", (event) => {
    if (closed) return;
    let data = event.data;
    if (pending.length > 0 && typeof data === "string" && data.startsWith(PONG_PREFIX)) {
      const received = now();
      const message = JSON.parse(data);
      const index = pending.findIndex((ping) => ping.t === message.t);
      if (index !== -1) {
        const [ping] = pending.splice(index, 1);
        data = JSON.stringify({ ...message, workerToRoomMs: Math.max(0, received - ping.started) });
      }
    }
    try {
      client.send(data);
    } catch {
      close(1011, "Client relay failed");
    }
  });
  for (const socket of [client, room]) {
    socket.addEventListener("close", (event) => close(event.code, event.reason));
    socket.addEventListener("error", () => close(1011, "WebSocket relay error"));
  }
}
