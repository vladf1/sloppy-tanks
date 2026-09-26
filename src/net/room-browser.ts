import { CONTENT_VERSION, roundMinutesReader } from "./protocol";
import { roomListReader, type RoomListing } from "./room-list";
import { mapMode, playerKind, team } from "./scene-codec";
import { preferredPlayerName, rememberPlayerName } from "./player-name";
import type { JoinChoice } from "./connection";
import type { RoomSelection } from "./pending-join";
import { MAP_OPTIONS } from "../game/map-options";
import type { VehicleKind } from "../game/types";

// Battle Setup loads this module alone before any other multiplayer code.
export { serverAddress } from "./server-address";
export { joinAfterReload } from "./pending-join";

const REFRESH_MS = 5000;
const LIST_TIMEOUT_MS = 8000;
const ROOM_PLAYERS = 8;
const ROOM_CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

function mapName(id: RoomListing["mapMode"]): string {
  return MAP_OPTIONS.find((map) => map.id === id)?.name ?? id;
}

function newRoomCode(): string {
  return [...crypto.getRandomValues(new Uint8Array(8))]
    .map((n) => ROOM_CODE_ALPHABET[n & 31])
    .join("");
}

/** A room link's room, and why the page came back to Battle Setup from it, if it did. */
export interface RoomLink {
  room: string;
  notice?: string;
}

/** Battle Setup's multiplayer tab. It polls the open-room list only while shown and
 * hands the chosen room to `enter` once. */
export class RoomBrowser {
  private readonly list: HTMLElement;
  private readonly message: HTMLElement;
  private readonly name: HTMLInputElement;
  private readonly join: HTMLButtonElement;
  private readonly refresh: HTMLButtonElement;
  private readonly endpoint: URL;
  private rooms: RoomListing[] = [];
  private selected = "";
  /** Shown instead of the usual prompt until the player picks a room. */
  private notice = "";
  private shown = false;
  private loading = false;
  private finished = false;
  private timer?: ReturnType<typeof setTimeout>;
  private controller?: AbortController;
  private readonly leave = () => this.close();

  constructor(
    private readonly panel: HTMLElement,
    address: URL,
    private readonly tank: () => VehicleKind,
    private readonly enter: (selection: RoomSelection) => void,
    private link?: RoomLink,
  ) {
    this.list = this.element("#room-list");
    this.message = this.element("#rooms-message");
    this.name = this.element("#player-name");
    this.join = this.element("#join-room");
    this.refresh = this.element("#refresh-rooms");
    this.endpoint = new URL("/rooms", address);
    this.endpoint.protocol = address.protocol === "wss:" ? "https:" : "http:";
    this.name.value ||= preferredPlayerName();
    // A setup copied from an earlier menu may still show that menu's rooms.
    this.render();
    this.message.textContent = "Looking for rooms…";
    this.refresh.addEventListener("click", () => void this.poll());
    this.join.addEventListener("click", () => this.joinSelected());
    const create = this.element<HTMLButtonElement>("#create-room");
    create.addEventListener("click", () => this.createRoom());
    create.disabled = false;
    window.addEventListener("pagehide", this.leave);
  }

  show(): void {
    if (!this.shown && !this.finished) {
      this.shown = true;
      void this.poll();
    }
  }

  hide(): void {
    this.shown = false;
    clearTimeout(this.timer);
    this.controller?.abort();
  }

  /** Stop for good: a room was chosen, the page is leaving, or the setup was discarded. */
  close(): void {
    this.finished = true;
    this.hide();
    window.removeEventListener("pagehide", this.leave);
  }

  private element<T extends HTMLElement>(selector: string): T {
    return this.panel.querySelector<T>(selector)!;
  }

  private checked(name: string): string {
    return this.element<HTMLInputElement>(`input[name="${name}"]:checked`).value;
  }

  private available(room: RoomListing): boolean {
    return room.reserved < ROOM_PLAYERS && room.contentVersion === CONTENT_VERSION;
  }

