import { hudMarkup } from "../game/ui-markup";
import { SettingsDialog } from "../game/settings-dialog";
import { AMMO_ORDER } from "../game/ammo-options";
import {
  deathCause,
  effectsLabel,
  killFeedNames,
  rankTitle,
  showFeedRow,
} from "../game/hud-feedback";
import { isExtraLevel, MAP_OPTIONS, mapOption, showsExtraLevels } from "../game/map-options";
import type {
  EngineEvent,
  HumanState,
  MatchState,
  PlayerVehicleKind,
  Team,
  Weapon,
} from "../game/engine-api";
import {
  DEFAULT_ROUND_MINUTES,
  MAX_ROUND_MINUTES,
  PLAYER_KINDS,
  isPlayerKind,
  type ConnectionEnd,
  type EndCause,
  type JoinChoice,
  type Lobby,
  type Player,
} from "./room-protocol";
import "./multiplayer.css";

/** The engine's HUD record for the viewer's tank and the scoreboard. The engine works
 * out health colours, ranks and ammo, so the page only displays them. */
export interface Hud {
  match: MatchState;
  elapsed: number;
  human: HumanState;
  scoreboard: { id: number; name: string; team: Team; kills: number; deaths: number }[];
}
/** A displayed event with the viewer-relative flags (`drain_events()`). */
export type HudEvent = EngineEvent;

/** Battle Setup's tank cards (`battle-setup.html`); the engine owns the vehicles' stats. */
const TANK_LABELS: Record<PlayerVehicleKind, { name: string; tag: string }> = {
  scout: { name: "SKIPPER", tag: "Light scout" },
  balanced: { name: "BRUISER", tag: "Balanced tank" },
  heavy: { name: "BIG RIG", tag: "Heavy tank" },
};
const TEAM_NAMES = ["BLUE", "RED"] as const;

/** Tanks per team, bots included, on a standard map; extra levels name their own. */
const STANDARD_TEAM_TANKS = 6;

