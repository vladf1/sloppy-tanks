import { DurableObject } from "cloudflare:workers";
import { RoomCatalog, type ListedRoom } from "./room-catalog";
import { roomListingReader } from "../src/net/room-list";
import { ROOM_CODE } from "../src/net/protocol";

export class RoomDirectory extends DurableObject<unknown> {
  private catalog = new RoomCatalog();
  constructor(ctx: DurableObjectState, env: unknown) {
    super(ctx, env);
    void ctx.blockConcurrencyWhile(async () => {
      this.catalog = new RoomCatalog(await ctx.storage.get<ListedRoom[]>("rooms"));
    });
  }
  override async fetch(request: Request): Promise<Response> {
    const now = Date.now();
    if (request.method === "PUT") {
      // Only PlayerRoom's binding can reach this internal write path.
      const entry = roomListingReader.read(await request.json());
      if (!ROOM_CODE.test(entry.room)) return new Response("Invalid room", { status: 400 });
      this.catalog.update(entry, now);
      await this.ctx.storage.put("rooms", this.catalog.saved(now));
      return new Response(null, { status: 204 });
    }
    if (request.method !== "GET") return new Response(null, { status: 405 });
    return Response.json({ rooms: this.catalog.list(now) });
  }
}
