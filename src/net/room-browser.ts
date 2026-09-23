import { CONTENT_VERSION, DEFAULT_ROUND_MINUTES, roundMinutesReader } from "./protocol";
import { roomListReader, type RoomListing } from "./room-list";
import { mapMode, playerKind, team } from "./scene-codec";
import { preferredPlayerName, rememberPlayerName } from "./player-name";
import type { JoinChoice } from "./connection";

const REFRESH_MS = 5000;
const MAP_NAMES = { village: "Pine Village", harbor: "Harbor Havoc", quarry: "Dusty Dig" };
export function browseRooms(
  root: HTMLElement,
  address: URL,
): Promise<{ room: string; choice: JoinChoice }> {
  root.classList.add("multiplayer");
  root.innerHTML = `
    <div id="overlay">
      <section class="menu network-menu room-browser" role="dialog" aria-modal="true" aria-labelledby="rooms-title">
        <header class="room-browser-header">
          <div><div class="eyebrow">PLAY WITH FRIENDS</div><h1 id="rooms-title">FIND A BATTLE</h1></div>
          <button id="back-single-player" class="room-text-button" type="button">← Single-player</button>
        </header>
        <div class="room-player-fields">
          <label>Your name<input id="player-name" maxlength="24" autocomplete="nickname" /></label>
          <label>Team<select id="player-team" title="Auto picks the team with fewer human players"><option value="auto">Auto</option><option value="0">Blue</option><option value="1">Red</option></select></label>
          <label>Your tank<select id="player-kind"><option value="scout">Scout</option><option value="balanced" selected>Balanced</option><option value="heavy">Heavy</option></select></label>
        </div>
        <div class="room-browser-columns">
          <section class="room-browse" aria-labelledby="open-rooms-title">
            <div class="room-list-heading"><h2 id="open-rooms-title">Open rooms <span id="room-count">0</span></h2><button id="refresh-rooms" class="room-text-button" type="button">↻ Refresh</button></div>
            <div class="room-list-body"><p id="rooms-message" role="status">Looking for rooms…</p><div id="room-list" role="radiogroup" aria-label="Choose a room"></div></div>
            <div class="room-join-footer"><span>Up to 8 players per room</span><button id="join-room" class="primary" type="button" disabled>Join room</button></div>
          </section>
          <section class="room-create" aria-labelledby="create-room-title">
            <h2 id="create-room-title">Start your own</h2>
            <p class="room-description">Pick a map and jump straight in.</p>
            <label class="room-map-field">Level<select id="create-map"><option value="village">Pine Village</option><option value="harbor">Harbor Havoc</option><option value="quarry">Dusty Dig</option></select></label>
            <label class="room-map-field room-length-field">Match length <span><input id="create-round-minutes" type="number" min="1" max="20" step="1" required value="${DEFAULT_ROUND_MINUTES}" /> minutes</span></label>
            <label class="room-bots-choice"><input id="create-humans-only" type="checkbox" checked /><span>Humans only<small>No bots in this battle</small></span></label>
            <button id="create-room" class="primary" type="button">Create room</button>
            <p class="room-create-note">Friends can join while you play.</p>
          </section>
        </div>
      </section>
    </div>`;

  const field = (id: string) => root.querySelector<HTMLInputElement | HTMLSelectElement>("#" + id)!;
  const name = field("player-name");
  name.value = preferredPlayerName();
  const list = root.querySelector<HTMLElement>("#room-list")!;
  const message = root.querySelector<HTMLElement>("#rooms-message")!;
  const join = root.querySelector<HTMLButtonElement>("#join-room")!;
  const refresh = root.querySelector<HTMLButtonElement>("#refresh-rooms")!;
  let selected = "";
  let rooms: RoomListing[] = [];
  let finished = false;
  let loading = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let controller: AbortController | undefined;
  const endpoint = new URL("/rooms", address);
  endpoint.protocol = address.protocol === "wss:" ? "https:" : "http:";
  const available = (room: RoomListing) =>
    room.reserved < 8 && room.contentVersion === CONTENT_VERSION;
  const render = () => {
    const focused = (document.activeElement as HTMLInputElement | null)?.name === "room-choice";
    list.replaceChildren();
    root.querySelector("#room-count")!.textContent = String(rooms.length);
    if (!rooms.some((room) => room.room === selected && available(room))) {
      selected = "";
    }
    for (const room of rooms) {
      const row = document.createElement("label");
      row.className = "room-row";
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "room-choice";
      radio.value = room.room;
      radio.checked = selected === room.room;
      radio.disabled = !available(room);
      radio.addEventListener("change", () => {
        selected = room.room;
        join.disabled = false;
      });
      const details = document.createElement("span");
      const title = document.createElement("strong");
      title.textContent = MAP_NAMES[room.mapMode] + " · " + room.players + "/8 players";
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
          : room.reserved >= 8
            ? " · Full"
            : "");
      details.append(title, info);
      row.append(radio, details);
      list.append(row);
    }
    join.disabled = !selected;
    if (focused && selected) {
      list.querySelector<HTMLInputElement>(`input[value="${selected}"]`)?.focus();
    }
  };
  const poll = async () => {
    clearTimeout(timer);
    if (finished || loading) {
      return;
    }
    if (!document.hidden) {
      loading = true;
      refresh.disabled = true;
      controller = new AbortController();
      const timeout = setTimeout(() => controller?.abort(), 8000);
      try {
        const response = await fetch(endpoint, { signal: controller.signal, cache: "no-store" });
        if (!response.ok) {
          throw new Error("Rooms unavailable. Try Refresh in a moment.");
        }
        const data = roomListReader.read(await response.json());
        if (finished) {
          return;
        }
        rooms = data.rooms;
        render();
        message.textContent = rooms.length
          ? "Choose a room to join the battle."
          : "No rooms yet.\nStart a battle and invite your friends.";
      } catch (error) {
        if (!finished) {
          rooms = [];
          render();
          message.textContent =
            error instanceof Error && error.name !== "AbortError"
              ? error.message
              : "Room list timed out. Try Refresh.";
        }
      } finally {
        clearTimeout(timeout);
        loading = false;
        refresh.disabled = false;
      }
    }
    if (!finished) {
      timer = setTimeout(() => void poll(), REFRESH_MS);
    }
  };
  const stop = () => {
    finished = true;
    clearTimeout(timer);
    controller?.abort();
    window.removeEventListener("pagehide", stop);
  };
  window.addEventListener("pagehide", stop, { once: true });
  refresh.addEventListener("click", () => void poll());
  root.querySelector("#back-single-player")!.addEventListener("click", () => {
    stop();
    const url = new URL(location.href);
    url.searchParams.delete("multiplayer");
    url.searchParams.delete("room");
    location.href = url.href;
  });
  return new Promise((resolve) => {
    const choose = (): JoinChoice | undefined => {
      const text = name.value.trim();
      if (!text) {
        message.textContent = "Enter a name to play.";
        name.focus();
        return;
      }
      rememberPlayerName(text);
      return {
        name: text,
        team:
          field("player-team").value === "auto"
            ? undefined
            : team.read(Number(field("player-team").value)),
        kind: playerKind.read(field("player-kind").value),
      };
    };
    join.addEventListener("click", () => {
      const choice = choose();
      if (!choice || finished || !rooms.some((room) => room.room === selected && available(room))) {
        return;
      }
      stop();
      resolve({ room: selected, choice: { ...choice, existingRoom: true } });
    });
    root.querySelector("#create-room")!.addEventListener("click", () => {
      const choice = choose();
      if (!choice || finished) {
        return;
      }
      const length = field("create-round-minutes") as HTMLInputElement;
      if (!length.value || !length.reportValidity()) {
        length.focus();
        return;
      }
      const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
      const room = [...crypto.getRandomValues(new Uint8Array(8))]
        .map((n) => alphabet[n & 31])
        .join("");
      const create = {
        mapMode: mapMode.read(field("create-map").value),
        difficulty: "normal" as const,
        humansOnly: root.querySelector<HTMLInputElement>("#create-humans-only")!.checked,
        roundMinutes: roundMinutesReader.read(Number(length.value)),
      };
      stop();
      resolve({ room, choice: { ...choice, create } });
    });
    void poll();
  });
}