  private render(): void {
    const focused = (document.activeElement as HTMLInputElement | null)?.name === "room-choice";
    this.list.replaceChildren();
    this.element("#room-count").textContent = String(this.rooms.length);
    if (!this.rooms.some((room) => room.room === this.selected && this.available(room))) {
      this.selected = "";
    }
    for (const room of this.rooms) {
      const row = document.createElement("label");
      row.className = "room-row";
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "room-choice";
      radio.value = room.room;
      radio.checked = this.selected === room.room;
      radio.disabled = !this.available(room);
      radio.addEventListener("change", () => {
        this.selected = room.room;
        this.join.disabled = false;
        this.notice = "";
        this.message.textContent = "Choose a room to join the battle.";
      });
      const details = document.createElement("span");
      const title = document.createElement("strong");
      title.textContent = `${mapName(room.mapMode)} · ${room.players}/${ROOM_PLAYERS} players`;
      const info = document.createElement("small");
      const seconds = Math.ceil(room.time);
      const phase =
        room.phase === "playing"
          ? `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")} left · ${room.scores.join("–")}`
          : room.phase === "lobby"
            ? "In lobby"
            : "Between rounds";
      info.textContent =
        `${room.room} · ${room.humansOnly ? "Humans only" : "Bots: " + room.difficulty} · ${room.roundMinutes} min · ${phase}` +
        (room.reserved > room.players ? ` · ${room.reserved - room.players} reconnecting` : "") +
        (room.contentVersion !== CONTENT_VERSION
          ? " · Reload for updated game"
          : room.reserved >= ROOM_PLAYERS
            ? " · Full"
            : "");
      details.append(title, info);
      row.append(radio, details);
      this.list.append(row);
    }
    this.join.disabled = !this.selected;
    if (focused && this.selected) {
      this.list.querySelector<HTMLInputElement>(`input[value="${this.selected}"]`)?.focus();
    }
  }

  private async poll(): Promise<void> {
    clearTimeout(this.timer);
    if (!this.shown || this.loading) {
      return;
    }
    let delay = REFRESH_MS;
    if (!document.hidden) {
      this.loading = true;
      this.refresh.disabled = true;
      const controller = new AbortController();
      this.controller = controller;
      let timedOut = false;
      const timeout = setTimeout(() => {
        timedOut = true;
        controller.abort();
      }, LIST_TIMEOUT_MS);
      try {
        const response = await fetch(this.endpoint, {
          signal: controller.signal,
          cache: "no-store",
        });
        if (!response.ok) {
          throw new Error("Rooms unavailable. Try Refresh in a moment.");
        }
        this.rooms = roomListReader.read(await response.json()).rooms;
        const linked = this.link && this.followLink(this.link);
        this.render();
        if (linked) {
          this.list
            .querySelector(".room-row:has(input:checked)")
            ?.scrollIntoView({ block: "nearest" });
        }
        this.message.textContent =
          this.notice ||
          (this.rooms.length
            ? "Choose a room to join the battle."
            : "No rooms yet.\nStart a new room and invite your friends.");
      } catch (error) {
        if (controller.signal.aborted && !timedOut) {
          // Hidden mid-request; if the tab is back already, ask again at once.
          delay = 0;
        } else {
          this.rooms = [];
          this.render();
          this.message.textContent = timedOut
            ? "Room list timed out. Try Refresh."
            : error instanceof Error
              ? error.message
              : "Rooms unavailable. Try Refresh in a moment.";
        }
      } finally {
        clearTimeout(timeout);
        this.loading = false;
        this.refresh.disabled = false;
      }
    }
    if (this.shown) {
      this.timer = setTimeout(() => void this.poll(), delay);
    }
  }

  /** Select the linked room if it can take a player; otherwise say why not. */
  private followLink(link: RoomLink): boolean {
    this.link = undefined;
    const listing = this.rooms.find((room) => room.room === link.room);
    const open = !!listing && this.available(listing);
    if (open) {
      this.selected = link.room;
    }
    this.notice = [
      link.notice,
      !listing
        ? `Room ${link.room} isn't open. Choose another room or create one.`
        : !open
          ? `Room ${link.room} can't take another player right now.`
          : "",
    ]
      .filter(Boolean)
      .join("\n");
    return open;
  }

  private choice(): JoinChoice | undefined {
    const name = this.name.value.trim();
    this.name.setCustomValidity(name ? "" : "Enter a name to play.");
    if (!this.name.reportValidity()) {
      this.name.focus();
      return undefined;
    }
    rememberPlayerName(name);
    const side = this.checked("playerTeam");
    return {
      name,
      kind: playerKind.read(this.tank()),
      team: side === "auto" ? undefined : team.read(Number(side)),
    };
  }

  private joinSelected(): void {
    const choice = this.choice();
    const room = this.selected;
    if (
      !choice ||
      this.finished ||
      !this.rooms.some((listing) => listing.room === room && this.available(listing))
    ) {
      return;
    }
    this.close();
    this.enter({ room, choice: { ...choice, existingRoom: true } });
  }

  private createRoom(): void {
    const choice = this.choice();
    if (!choice || this.finished) {
      return;
    }
    const length = this.element<HTMLInputElement>("#create-round-minutes");
    if (!length.value || !length.reportValidity()) {
      length.focus();
      return;
    }
    const create = {
      mapMode: mapMode.read(this.checked("roomMap")),
      difficulty: "normal" as const,
      humansOnly: this.element<HTMLInputElement>("#create-humans-only").checked,
      roundMinutes: roundMinutesReader.read(Number(length.value)),
    };
    this.close();
    this.enter({ room: newRoomCode(), choice: { ...choice, create } });
  }
}
