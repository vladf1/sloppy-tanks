import { hudMarkup } from "../game/ui-markup";
import { AMMO_ORDER, equippedWeapon, hasAmmo } from "../game/ammunition";
import { VEHICLES } from "../game/data";
import { healthBarState } from "../game/health-bar";
import { rankIndex, RANKS } from "../game/veterancy";
import type { RenderState } from "../game/render-state";
import type { SimEvent, Weapon } from "../game/types";
import type { Lobby } from "./protocol";
import type { JoinChoice } from "./connection";
import { playerKind, team } from "./scene-codec";
import "./multiplayer.css";
import { preferredPlayerName, rememberPlayerName } from "./player-name";

export interface NetworkActions {
  join(choice: JoinChoice): void;
  choose(choice: JoinChoice): void;
  settings(map: string, difficulty: string, humansOnly: boolean): void;
  start(): void;
  pause(): void;
  resume(): void;
  end(): void;
  leave(): void;
  ammo(weapon: Weapon): void;
  volume(value: number): void;
}
export class NetworkUI {
  readonly canvas: HTMLCanvasElement;
  readonly panel: HTMLElement;
  private lastLobby?: Lobby;
  private feed: { text: string; time: number }[] = [];
  private toastTime = 0;
  private hurtTime = 0;
  private playerId = "";
  private isJoined = false;
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
      '<div id="network-status" role="status">Choose your tank and join the room.</div><div id="network-respawn" hidden></div>';
    this.canvas = root.querySelector("canvas")!;
    this.panel = root.querySelector("#overlay")!;
    this.panel.innerHTML =
      '<section class="menu compact network-menu"><div class="eyebrow">PLAY WITH FRIENDS</div><h1>ROOM <span id="room-code"></span></h1><p id="network-message">Up to eight friends. Bots fill both teams.</p><div class="network-choices"><label>Your name<input id="player-name" maxlength="24" autocomplete="nickname" placeholder="Tank driver" /></label><label>Team<select id="player-team"><option value="auto">Auto · fewer humans</option><option value="0">Blue</option><option value="1">Red</option></select></label><label>Your tank<select id="player-kind"><option value="scout">Scout</option><option value="balanced" selected>Balanced</option><option value="heavy">Heavy</option></select></label></div><div id="host-settings" class="network-choices" hidden><label>Map<select id="room-map"><option value="village">Pine Village</option><option value="harbor">Harbor Havoc</option><option value="quarry">Dusty Dig</option></select></label><label>Bots<select id="room-difficulty"><option value="easy">Easy</option><option value="normal" selected>Normal</option><option value="hard">Hard</option></select></label><label class="network-toggle"><input id="room-humans-only" type="checkbox" />Humans only (no bots)</label></div><div id="network-roster"></div><div id="network-scoreboard"></div><div class="network-actions"><button id="join-room" class="primary">JOIN ROOM</button><button id="start-match" class="primary" hidden>START BATTLE</button><button id="network-resume" class="primary" hidden>RESUME</button><button id="network-end" class="secondary" hidden>END BATTLE</button><button id="copy-room" class="secondary">COPY ROOM LINK</button><button id="leave-room" class="quiet">BROWSE ROOMS</button></div><label class="network-local" hidden>Touch controls<select id="touch-mode"><option value="auto">Auto</option><option value="on">On</option><option value="off">Off</option></select></label><label class="network-local" hidden>Sound<input id="network-volume" type="range" min="0" max="1" step="0.05" /></label><p id="network-help" class="network-help">WASD / arrows to drive · Mouse to aim and fire · Right click for mines<br />Opening this menu lets a bot drive your tank. The match keeps going.</p></section>';
    this.set("room-code", room);
    this.input("player-name").value = preferredPlayerName();
    this.input("network-volume").value = localStorage.getItem("sloppy-volume") ?? "0.6";
    this.on("join-room", () => {
      const choice = this.choice();
      if (!choice.name) {
        this.input("player-name").focus();
        this.set("network-message", "Enter a name to join.");
        return;
      }
      this.actions.join(choice);
      this.button("join-room").disabled = true;
      rememberPlayerName(choice.name);
    });
    this.on("start-match", () => this.actions.start());
    this.on("network-end", () => this.actions.end());
    this.on("network-resume", () => this.actions.resume());
    this.on("leave-room", () => this.actions.leave());
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
    for (const field of ["room-map", "room-difficulty", "room-humans-only"]) {
      this.root
        .querySelector("#" + field)!
        .addEventListener("change", () =>
          this.actions.settings(
            this.input("room-map").value,
            this.input("room-difficulty").value,
            this.root.querySelector<HTMLInputElement>("#room-humans-only")!.checked,
          ),
        );
    }
    this.input("network-volume").addEventListener("input", () =>
      this.actions.volume(Number(this.input("network-volume").value)),
    );
    this.on("copy-room", () => {
      void navigator.clipboard
        .writeText(location.href)
        .then(() => this.set("network-message", "Room link copied. Send it to your friends."))
        .catch(() => {
          this.set("network-message", "Copy the address from your browser to invite friends.");
        });
    });
  }
  private input(name: string): HTMLInputElement | HTMLSelectElement {
    return this.root.querySelector<HTMLInputElement | HTMLSelectElement>("#" + name)!;
  }
  private button(name: string): HTMLButtonElement {
    return this.root.querySelector<HTMLButtonElement>("#" + name)!;
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
  private choice(): JoinChoice {
    const side = this.input("player-team").value;
    return {
      name: this.input("player-name").value.trim(),
      kind: playerKind.read(this.input("player-kind").value),
      team: side === "auto" ? undefined : team.read(Number(side)),
    };
  }
  status(text: string, connected: boolean): void {
    this.set("network-status", text);
    this.set("network-message", text);
    if (!connected) {
      this.button("join-room").disabled = false;
      this.button("join-room").hidden = false;
      this.panel.style.display = "grid";
    }
  }
  lobby(lobby: Lobby, playerId: string): void {
    this.lastLobby = lobby;
    this.playerId = playerId;
    this.isJoined = true;
    const mine = lobby.players.find((player) => player.playerId === playerId);
    const host = lobby.hostId === playerId;
    const playing = lobby.phase === "playing";
    this.button("join-room").hidden = true;
    this.button("start-match").hidden = !host || playing;
    this.button("start-match").disabled = false;
    this.button("start-match").textContent =
      lobby.phase === "results" ? "PLAY AGAIN" : "START BATTLE";
    this.button("network-end").hidden = !host || !playing;
    this.button("network-resume").hidden = !playing;
    this.button("leave-room").textContent = "LEAVE ROOM";
    this.input("player-name").disabled = true;
    for (const field of ["player-team", "player-kind"]) {
      this.input(field).disabled = playing;
    }
    if (mine) {
      this.input("player-team").value = String(mine.team);
      this.input("player-kind").value = mine.kind;
    }
    this.input("room-map").value = lobby.settings.mapMode;
    this.input("room-difficulty").value = lobby.settings.difficulty;
    this.input("room-map").disabled = this.input("room-difficulty").disabled = !host || playing;
    const humansOnly = this.root.querySelector<HTMLInputElement>("#room-humans-only")!;
    humansOnly.checked = lobby.settings.humansOnly;
    humansOnly.disabled = !host || playing;
    this.input("room-difficulty").disabled ||= lobby.settings.humansOnly;
    this.set(
      "network-help",
      "WASD / arrows to drive · Mouse to aim and fire · Right click for mines. " +
        (lobby.settings.humansOnly
          ? "Humans only: empty seats stay empty. Opening this menu leaves your tank idle and vulnerable. The match keeps going."
          : "Opening this menu lets a bot drive your tank. The match keeps going."),
    );
    this.root.querySelector<HTMLElement>("#host-settings")!.hidden = false;
    this.root
      .querySelectorAll<HTMLElement>(".network-local")
      .forEach((node) => (node.hidden = !playing));
    const roster = this.root.querySelector("#network-roster")!;
    roster.replaceChildren();
    for (const side of [0, 1]) {
      const column = document.createElement("div");
      const title = document.createElement("b");
      title.textContent = side === 0 ? "BLUE TEAM" : "RED TEAM";
      column.append(title);
      for (const player of lobby.players.filter((player) => player.team === side)) {
        const row = document.createElement("div");
        row.textContent =
          player.name +
          (player.playerId === lobby.hostId ? " · Host" : "") +
          (!player.connected ? " · Reconnecting" : "");
        column.append(row);
      }
      const bots = document.createElement("small");
      bots.textContent =
        6 -
        lobby.players.filter((player) => player.team === side).length +
        (lobby.settings.humansOnly ? " open seats" : " bots");
      column.append(bots);
      roster.append(column);
    }
    const board = this.root.querySelector("#network-scoreboard")!;
    board.replaceChildren();
    if (lobby.phase === "results") {
      this.menu = false;
      const heading = document.createElement("h2");
      heading.textContent = "ROUND COMPLETE";
      board.append(heading);
      for (const player of [...lobby.scoreboard].sort((a, b) => b.kills - a.kills)) {
        const row = document.createElement("p");
        row.textContent =
          player.name + " · " + player.kills + " kills / " + player.deaths + " deaths";
        board.append(row);
      }
    }
    this.panel.style.display = playing && !this.menu ? "none" : "grid";
    if (!playing) {
      this.set(
        "network-message",
        host
          ? "Invite friends, choose the map, then start when ready."
          : "Waiting for the host to start the battle.",
      );
    }
  }
  setMenu(open: boolean): void {
    this.menu = open;
    if (this.lastLobby) {
      this.lobby(this.lastLobby, this.playerId);
    }
  }
  resetFeedback(): void {
    this.feed = [];
    this.toastTime = 0;
    this.hurtTime = 0;
    this.root.querySelector("#feed")!.replaceChildren();
    this.root.querySelector("#toast")!.classList.remove("visible");
    this.root.querySelector<HTMLElement>("#damage-direction")!.hidden = true;
  }
  event(event: SimEvent, state: RenderState, damageAngle: number | null): void {
    if (event.type === "death") {
      const name = (id: number | undefined) =>
        id === state.viewerId
          ? "YOU"
          : (state.tanks.find((tank) => tank.id === id)?.name ?? "YARD");
      this.feed.unshift({ text: name(event.owner) + "  ▸  " + name(event.id), time: 5 });
      this.feed.length = Math.min(4, this.feed.length);
    }
    if (event.id === state.viewerId) {
      if (event.label) {
        this.set("toast", event.label);
        this.toastTime = 2;
      }
      if ((event.type === "hurt" || event.type === "death") && damageAngle !== null) {
        const indicator = this.root.querySelector<HTMLElement>("#damage-direction")!;
        indicator.style.transform = "translate(-50%, -50%) rotate(" + damageAngle + "rad)";
        this.hurtTime = 1.2;
      }
      if (event.type === "respawn") {
        this.hurtTime = 0;
      }
    }
  }
  update(state: RenderState, dt: number, rtt: number, connected: boolean): void {
    const tank = state.viewer;
    const match = state.match;
    const health = healthBarState(tank.hp, tank.maxHp, tank.team);
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
    this.set("vehicle-name", VEHICLES[tank.kind].name);
    this.set("rank", RANKS[rankIndex(tank)].name.toUpperCase());
    const bar = this.root.querySelector<HTMLElement>("#hpbar")!;
    bar.style.width = health.ratio * 100 + "%";
    bar.style.backgroundColor = "#" + health.color.toString(16).padStart(6, "0");
    const selected = equippedWeapon(tank);
    for (const weapon of AMMO_ORDER) {
      this.set("ammo-count-" + weapon, weapon === "standard" ? "∞" : String(tank.ammo[weapon]));
      const slot = this.button("ammo-" + weapon);
      slot.disabled = !tank.alive || !connected || this.menu;
      slot.classList.toggle("selected", selected === weapon);
      slot.classList.toggle("empty", !hasAmmo(tank, weapon));
      slot.setAttribute("aria-pressed", String(selected === weapon));
    }
    this.set(
      "mine",
      tank.mineCooldown > 0 ? "MINE " + tank.mineCooldown.toFixed(1) + "s" : "MINE READY · RMB",
    );
    this.set(
      "effects",
      [
        tank.protection > 0 ? "SPAWN SHIELD" : "",
        tank.shield > 0 ? "SHIELD " + Math.ceil(tank.shieldPoints) + " HP" : "",
        tank.rapid > 0 ? "RAPID" : "",
        tank.speed > 0 ? "BOOST" : "",
        tank.laser > 0 ? "LASER DEFENSE" : "",
      ]
        .filter(Boolean)
        .join(" · "),
    );
    const respawn = this.root.querySelector<HTMLElement>("#network-respawn")!;
    respawn.hidden = tank.alive || this.menu || match.phase !== "playing";
    respawn.textContent = "Respawn in " + Math.ceil(tank.respawn);
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
      let node = feed.children[index];
      if (!node) {
        node = document.createElement("div");
        feed.append(node);
      }
      if (node.textContent !== row.text) {
        node.textContent = row.text;
      }
    });
    if (connected && !this.menu) {
      this.set("network-status", Math.round(rtt) + " ms" + (rtt > 120 ? " · High latency" : ""));
    }
  }
}
