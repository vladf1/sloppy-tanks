// The engine build stamps the content version its joins send (scripts/build-wasm.mjs);
// rooms of another version can't take this page's players.
import { CONTENT_VERSION } from "../generated/engine/content-version.js";
import { busiestOpenRoom, readRoomList, type RoomListing } from "./room-list";
import { preferredPlayerName, rememberPlayerName } from "./player-name";
import { ROOM_SEATS, isPlayerKind, isRoundMinutes, type JoinChoice } from "./room-protocol";
import type { RoomSelection } from "./pending-join";
import { debugPage, isExtraLevel, mapOption } from "../game/map-options";
import { showRoomMap } from "../game/map-picker";
import { isPhone } from "../game/phone-mode";
import type { GameOptions } from "../game/game-options";

// Battle Setup loads this module alone before any other multiplayer code.
export { serverAddress } from "./server-address";
export { joinAfterReload } from "./pending-join";

const REFRESH_MS = 5000;
const LIST_TIMEOUT_MS = 8000;
const ROOM_CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

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
  private readonly status: HTMLElement;
  private readonly message: HTMLElement;
  private readonly hint: HTMLElement;
  private readonly name: HTMLInputElement;
  private readonly join: HTMLButtonElement;
  private readonly create: HTMLButtonElement;
  private readonly refresh: HTMLButtonElement;
  private readonly newRoom: HTMLInputElement;
  private readonly endpoint: URL;
  /** Extra-level rooms show only on a page offering those levels, or when linked. */
  private readonly extraLevels = debugPage(location.search);
  private readonly linkedRoom?: string;
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
  /** Phones show no room list: they join the busiest open room, or create one. */
  private readonly autoPick = isPhone();
  /** A room link chose the selected room. A phone keeps that choice; any other it picks
   * again on every refresh, as rooms fill, empty and finish their battles. */
  private linkChose = false;

  constructor(
    private readonly panel: HTMLElement,
    /** The shared map choice. An open room keeps its own map, so while one is chosen
     * the choice shows that map and waits. */
    private readonly mapChoice: HTMLElement,
    address: URL,
    /** The shared tank and map; a new room plays the map. */
    private readonly choices: () => Pick<GameOptions, "humanKind" | "mapMode">,
    private readonly enter: (selection: RoomSelection) => void,
    private link?: RoomLink,
  ) {
    this.list = this.element("#room-list");
    this.status = this.element(".room-status");
    this.message = this.element("#rooms-message");
    this.hint = this.element("#rooms-hint");
    this.name = this.element("#player-name");
    this.join = this.element("#join-room");
    this.create = this.element("#create-room");
    this.refresh = this.element("#refresh-rooms");
    this.newRoom = this.element("#new-room");
    this.linkedRoom = link?.room;
    this.endpoint = new URL("/rooms", address);
    this.endpoint.protocol = address.protocol === "wss:" ? "https:" : "http:";
    // The plain list, which the traffic bots read too, leaves extra-level rooms out.
    this.endpoint.searchParams.set("debug", "");
    this.name.value ||= preferredPlayerName();
    if (this.autoPick) {
      // A phone offers no room rules, and an empty arena is no battle.
      this.element<HTMLInputElement>("#create-humans-only").checked = false;
    }
    // A setup copied from an earlier menu may still show that menu's rooms.
    this.render();
    this.showStatus("loading", "Looking for rooms…", "Pick your tank and map meanwhile.");
    this.refresh.addEventListener("click", () => void this.poll());
    this.join.addEventListener("click", () => this.joinSelected());
    this.create.addEventListener("click", () => this.createRoom());
    // A phone's one action creates a room only when it knows none is open, so it waits
    // for the first room list; elsewhere the new room is a choice beside the list.
    this.create.disabled = this.autoPick;
    this.newRoom.addEventListener("change", () => this.choose(""));
    // Editing the new room's rules chooses the new room.
    this.element(".room-rules").addEventListener("focusin", () => {
      if (!this.newRoom.checked) {
        this.newRoom.checked = true;
        this.choose("");
      }
    });
    window.addEventListener("pagehide", this.leave);
  }

  show(): void {
    if (!this.shown && !this.finished) {
      this.shown = true;
      this.lockMap();
      void this.poll();
    }
  }

  hide(): void {
    this.shown = false;
    this.lockMap();
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
    return room.reserved < ROOM_SEATS && room.contentVersion === CONTENT_VERSION;
  }

  private render(): void {
    const focused = (document.activeElement as HTMLInputElement | null)?.name === "room-choice";
    this.list.replaceChildren();
    this.element("#room-count").textContent = String(this.rooms.length);
    if (!this.rooms.some((room) => room.room === this.selected && this.available(room))) {
      this.selected = "";
      this.linkChose = false;
    }
    if (this.autoPick && !this.linkChose) {
      this.selected = busiestOpenRoom(this.rooms, (room) => this.available(room))?.room ?? "";
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
      radio.addEventListener("change", () => this.choose(room.room));
      const details = document.createElement("span");
      const title = document.createElement("strong");
      title.append(mapOption(room.mapMode)?.name ?? room.mapMode);
      if (isExtraLevel(room.mapMode)) {
        const badge = document.createElement("em");
        badge.className = "level-badge";
        badge.textContent = "EXTRA";
        title.append(badge);
      }
      title.append(` · ${room.players}/${ROOM_SEATS} players`);
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
          : room.reserved >= ROOM_SEATS
            ? " · Full"
            : "");
      details.append(title, info);
      row.append(radio, details);
      this.list.append(row);
    }
    this.join.disabled = !this.selected;
    this.newRoom.checked = !this.selected;
    this.lockMap();
    if (focused && this.selected) {
      this.list.querySelector<HTMLInputElement>(`input[value="${this.selected}"]`)?.focus();
    }
  }

  /** `room` is an open room's code, or "" for the new room. */
  private choose(room: string): void {
    this.selected = room;
    this.join.disabled = !room;
    this.lockMap();
    this.notice = "";
    this.showPrompt();
  }

  /** The map applies to single player and a new room only. */
  private lockMap(): void {
    const room = this.shown
      ? this.rooms.find((listing) => listing.room === this.selected)
      : undefined;
    this.mapChoice.inert = !!room;
    showRoomMap(this.mapChoice, room?.mapMode);
  }

  /** The status well's line, its hint and its bar: "loading" sweeps, "ready" is full. */
  private showStatus(state: "loading" | "ready" | "error", message: string, hint = ""): void {
    this.status.dataset.state = state;
    this.message.textContent = message;
    this.hint.textContent = hint;
  }

  /** What the action button will do, or the notice that brought the player here. */
  private showPrompt(): void {
    if (this.notice) {
      this.showStatus("ready", this.notice);
    } else if (this.selected) {
      this.showStatus("ready", `Ready to join room ${this.selected}`, "Hit JOIN ROOM to play.");
    } else {
      this.showStatus(
        "ready",
        "Ready to create a room",
        this.rooms.length ? "Or pick an open room to join." : "Share its link to invite friends.",
      );
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
          throw new Error("Rooms unavailable");
        }
        this.rooms = readRoomList(await response.json()).filter(
          (room) =>
            this.extraLevels || !isExtraLevel(room.mapMode) || room.room === this.linkedRoom,
        );
        const linked = this.link && this.followLink(this.link);
        this.render();
        this.create.disabled = false;
        if (linked) {
          this.list
            .querySelector(".room-row:has(input:checked)")
            ?.scrollIntoView({ block: "nearest" });
        }
        this.showPrompt();
      } catch (error) {
        if (controller.signal.aborted && !timedOut) {
          // Hidden mid-request; if the tab is back already, ask again at once.
          delay = 0;
        } else {
          this.rooms = [];
          this.render();
          // Without a list, a new room is the one thing a phone can still do.
          this.create.disabled = false;
          // fetch() rejects with a TypeError when the server cannot be reached at all.
          this.showStatus(
            "error",
            timedOut
              ? "Room list timed out"
              : error instanceof TypeError
                ? "Can't reach the game server"
                : error instanceof Error
                  ? error.message
                  : "Rooms unavailable",
            "Trying again in a moment.",
          );
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
      this.linkChose = true;
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
    const kind = this.choices().humanKind;
    if (!isPlayerKind(kind)) {
      return undefined;
    }
    return { name, kind, team: side === "0" ? 0 : side === "1" ? 1 : undefined };
  }

  private joinSelected(): void {
    const choice = this.choice();
    const room = this.selected;
    const listing = this.rooms.find((listing) => listing.room === room);
    if (!choice || this.finished || !listing || !this.available(listing)) {
      return;
    }
    this.close();
    // The join screen keeps the map inert and showing the room's map while it loads.
    showRoomMap(this.mapChoice, listing.mapMode);
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
    const map = mapOption(this.choices().mapMode);
    const roundMinutes = Number(length.value);
    if (!map || !isRoundMinutes(roundMinutes)) {
      return;
    }
    const create = {
      mapMode: map.id,
      // Phones play their bots on Easy, as in single player.
      difficulty: this.autoPick ? ("easy" as const) : ("normal" as const),
      humansOnly: this.element<HTMLInputElement>("#create-humans-only").checked,
      roundMinutes,
    };
    this.close();
    this.enter({ room: newRoomCode(), choice: { ...choice, create } });
  }
}