/** The in-room menu. Players choose their name, and first team and tank, on Battle Setup. */
export interface NetworkActions {
  choose(choice: Pick<JoinChoice, "team" | "kind">): void;
  settings(map: string, difficulty: string, humansOnly: boolean, roundMinutes: number): void;
  start(): void;
  pause(): void;
  resume(): void;
  end(): void;
  leave(): void;
  /** Join this room again after the connection ended. */
  rejoin(): void;
  /** Battle Setup with this room selected, `notice` saying why. */
  setup(notice: string): void;
  ammo(weapon: Weapon): void;
  volume(value: number): void;
  /** The touch preference Settings shows, and the one they save. */
  touchMode(): string;
  setTouchMode(mode: string): void;
}
/** The heading of each connection end, and the retry it offers besides Battle Setup. */
const ENDINGS: Record<EndCause, { title: string; retry?: string }> = {
  lost: { title: "CONNECTION LOST", retry: "TRY AGAIN" },
  rejected: { title: "DISCONNECTED", retry: "TRY AGAIN" },
  "seat-expired": { title: "SEAT EXPIRED", retry: "JOIN AGAIN" },
  "other-tab": { title: "PLAYING IN ANOTHER TAB", retry: "PLAY HERE" },
  "room-ended": { title: "ROOM CLOSED" },
  outdated: { title: "GAME UPDATED" },
  renderer: { title: "RENDERER STOPPED" },
};
/** Battle Setup's card name, such as "Big Rig". */
function tankName(kind: PlayerVehicleKind): string {
  return TANK_LABELS[kind].name
    .split(" ")
    .map((word) => word[0] + word.slice(1).toLowerCase())
    .join(" ");
}
const CONTROLS_HELP = [
  "WASD / arrows: drive",
  "Mouse: aim",
  "Hold left click: fire",
  "Right click: mine",
  "Q / E or 1–5: ammo",
  "Esc: menu",
];
const MENU_MARKUP = `<section class="menu network-menu" aria-labelledby="network-title">
  <header class="dialog-head">
    <div class="dialog-eyebrow"><span class="eyebrow">ROOM <b class="room-code"></b></span><button id="copy-room" class="text-button" type="button">Copy invite link</button></div>
    <div class="dialog-title"><h2 id="network-title"></h2><div id="network-score" class="dialog-score" aria-label="Final score" hidden><span id="final-blue" class="blue"></span> : <span id="final-red" class="red"></span></div></div>
    <p id="network-hint" class="dialog-lede"></p>
    <p id="network-message" role="status"></p>
  </header>
  <section class="network-next" aria-labelledby="next-label">
    <div id="next-label" class="network-label">NEXT BATTLE</div>
    <div class="network-line">
      <p id="network-summary" class="network-summary" aria-label="Room rules"></p>
      <button id="change-rules" class="text-button" type="button" aria-controls="host-settings" aria-expanded="false">Change rules</button>
    </div>
    <div id="host-settings" class="network-fields" hidden>
      <label id="room-map-field">Map<select id="room-map">${MAP_OPTIONS.filter(
        (map) => !("extra" in map),
      )
        .map((map) => `<option value="${map.id}">${map.name}</option>`)
        .join("")}</select></label>
      <label>Bots<select id="room-bots"><option value="easy">Easy</option><option value="normal">Normal</option><option value="hard">Hard</option><option value="none">None</option></select></label>
      <label>Minutes<input id="room-round-minutes" type="number" min="1" max="${MAX_ROUND_MINUTES}" step="1" required /></label>
    </div>
    <div id="choice-line" class="network-line">
      <span>You: <b id="next-choice"></b></span>
      <button id="change-choice" class="text-button" type="button" aria-controls="player-fields" aria-expanded="false">Change team or tank</button>
    </div>
    <div id="player-fields" class="network-fields" hidden>
      <label>Team<select id="player-team"><option value="auto">Auto · fewer humans</option><option value="0">Blue</option><option value="1">Red</option></select></label>
      <label>Tank<select id="player-kind">${PLAYER_KINDS.map((kind) => `<option value="${kind}">${tankName(kind)} · ${TANK_LABELS[kind].tag.toLowerCase()}</option>`).join("")}</select></label>
    </div>
  </section>
  <div id="network-scoreboard" hidden></div>
  <div id="network-roster"></div>
  <div class="network-actions">
    <button id="start-match" class="primary" type="button" hidden>START BATTLE</button>
    <button id="network-resume" class="primary" type="button" hidden>RESUME</button>
    <button id="network-end" class="secondary danger" type="button" hidden>END BATTLE</button>
    <button id="leave-room" class="secondary" type="button">LEAVE ROOM</button>
  </div>
  <p id="network-leave-note" class="network-note" hidden>You're the last player here, so leaving closes the room.</p>
  <p class="network-help">${CONTROLS_HELP.map((item) => `<span>${item}</span>`).join(" ")}</p>
</section>
<section class="menu network-connection" role="alertdialog" aria-labelledby="connection-title" aria-describedby="connection-message" hidden>
  <header class="dialog-head">
    <div class="dialog-eyebrow"><span class="eyebrow">ROOM <b class="room-code"></b></span></div>
    <div class="dialog-title"><h2 id="connection-title"></h2></div>
    <p id="connection-message" class="dialog-lede"></p>
  </header>
  <div class="startup-track" aria-hidden="true"><span></span></div>
  <div class="network-actions">
    <button id="connection-retry" class="primary" type="button"></button>
    <button id="connection-setup" type="button"></button>
    <button id="connection-leave" class="secondary" type="button">LEAVE ROOM</button>
  </div>
</section>`;

