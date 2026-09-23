import { MAX_LISTED_ROOMS, ROOM_LIST_TTL_MS, type RoomListing } from "../src/net/room-list";

export interface ListedRoom {
  entry: RoomListing;
  updatedAt: number;
}
/** Discovery metadata only; room authority and seat credentials never live here. */
export class RoomCatalog {
  private rooms: Map<string, ListedRoom>;
  constructor(saved: ListedRoom[] = []) {
    this.rooms = new Map(saved.slice(-MAX_LISTED_ROOMS).map((room) => [room.entry.room, room]));
  }
  update(entry: RoomListing, now: number): void {
    this.prune(now);
    this.rooms.delete(entry.room);
    if (!entry.players) return;
    if (this.rooms.size >= MAX_LISTED_ROOMS) {
      const oldest = [...this.rooms.values()].sort((a, b) => a.updatedAt - b.updatedAt)[0];
      this.rooms.delete(oldest.entry.room);
    }
    this.rooms.set(entry.room, { entry, updatedAt: now });
  }
  private prune(now: number): void {
    for (const [code, room] of this.rooms) {
      if (now - room.updatedAt >= ROOM_LIST_TTL_MS) this.rooms.delete(code);
    }
  }
  saved(now: number): ListedRoom[] {
    this.prune(now);
    return [...this.rooms.values()];
  }
  list(now: number): RoomListing[] {
    return this.saved(now)
      .map((room) => room.entry)
      .sort((a, b) => b.players - a.players || a.room.localeCompare(b.room));
  }
}
