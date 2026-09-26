import {
  createServer as createHttpServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import type { AddressInfo } from "node:net";
import type { Duplex } from "node:stream";
import { WebSocket, WebSocketServer } from "ws";
import { CONTENT_VERSION, PROTOCOL_VERSION, ROOM_CODE } from "../src/net/protocol";
import { RoomCatalog } from "./room-catalog";
import { RoomSession, type RoomSocket } from "./room-session";
import { ServerMonitor } from "./monitor";
import { RateLimit } from "./rate-limit";

/** Headroom over MAX_CLIENT_MESSAGE_BYTES; RoomSession enforces the exact protocol limit. */
const MAX_FRAME_BYTES = 8192;
/** Queued output after which a reader is too slow to follow 20 Hz snapshots. */
const MAX_BUFFERED_BYTES = 2_000_000;
/** Time given to clients to finish closing handshakes when the server stops. */
const SHUTDOWN_GRACE_MS = 1000;
/**
 * Live rooms per process. A busy room measured about 3–4 ms of each 50 ms tick on the
 * one-vCPU VPS, so ten busy rooms use roughly two thirds of it, leaving headroom for
 * garbage collection and bursts. Raise it with MAX_ROOMS on a larger host. Joining an
 * existing room is never refused by this cap.
 */
export const DEFAULT_MAX_ROOMS = 10;
/** Concurrent sockets from one IP; covers a 32-bot traffic swarm sharing an egress address. */
export const DEFAULT_MAX_SOCKETS_PER_IP = 32;

export interface ServerOptions {
  allowedOrigins: string[];
  /** Take the client IP from the last X-Forwarded-For hop (the local reverse proxy). */
  trustProxy: boolean;
  maxRooms?: number;
  maxSocketsPerIp?: number;
  /** Room lifecycle and summary lines; defaults to console.log (the systemd journal). */
  log?: (line: string) => void;
}
export interface MultiplayerServer {
  listen(port: number, host: string): Promise<AddressInfo>;
  close(): Promise<void>;
  readonly rooms: ReadonlyMap<string, RoomSession>;
  readonly monitor: ServerMonitor;
}

/** Multiplayer host: /health, /rooms, the /room/CODE WebSocket, and loopback-only /stats. */
export function createServer(options: ServerOptions): MultiplayerServer {
  const rooms = new Map<string, RoomSession<NodeSocket>>(),
    maxRooms = options.maxRooms ?? DEFAULT_MAX_ROOMS,
    maxSocketsPerIp = options.maxSocketsPerIp ?? DEFAULT_MAX_SOCKETS_PER_IP,
    socketsByIp = new Map<string, number>(),
    catalog = new RoomCatalog(),
    connectionRate = new RateLimit(60),
    entryRate = new RateLimit(120),
    directoryRate = new RateLimit(120),
    sockets = new WebSocketServer({
      noServer: true,
      maxPayload: MAX_FRAME_BYTES,
      perMessageDeflate: false,
    }),
    http = createHttpServer((request, response) => handle(request, response)),
    monitor = new ServerMonitor(
      {
        samples: () => [...rooms.values()].flatMap((session) => session.sample() ?? []),
        sockets: () => sockets.clients.size,
      },
      options.log,
    );
  const allowed = (request: IncomingMessage) =>
    options.allowedOrigins.includes(request.headers.origin ?? "");
  const clientIp = (request: IncomingMessage) => {
    const forwarded = request.headers["x-forwarded-for"];
    const hops = (Array.isArray(forwarded) ? forwarded.join(",") : (forwarded ?? "")).split(",");
    const proxied = options.trustProxy ? hops.at(-1)?.trim() : "";
    return proxied || request.socket.remoteAddress || "local";
  };

  function handle(request: IncomingMessage, response: ServerResponse): void {
    const path = new URL(request.url ?? "/", "http://host").pathname,
      origin = request.headers.origin ?? "",
      cors = { "Access-Control-Allow-Origin": origin, "Cache-Control": "no-store", Vary: "Origin" };
    const reply = (status: number, body?: unknown, headers: Record<string, string> = {}) => {
      const json = body !== undefined && typeof body !== "string";
      response.writeHead(status, {
        ...headers,
        ...(body === undefined ? {} : { "Content-Type": json ? "application/json" : "text/plain" }),
      });
      response.end(body === undefined ? undefined : json ? JSON.stringify(body) : body);
    };
    if (path === "/" || path === "/health") {
      // Pretty-printed because operators read it in a browser; /rooms stays compact.
      const health = { version: PROTOCOL_VERSION, contentVersion: CONTENT_VERSION };
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify(health, null, 2) + "\n");
      return;
    }
    if (path === "/stats") {
      // Operator view with every room code, including unlisted ones: only a direct local
      // request qualifies. Caddy also refuses the path, and proxied requests carry XFF.
      const local = ["127.0.0.1", "::1", "::ffff:127.0.0.1"].includes(
        request.socket.remoteAddress ?? "",
      );
      if (!local || request.headers["x-forwarded-for"]) return reply(404, "Not found");
      return reply(200, monitor.latest ?? monitor.sample(), { "Cache-Control": "no-store" });
    }
    if (path === "/rooms") {
      if (!allowed(request)) return reply(403, "Origin not allowed");
      if (request.method !== "GET") return reply(405, undefined, cors);
      if (!directoryRate.allow(clientIp(request), Date.now()))
        return reply(429, "Too many refreshes; try again shortly", cors);
      return reply(200, { rooms: catalog.list(Date.now()) }, cors);
    }
    const room = /^\/room\/([^/]+)$/.exec(path)?.[1];
    if (room && ROOM_CODE.test(room)) {
      if (!allowed(request)) return reply(403, "Origin not allowed");
      return reply(426, "WebSocket required");
    }
    reply(404, "Not found");
  }

  http.on("upgrade", (request: IncomingMessage, stream: Duplex, head: Buffer) => {
    const refuse = (status: number, text: string) => {
      stream.end(
        `HTTP/1.1 ${status} ${text}\r\nConnection: close\r\nContent-Type: text/plain\r\n` +
          `Content-Length: ${Buffer.byteLength(text)}\r\n\r\n${text}`,
      );
    };
    const path = new URL(request.url ?? "/", "http://host").pathname,
      code = /^\/room\/([^/]+)$/.exec(path)?.[1];
    if (!code || !ROOM_CODE.test(code)) return refuse(404, "Not Found");
    if (!allowed(request)) return refuse(403, "Origin not allowed");
    const now = Date.now(),
      ip = clientIp(request);
    if (!connectionRate.allow(ip, now) || !entryRate.allow("rooms", now))
      return refuse(429, "Too many room connections; try again shortly");
    if ((socketsByIp.get(ip) ?? 0) >= maxSocketsPerIp)
      return refuse(429, "Too many open connections from this address");
    const existing = rooms.get(code);
    if (existing?.full) return refuse(429, "Room connection limit");
    if (!existing && rooms.size >= maxRooms) return refuse(503, "Server is full; try again later");
    sockets.handleUpgrade(request, stream, head, (socket) => admit(code, ip, socket));
  });

  function admit(code: string, ip: string, socket: WebSocket): void {
    let session = rooms.get(code);
    // Handshakes finish asynchronously, so a burst can pass the upgrade check together.
    if (!session && rooms.size >= maxRooms) {
      socket.close(1013, "Server is full");
      return;
    }
    if (!session) {
      const created = new RoomSession<NodeSocket>(code, {
        listing: (entry) => catalog.update(entry, Date.now()),
        activity: (event) => monitor.activity(code, event),
        ended: (reason, ageMs) => {
          monitor.ended(code, reason, ageMs);
          if (rooms.get(code) === created) rooms.delete(code);
        },
      });
      rooms.set(code, created);
      session = created;
    }
    const adapter = new NodeSocket(socket);
    if (!session.accept(adapter)) {
      socket.close(1013, "Room connection limit");
      return;
    }
    const owner = session;
    socketsByIp.set(ip, (socketsByIp.get(ip) ?? 0) + 1);
    socket.on("message", (data: Buffer, binary: boolean) =>
      owner.message(adapter, binary ? new ArrayBuffer(0) : data.toString("utf8")),
    );
    socket.on("close", (closeCode: number) => {
      const open = (socketsByIp.get(ip) ?? 1) - 1;
      if (open > 0) socketsByIp.set(ip, open);
      else socketsByIp.delete(ip);
      owner.closed(adapter, closeCode);
    });
    socket.on("error", () => owner.failed(adapter));
  }

  return {
    rooms,
    monitor,
    listen: (port, host) =>
      new Promise((resolve, reject) => {
        http.once("error", reject);
        http.listen(port, host, () => {
          monitor.start();
          resolve(http.address() as AddressInfo);
        });
      }),
    async close() {
      monitor.stop();
      http.close();
      for (const session of [...rooms.values()]) session.reset("server-restart");
      const deadline = Date.now() + SHUTDOWN_GRACE_MS;
      while (sockets.clients.size && Date.now() < deadline)
        await new Promise((done) => setTimeout(done, 20));
      for (const socket of sockets.clients) socket.terminate();
      sockets.close();
      http.closeAllConnections();
    },
  };
}

/** Adapts a `ws` socket to RoomSession and sheds readers whose output queue keeps growing. */
class NodeSocket implements RoomSocket {
  constructor(private readonly socket: WebSocket) {}
  send(text: string): void {
    if (this.socket.readyState !== WebSocket.OPEN) return;
    if (this.socket.bufferedAmount > MAX_BUFFERED_BYTES) {
      this.socket.close(4002, "Slow reader");
      return;
    }
    this.socket.send(text);
  }
  close(code: number, reason: string): void {
    if (this.socket.readyState === WebSocket.OPEN) this.socket.close(code, reason);
  }
}