export class NetworkUI {
  readonly canvas: HTMLCanvasElement;
  readonly panel: HTMLElement;
  private lastLobby?: Lobby;
  private playerRows = new Map<number, HTMLElement>();
  private feed: { names: string[]; time: number }[] = [];
  private toastTime = 0;
  private hurtTime = 0;
  private deathCause = "";
  private isJoined = false;
  /** Whether the socket is live, being (re)connected, or has given up. */
  private link: "live" | "connecting" | ConnectionEnd = "connecting";
  /** The finished round, read from the final replicated state. */
  private outcome?: { match: MatchState; team: number };
  /** The round this host ended early; its seat had the menu open, so no final state came. */
  private endedRound?: number;
  /** Between battles the rules and your team and tank stay folded until asked for. */
  private editingRules = false;
  private editingChoice = false;
  private canEditRules = false;
  menu = false;
  constructor(
    readonly root: HTMLElement,
    room: string,
    private readonly actions: NetworkActions,
  ) {
    root.classList.add("multiplayer");
    root.innerHTML =
      '<canvas id="game" tabindex="0" aria-label="Multiplayer tank arena"></canvas>' +
      hudMarkup() +
      `<div id="network-status" role="status"></div>
      <div id="network-respawn" hidden>
        <h2 id="network-respawn-count"></h2>
        <p id="network-death-cause" role="status"></p>
      </div>`;
    this.canvas = root.querySelector("canvas")!;
    this.panel = root.querySelector("#overlay")!;
    this.panel.innerHTML = MENU_MARKUP;
    this.panel.querySelectorAll(".room-code").forEach((code) => (code.textContent = room));
    this.input("room-round-minutes").value = String(DEFAULT_ROUND_MINUTES);
    const players = document.createElement("aside");
    players.id = "network-players";
    players.hidden = true;
    players.setAttribute("aria-label", "Players and kills");
    this.root.querySelector("#hud")!.append(players);
    this.root.querySelector("#feed")!.setAttribute("aria-live", "polite");
    // Settings open only during play (the corner button hides behind menus): they hand
    // the tank to a bot while the battle keeps playing, and closing them takes it back.
    let resumeAfterSettings = false;
    new SettingsDialog(root, "MULTIPLAYER", {
      touchMode: () => this.actions.touchMode(),
      setTouchMode: (mode) => this.actions.setTouchMode(mode),
      setVolume: (value) => this.actions.volume(value),
      opened: () => {
        resumeAfterSettings =
          this.lastLobby?.phase === "playing" && this.link === "live" && !this.menu;
        if (resumeAfterSettings) {
          this.actions.pause();
        }
      },
      closed: () => {
        if (resumeAfterSettings && this.menu) {
          this.actions.resume();
        }
        resumeAfterSettings = false;
      },
    });
    this.on("start-match", () => this.actions.start());
    this.on("network-end", () => {
      this.endedRound = this.lastLobby?.roundId;
      this.actions.end();
    });
    this.on("network-resume", () => this.actions.resume());
    this.on("leave-room", () => this.actions.leave());
    this.on("connection-leave", () => this.actions.leave());
    this.on("connection-retry", () => {
      this.showConnecting("RECONNECTING", "Connecting to the room…");
      this.actions.rejoin();
    });
    this.on("connection-setup", () => {
      if (typeof this.link === "object") {
        this.actions.setup(this.link.text);
      }
    });
    this.on("change-rules", () => {
      this.editingRules = !this.editingRules;
      this.renderEditing();
    });
    this.on("change-choice", () => {
      this.editingChoice = !this.editingChoice;
      this.renderEditing();
    });
    this.on("pause", () => this.actions.pause());
    this.on("fullscreen", () => {
      if (document.fullscreenElement) {
        void document.exitFullscreen();
      } else {
        void root.requestFullscreen().catch(() => {});
      }
    });
    for (const weapon of AMMO_ORDER) {
      this.on("ammo-" + weapon, () => this.actions.ammo(weapon));
    }
    for (const field of ["player-team", "player-kind"]) {
      this.root.querySelector("#" + field)!.addEventListener("change", () => {
        if (this.isJoined) {
          this.actions.choose(this.choice());
        }
      });
    }
    for (const field of ["room-map", "room-bots", "room-round-minutes"]) {
      this.root.querySelector("#" + field)!.addEventListener("change", () => {
        const length = this.input("room-round-minutes") as HTMLInputElement;
        if (!this.lastLobby || !length.reportValidity()) {
          return;
        }
        const bots = this.input("room-bots").value;
        // "None" keeps the last difficulty for when bots come back.
        this.actions.settings(
          this.input("room-map").value,
          bots === "none" ? this.lastLobby.settings.difficulty : bots,
          bots === "none",
          Number(length.value),
        );
      });
    }
    this.on("copy-room", () => {
      void navigator.clipboard
        .writeText(location.href)
        .then(() => this.notice("Invite link copied. Send it to your friends."))
        .catch(() => this.notice("Copy the address from your browser to invite friends."));
    });
  }
  private input(name: string): HTMLInputElement | HTMLSelectElement {
    return this.root.querySelector<HTMLInputElement | HTMLSelectElement>("#" + name)!;
  }
  private button(name: string): HTMLButtonElement {
    return this.root.querySelector<HTMLButtonElement>("#" + name)!;
  }
  private element(name: string): HTMLElement {
    return this.root.querySelector<HTMLElement>("#" + name)!;
  }
  private on(name: string, action: () => void): void {
    this.root.querySelector("#" + name)!.addEventListener("click", action);
  }
  private set(name: string, text: string): void {
    const node = this.root.querySelector("#" + name);
    if (node && node.textContent !== text) {
      node.textContent = text;
    }
  }
  private choice(): Pick<JoinChoice, "team" | "kind"> {
    const side = this.input("player-team").value;
    const kind = this.input("player-kind").value;
    return {
      kind: isPlayerKind(kind) ? kind : "balanced",
      team: side === "0" ? 0 : side === "1" ? 1 : undefined,
    };
  }
  /** Connection progress. While the socket is down the room menu gives way to a dialog
   * that says so; its only choice is to leave, since reconnecting is automatic. */
  status(text: string, connected: boolean): void {
    this.set("network-status", connected ? text : "");
    if (connected) {
      this.link = "live";
      this.render();
    } else {
      // A drop from a live room is news; later attempts only update the message.
      this.showConnecting(
        this.link === "live" ? "CONNECTION LOST" : this.element("connection-title").textContent,
        text,
      );
    }
  }
  /** A one-line answer in the room menu, such as a copied link or a full team. */
  notice(text: string): void {
    this.set("network-message", text);
  }
  /** The connection gave up. Offer what can still work for this cause. */
  ended(end: ConnectionEnd): void {
    this.link = end;
    const ending = ENDINGS[end.cause];
    this.set("connection-title", ending.title);
    this.set("connection-message", end.text);
    const retry = this.button("connection-retry");
    retry.hidden = !ending.retry;
    retry.textContent = ending.retry ?? "";
    const setup = this.button("connection-setup");
    setup.hidden = false;
    setup.className = ending.retry ? "secondary" : "primary";
    setup.textContent =
      end.cause === "outdated" || end.cause === "renderer" ? "RELOAD" : "BATTLE SETUP";
    this.button("connection-leave").hidden = true;
    this.panel.querySelector<HTMLElement>(".network-connection .startup-track")!.hidden = true;
    this.render();
  }
  private showConnecting(title: string, text: string): void {
    this.link = "connecting";
    this.set("connection-title", title);
    const playing = this.lastLobby?.phase === "playing";
    this.set(
      "connection-message",
      text +
        (!playing
          ? ""
          : this.lastLobby!.settings.humansOnly
            ? " Your tank sits idle until you're back."
            : " A bot drives your tank until you're back."),
    );
    this.button("connection-retry").hidden = true;
    this.button("connection-setup").hidden = true;
    this.button("connection-leave").hidden = false;
    this.panel.querySelector<HTMLElement>(".network-connection .startup-track")!.hidden = false;
    this.render();
  }
  lobby(lobby: Lobby, playerId: string): void {
    if (this.lastLobby?.roomEpoch === lobby.roomEpoch) {
      for (const player of lobby.players) {
        const previous = this.lastLobby.players.find((item) => item.playerId === player.playerId);
        if (player.playerId !== playerId && player.connected && !previous?.connected) {
          this.addFeed(
            player.name +
              (previous
                ? " reconnected"
                : " joined " + (player.team === 0 ? "Blue" : "Red") + " team"),
          );
        }
      }
    }
    this.lastLobby = lobby;
    this.isJoined = true;
    const host = lobby.hostId === playerId;
    const playing = lobby.phase === "playing";
    if (lobby.phase !== "results") {
      this.outcome = undefined;
    }
    if (playing) {
      this.editingRules = this.editingChoice = false;
    }
    // Choices are only open between battles, and room rules only to the host. Everyone
    // reads the rules as text; the controls unfold only when someone asks to change them.
    this.canEditRules = host && !playing;
    const alone = lobby.players.length === 1;
    this.panel.querySelector<HTMLElement>(".network-menu")!.dataset.phase = lobby.phase;
    this.renderHeading(lobby, host, alone);
    this.renderSummary(lobby);
    this.element("next-label").hidden = playing;
    this.element("choice-line").hidden = playing;
    const mine = lobby.players.find((player) => player.playerId === playerId);
    if (mine) {
      this.input("player-team").value = String(mine.team);
      this.input("player-kind").value = mine.kind;
      this.set("next-choice", (mine.team === 0 ? "Blue" : "Red") + " · " + tankName(mine.kind));
    }
    this.renderEditing();
    this.offerExtraLevels(isExtraLevel(lobby.settings.mapMode));
    this.input("room-map").value = lobby.settings.mapMode;
    this.input("room-bots").value = lobby.settings.humansOnly ? "none" : lobby.settings.difficulty;
    if (document.activeElement !== this.input("room-round-minutes")) {
      this.input("room-round-minutes").value = String(lobby.settings.roundMinutes);
    }
    this.button("start-match").hidden = !host || playing;
    this.button("start-match").textContent =
      lobby.phase === "results" ? "PLAY AGAIN" : "START BATTLE";
    this.button("network-end").hidden = !host || !playing;
    this.button("network-resume").hidden = !playing;
    // Between battles, leaving the room is how you get back to Battle Setup.
    this.button("leave-room").textContent = playing ? "LEAVE ROOM" : "BATTLE SETUP";
    this.element("network-leave-note").hidden = !alone;
    this.renderPlayers(lobby, playerId);
    this.renderRoster(lobby, playerId, playing);
    this.renderScoreboard(lobby, playerId);
    if (lobby.phase === "results") {
      this.menu = false;
    }
    this.render();
  }
  private renderHeading(lobby: Lobby, host: boolean, alone: boolean): void {
    const next = lobby.phase === "results" ? "the next battle" : "the battle";
    const hostName = lobby.players.find((player) => player.playerId === lobby.hostId)?.name;
    const hint =
      lobby.phase === "playing"
        ? lobby.settings.humansOnly
          ? "The battle keeps going. Your tank sits idle and vulnerable while this menu is open."
          : "The battle keeps going. A bot drives your tank while this menu is open."
        : !host
          ? "You stay in this room with everyone. Waiting for " +
            (hostName ?? "the host") +
            " to start " +
            next +
            "."
          : alone
            ? "You're the only one here. Share the invite link, or start " + next + " on your own."
            : "Everyone stays in this room. " +
              (lobby.phase === "results" ? "Play again" : "Start the battle") +
              " to keep the teams together.";
    this.set("network-hint", hint);
    this.set(
      "network-title",
      lobby.phase === "playing"
        ? "BATTLE IN PROGRESS"
        : lobby.phase === "results"
          ? this.resultsTitle()
          : "LOBBY",
    );
    this.renderScore();
  }
  /** The round that just ended, as the viewer's tank played it. */
  result(match: MatchState, team: number): void {
    this.outcome = { match, team };
  }
  /** Victory or defeat when the final state arrived. A player who joined during the
   * results, or had the menu open as the round ended, gets a neutral heading. */
  private resultsTitle(): string {
    const outcome = this.outcome;
    if (!outcome) {
      return this.endedRound === this.lastLobby?.roundId ? "BATTLE ENDED" : "ROUND COMPLETE";
    }
    if (outcome.match.endedEarly || outcome.match.winner === null) {
      return "BATTLE ENDED";
    }
    return outcome.match.winner === outcome.team ? "VICTORY" : "DEFEAT";
  }
  private renderScore(): void {
    const match = this.lastLobby?.phase === "results" ? this.outcome?.match : undefined;
    this.element("network-score").hidden = !match;
    if (match) {
      this.set("final-blue", String(match.scores[0]));
      this.set("final-red", String(match.scores[1]));
    }
  }
  /** The host's map list adds the extra levels on a page opened with `?debug`, or
   * while the room plays one, so the current map always shows. */
  private offerExtraLevels(playingOne: boolean): void {
    const select = this.input("room-map");
    const offered = select.querySelector("optgroup");
    if (offered || !(playingOne || showsExtraLevels(location.search))) {
      return;
    }
    const group = document.createElement("optgroup");
    group.label = "Extra levels";
    for (const map of MAP_OPTIONS) {
      if ("extra" in map) {
        group.append(new Option(map.name, map.id));
      }
    }
    select.append(group);
  }
  /** The room's rules as read-only chips: the arena, the bots and the match length. */
  private renderSummary(lobby: Lobby): void {
    const settings = lobby.settings;
    const map = mapOption(settings.mapMode);
    const arena = (map?.name ?? settings.mapMode) + (map && "extra" in map ? " · Extra level" : "");
    const bots = settings.humansOnly
      ? "No bots"
      : settings.difficulty[0].toUpperCase() + settings.difficulty.slice(1) + " bots";
    const chips = [arena, bots, settings.roundMinutes + " min"];
    const summary = this.element("network-summary");
    if (summary.textContent !== chips.join("")) {
      summary.replaceChildren(
        ...chips.map((text) =>
          Object.assign(document.createElement("span"), { textContent: text }),
        ),
      );
    }
  }
  /** The compact kill list beside the HUD during a battle. */
  private renderPlayers(lobby: Lobby, playerId: string): void {
    const players = this.element("network-players");
    players.replaceChildren();
    this.playerRows.clear();
    const header = document.createElement("div");
    header.className = "network-player-heading";
    const title = document.createElement("strong");
    const kills = document.createElement("span");
    title.textContent = "PLAYERS · " + lobby.players.length;
    kills.textContent = "KILLS";
    header.append(title, kills);
    players.append(header);
    for (const player of [...lobby.players].sort((a, b) => a.team - b.team || a.slot - b.slot)) {
      const row = document.createElement("div");
      const name = document.createElement("span");
      const score = document.createElement("b");
      row.className = "network-player";
      row.dataset.team = String(player.team);
      row.dataset.playerId = player.playerId;
      name.textContent = player.name + (player.playerId === playerId ? " (you)" : "");
      name.title = player.name + (!player.connected ? " · Reconnecting" : "");
      row.classList.toggle("reconnecting", !player.connected);
      score.textContent = String(player.kills);
      row.append(name, score);
      players.append(row);
      if (player.tankId !== undefined) {
        this.playerRows.set(player.tankId, score);
      }
    }
  }
  private renderRoster(lobby: Lobby, playerId: string, playing: boolean): void {
    const roster = this.element("network-roster");
    roster.replaceChildren();
    const map = mapOption(lobby.settings.mapMode);
    const teamTanks =
      map && "teamTanks" in map && !lobby.settings.humansOnly ? map.teamTanks : STANDARD_TEAM_TANKS;
    for (const side of [0, 1]) {
      const members = lobby.players.filter((player) => player.team === side);
      const column = document.createElement("section");
      column.className = "network-team";
      column.dataset.team = String(side);
      const header = document.createElement("header");
      const name = document.createElement("strong");
      const fill = document.createElement("small");
      name.textContent = TEAM_NAMES[side] + " TEAM";
      const open = teamTanks - members.length;
      fill.textContent = lobby.settings.humansOnly
        ? open + (open === 1 ? " open seat" : " open seats")
        : open + (open === 1 ? " bot" : " bots");
      header.append(name, fill);
      column.append(header);
      for (const player of members) {
        column.append(this.rosterRow(player, lobby, playerId, playing));
      }
      roster.append(column);
    }
  }
  private rosterRow(player: Player, lobby: Lobby, playerId: string, playing: boolean): HTMLElement {
    const row = document.createElement("div");
    row.className = "roster-player";
    row.classList.toggle("you", player.playerId === playerId);
    row.classList.toggle("reconnecting", !player.connected);
    const name = document.createElement("span");
    name.className = "roster-name";
    name.textContent = player.name;
    name.title = player.name;
    const tags = [
      player.playerId === playerId ? "YOU" : "",
      player.playerId === lobby.hostId ? "HOST" : "",
      !player.connected ? "RECONNECTING" : "",
    ].filter(Boolean);
    row.append(name);
    if (tags.length) {
      const tag = document.createElement("small");
      tag.textContent = tags.join(" · ");
      row.append(tag);
    }
    if (playing) {
      const kills = document.createElement("b");
      kills.textContent = String(player.kills);
      kills.title = player.kills + (player.kills === 1 ? " kill" : " kills");
      row.append(kills);
    }
    return row;
  }
  private renderScoreboard(lobby: Lobby, playerId: string): void {
    const board = this.element("network-scoreboard");
    board.hidden = lobby.phase !== "results";
    board.replaceChildren();
    if (board.hidden) {
      return;
    }
    const table = document.createElement("table");
    const head = table.createTHead().insertRow();
    for (const label of ["LAST ROUND", "KILLS", "DEATHS"]) {
      const cell = document.createElement("th");
      cell.textContent = label;
      head.append(cell);
    }
    const body = table.createTBody();
    for (const player of [...lobby.scoreboard].sort(
      (a, b) => b.kills - a.kills || a.deaths - b.deaths,
    )) {
      const row = body.insertRow();
      row.dataset.team = String(player.team);
      row.classList.toggle("you", player.playerId === playerId);
      for (const value of [player.name, String(player.kills), String(player.deaths)]) {
        row.insertCell().textContent = value;
      }
    }
    board.append(table);
  }
  /** Fold or unfold the rules and your team and tank choice. */
  private renderEditing(): void {
    const between = !!this.lastLobby && this.lastLobby.phase !== "playing";
    const rules = this.button("change-rules");
    rules.hidden = !this.canEditRules;
    rules.textContent = this.editingRules ? "Done" : "Change rules";
    rules.setAttribute("aria-expanded", String(this.editingRules));
    this.element("host-settings").hidden = !(this.canEditRules && this.editingRules);
    const choice = this.button("change-choice");
    choice.textContent = this.editingChoice ? "Done" : "Change team or tank";
    choice.setAttribute("aria-expanded", String(this.editingChoice));
    this.element("player-fields").hidden = !(between && this.editingChoice);
  }
  /** Which of the overlay's two dialogs shows, if any. */
  private render(): void {
    const playing = this.lastLobby?.phase === "playing";
    const dropped = this.link !== "live";
    this.panel.querySelector<HTMLElement>(".network-menu")!.hidden = dropped;
    this.panel.querySelector<HTMLElement>(".network-connection")!.hidden = !dropped;
    this.panel.style.display = dropped || !playing || this.menu ? "grid" : "none";
    this.root
      .querySelector("#hud")!
      .classList.toggle("menu-open", dropped || !playing || this.menu);
    this.element("network-players").hidden = !playing || this.menu || dropped;
  }
  setMenu(open: boolean): void {
    this.menu = open;
    this.notice("");
    this.render();
  }
  resetFeedback(): void {
    this.feed = [];
    this.toastTime = 0;
    this.hurtTime = 0;
    this.deathCause = "";
    this.set("network-death-cause", "");
    this.root.querySelector<HTMLElement>("#network-respawn")!.hidden = true;
    this.root.querySelector(".status")!.classList.remove("critical-health");
    this.root.querySelector("#feed")!.replaceChildren();
    this.root.querySelector("#toast")!.classList.remove("visible");
    this.root.querySelector<HTMLElement>("#damage-direction")!.hidden = true;
  }
  private addFeed(...names: string[]): void {
    this.feed.unshift({ names, time: 5 });
    this.feed.length = Math.min(4, this.feed.length);
  }
  event(event: HudEvent, hud: Hud): void {
    const viewerId = hud.human.id;
    const damageAngle = event.damageAngle;
    if (event.type === "death") {
      this.addFeed(...killFeedNames(event, viewerId, hud.scoreboard));
    }
    if (event.id === viewerId) {
      if (event.type === "death") {
        this.deathCause = deathCause(event, hud.scoreboard);
        this.set("toast", "");
        this.toastTime = 0;
      } else if (event.label) {
        this.set("toast", event.label);
        this.toastTime = 2;
      }
      if ((event.type === "hurt" || event.type === "death") && damageAngle !== null) {
        const indicator = this.root.querySelector<HTMLElement>("#damage-direction")!;
        indicator.style.transform = "translate(-50%, -50%) rotate(" + damageAngle + "rad)";
        this.hurtTime = 1.2;
      }
      if (event.type === "respawn") {
        this.deathCause = "";
        this.set("network-death-cause", "");
        this.toastTime = 0;
        this.hurtTime = 0;
      }
    }
  }
  update(hud: Hud, dt: number, connected: boolean): void {
    for (const tank of hud.scoreboard) {
      const score = this.playerRows.get(tank.id);
      if (score && score.textContent !== String(tank.kills)) {
        score.textContent = String(tank.kills);
      }
    }
    const tank = hud.human;
    const match = hud.match;
    this.root.querySelector<HTMLElement>("#hud")!.style.opacity = "1";
    this.set("score0", String(match.scores[0]));
    this.set("score1", String(match.scores[1]));
    const seconds = Math.max(0, Math.ceil(match.time));
    this.set(
      "time",
      match.overtime
        ? "NEXT KILL"
        : Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0"),
    );
    this.set("hp", String(Math.max(0, Math.ceil(tank.hp))));
    this.set("vehicle-name", tank.vehicleName);
    this.set("rank", tank.rankName.toUpperCase());
    const rank = this.root.querySelector<HTMLElement>("#rank")!;
    rank.dataset.rank = String(tank.rank);
    rank.title = rankTitle(tank);
    const bar = this.root.querySelector<HTMLElement>("#hpbar")!;
    bar.style.width = tank.healthRatio * 100 + "%";
    bar.style.backgroundColor = "#" + tank.healthColor.toString(16).padStart(6, "0");
    for (const ammo of tank.ammo) {
      this.set("ammo-count-" + ammo.weapon, ammo.count === null ? "∞" : String(ammo.count));
      const slot = this.button("ammo-" + ammo.weapon);
      slot.disabled = !tank.alive || !connected || this.menu;
      slot.classList.toggle("selected", ammo.selected);
      slot.classList.toggle("empty", !ammo.available);
      slot.setAttribute("aria-pressed", String(ammo.selected));
    }
    this.set(
      "mine",
      tank.mineCooldown > 0 ? "MINE " + tank.mineCooldown.toFixed(1) + "s" : "MINE READY · RMB",
    );
    this.set("effects", effectsLabel(tank));
    this.root
      .querySelector(".status")!
      .classList.toggle("critical-health", tank.alive && tank.healthRatio < 0.25);
    this.root
      .querySelector("#hud")!
      .classList.toggle("paused", this.menu || !connected || match.phase !== "playing");
    const respawn = this.root.querySelector<HTMLElement>("#network-respawn")!;
    respawn.hidden = tank.alive || this.menu || !connected || match.phase !== "playing";
    this.set("network-respawn-count", "Respawn in " + Math.ceil(tank.respawn));
    this.set("network-death-cause", this.deathCause);
    this.toastTime -= dt;
    this.root.querySelector("#toast")!.classList.toggle("visible", this.toastTime > 0);
    this.hurtTime -= dt;
    this.root.querySelector<HTMLElement>("#damage-direction")!.hidden = this.hurtTime <= 0;
    this.feed = this.feed.filter((row) => (row.time -= dt) > 0);
    const feed = this.root.querySelector("#feed")!;
    while (feed.children.length > this.feed.length) {
      feed.lastElementChild!.remove();
    }
    this.feed.forEach((row, index) => {
      let node = feed.children[index] as HTMLElement | undefined;
      if (!node) {
        node = document.createElement("div");
        feed.append(node);
      }
      showFeedRow(node, row.names);
    });
    // The status line is only for connection messages; keep it empty during live play.
    if (connected && !this.menu) {
      this.set("network-status", "");
    }
  }
}
