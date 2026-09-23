import { CONTENT_VERSION } from "./protocol";
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
  root.innerHTML =
    '<div id="overlay"><section class="menu compact network-menu room-browser" role="dialog" aria-modal="true" aria-labelledby="rooms-title"><div class="eyebrow">PLAY WITH FRIENDS</div><h1 id="rooms-title">FIND A BATTLE</h1><div class="network-choices"><label>Your name<input id="player-name" maxlength="24" autocomplete="nickname" /></label><label>Team<select id="player-team"><option value="auto">Auto · fewer humans</option><option value="0">Blue</option><option value="1">Red</option></select></label><label>Your tank<select id="player-kind"><option value="scout">Scout</option><option value="balanced" selected>Balanced</option><option value="heavy">Heavy</option></select></label></div><div class="room-list-heading"><h2>OPEN ROOMS</h2><button id="refresh-rooms" class="quiet" type="button">REFRESH</button></div><p id="rooms-message" role="status">Looking for rooms…</p><div id="room-list" role="radiogroup" aria-label="Choose a room"></div><button id="join-room" class="primary" disabled>JOIN SELECTED ROOM</button><div class="room-create"><h2>START YOUR OWN</h2><div class="network-choices"><label>Level<select id="create-map"><option value="village">Pine Village</option><option value="harbor">Harbor Havoc</option><option value="quarry">Dusty Dig</option></select></label><label class="network-toggle"><input id="create-humans-only" type="checkbox" checked />Humans only (no bots)</label></div><p class="network-help">Start playing immediately. Friends can join while you play.</p><button id="create-room" class="secondary">CREATE ROOM</button></div><button id="back-single-player" class="quiet">BACK TO SINGLE-PLAYER</button></section></div>';
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
        `${room.room} · ${room.humansOnly ? "Humans only" : "Bots: " + room.difficulty} · ${phase}` +
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
          : "No rooms yet. Create one and invite your friends.";
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
      const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
      const room = [...crypto.getRandomValues(new Uint8Array(8))]
        .map((n) => alphabet[n & 31])
        .join("");
      const create = {
        mapMode: mapMode.read(field("create-map").value),
        difficulty: "normal" as const,
        humansOnly: root.querySelector<HTMLInputElement>("#create-humans-only")!.checked,
      };
      stop();
      resolve({ room, choice: { ...choice, create } });
    });
    void poll();
  });
}
