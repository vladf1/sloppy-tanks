import { DurableObject } from "cloudflare:workers";
import type { RoomListing } from "../src/net/room-list";
import type { RoomDirectory } from "./room-directory";
import { RoomSession } from "./room-session";

interface Env {
  DIRECTORY: DurableObjectNamespace<RoomDirectory>;
}
export class PlayerRoom extends DurableObject<Env> {
  private session?: RoomSession<WebSocket>;
  private listing?: RoomListing;
  private directoryPublishing = false;
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    for (const socket of ctx.getWebSockets()) {
      try {
        socket.send(
          JSON.stringify({
            type: "room-reset",
            roomEpoch: crypto.randomUUID(),
            reason: "runtime-restart",
          }),
        );
        socket.close(1012, "Room restarted");
      } catch {
        /* Disconnected while the object was evicted. */
      }
    }
  }
  override fetch(request: Request): Response {
    this.session ??= new RoomSession(new URL(request.url).pathname.split("/").at(-1)!, {
      listing: (entry) => this.publishDirectory(entry),
    });
    if (this.session.full) return new Response("Room connection limit", { status: 429 });
    const pair = new WebSocketPair(),
      socket = pair[1];
    this.ctx.acceptWebSocket(socket);
    this.session.accept(socket);
    return new Response(null, { status: 101, webSocket: pair[0] });
  }
  private publishDirectory(entry: RoomListing): void {
    if (!this.env.DIRECTORY) return;
    this.listing = entry;
    if (this.directoryPublishing) return;
    this.directoryPublishing = true;
    this.ctx.waitUntil(
      (async () => {
        try {
          // Coalesce lobby churn instead of appending a queue of stale listings.
          while (this.listing) {
            const body = JSON.stringify(this.listing);
            this.listing = undefined;
            const response = await this.env.DIRECTORY.getByName("rooms").fetch(
              "https://directory/rooms",
              { method: "PUT", body },
            );
            if (!response.ok) throw new Error("Room directory update failed");
          }
        } catch (error) {
          console.error("Room listing unavailable", error);
        } finally {
          this.directoryPublishing = false;
        }
      })(),
    );
  }
  override webSocketMessage(socket: WebSocket, message: string | ArrayBuffer): void {
    this.session?.message(socket, message);
  }
  override webSocketClose(socket: WebSocket, code: number, reason: string): void {
    this.session?.closed(socket, code);
    // Hibernating sockets require the server half of the closing handshake.
    // Without it browsers can remain CLOSING until their seat reservation expires.
    try {
      socket.close(code === 1005 || code === 1006 ? 1000 : code, reason);
    } catch {
      /* The transport may already be gone. */
    }
  }
  override webSocketError(socket: WebSocket): void {
    this.session?.failed(socket);
  }
}
