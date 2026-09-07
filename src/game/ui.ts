import { VEHICLES, WEAPONS, TEAM_NAMES, PICKUPS } from "./data";
import type { Simulation } from "./simulation";
import type { VehicleKind, SimEvent } from "./types";
import { tankPreview } from "./tank-previews";
import { speedTuning } from "./speed-tuning";
import { equippedWeapon } from "./bot-personalities";
import { healthBarState } from "./health-bar";
export class UI {
  overlay: HTMLElement;
  hud: HTMLElement;
  toast: HTMLElement;
  feed: HTMLElement;
  lastPhase = "";
  lastDead = false;
  toastTime = 0;
  feedRows: { text: string; time: number }[] = [];
  constructor(
    root: HTMLElement,
    public s: Simulation,
    public start: () => void,
    public resume: () => void,
    public restart: () => void,
    public setting: (key: string, value: number) => void,
  ) {
    root.insertAdjacentHTML(
      "beforeend",
      `<div id="hud"><div class="brand">SLOPPY<span>TANKS</span></div><div class="scoreboard"><div class="team mint"><small id="label0">◆ BLUE</small><b id="score0">0</b></div><div class="clock"><b id="time">5:00</b><small id="objective">FIRST TO 50</small></div><div class="team coral"><small id="label1">RED Ⅱ</small><b id="score1">0</b></div></div><button id="pause" class="quiet">Ⅱ <span>PAUSE</span></button><div id="feed"></div><div id="toast"></div><div class="bottom"><div class="combat-status"><div class="status"><small id="vehicle-name">BRUISER</small><div><b id="hp">100</b><span>HULL</span><i id="hpbar"></i></div></div><div class="weapon"><small id="weapon">STANDARD SHELLS</small><span id="mine">MINE READY · RMB</span></div><em id="effects"></em></div><div class="keyhint">W A S D <span>DRIVE</span>　 MOUSE <span>AIM & FIRE</span>　 SCROLL <span>ZOOM</span></div></div></div><div id="overlay"></div>`,
    );
    this.overlay = root.querySelector("#overlay")!;
    this.hud = root.querySelector("#hud")!;
    this.toast = root.querySelector("#toast")!;
    this.feed = root.querySelector("#feed")!;
    root.querySelector("#pause")!.addEventListener("click", () => {
      if (s.match.phase === "playing") {
        s.match.phase = "paused";
        this.lastPhase = "";
      }
    });
  }
  chooseCards() {
    return `<div class="vehicles">${(Object.keys(VEHICLES) as VehicleKind[])
      .map((kind) => {
        const v = VEHICLES[kind];
        return `<button class="vehicle ${this.s.humanKind === kind ? "selected" : ""}" data-kind="${kind}"><strong>${v.name}</strong><small>${v.tag}</small><img class="tank-preview" src="${tankPreview(kind, this.s.humanTeam)}" alt="${v.name} tank" draggable="false"><div class="spec"><span>${v.health} HIT POINTS</span><span>${v.speedKmh} KM/H</span></div></button>`;
      })
      .join("")}</div>`;
  }
  bindCards() {
    this.overlay
      .querySelectorAll<HTMLButtonElement>("[data-kind]")
      .forEach((b) =>
        b.addEventListener("click", () => {
          this.s.humanKind = b.dataset.kind as VehicleKind;
          if (this.s.match.phase === "ready") {
            this.start();
            return;
          }
          this.overlay
            .querySelectorAll(".vehicle")
            .forEach((n) => n.classList.remove("selected"));
          b.classList.add("selected");
        }),
      );
  }
  show() {
    const phase = this.s.match.phase;
    this.overlay.style.display =
      phase === "playing" && this.s.human.alive ? "none" : "grid";
    this.hud.style.opacity = phase === "ready" ? "0" : "1";
    if (phase === "ready")
      this.overlay.innerHTML = `<section class="menu start"><div class="eyebrow">${this.s.mapName} / ${this.s.gameMode === "solo" ? "1 V 20" : "6 V 6"}</div><h1>CHOOSE YOUR TANK</h1><p class="intro">Choose your battle, then click a tank to start.</p>${this.modeOptions()}${this.chooseCards()}<div class="menu-foot"><div><b>${this.s.gameMode === "solo" ? "YOUR TANK" : "YOUR TEAM"}: ${this.s.humanTeam === 0 ? "◆" : "Ⅱ"} ${TEAM_NAMES[this.s.humanTeam]}</b><small>${this.s.gameMode === "solo" ? "5 MINUTES · CLEAR ALL 20 ENEMIES · ONE LIFE" : "5 MINUTES · FIRST TO 50 · FRIENDLY FIRE OFF"}</small></div></div><div class="menu-help">WASD drive · Mouse aim · Hold left click to fire · Right click mine · Scroll zoom · Esc pause<br>Shoot incoming shells to intercept · Collect upgrades to combine their effects</div></section>`;
    else if (phase === "paused")
      this.overlay.innerHTML = `<section class="menu compact"><h2>PAUSED</h2><p>WASD drive · Mouse aim · Hold left click to fire<br>Right click mine · Scroll zoom · Escape pause</p><label>Sound <input id="volume" type="range" min="0" max="1" step=".05" value="${localStorage.getItem("sloppy-volume") ?? ".6"}"></label>${this.speedSliders()}<button id="resume" class="primary">RESUME</button><button id="restart" class="secondary">New round / choose vehicle</button><a href="${import.meta.env.BASE_URL}benchmark.html" target="_blank">Open performance notebook</a></section>`;
    else if (phase === "results" && this.s.gameMode === "solo")
      this.overlay.innerHTML = `<section class="menu compact"><div class="eyebrow">SOLO ASSAULT / ${this.s.mapName}</div><h2>${this.s.match.winner === this.s.humanTeam ? "AREA CLEARED" : "ASSAULT FAILED"}</h2><div class="result-score">${this.s.enemiesEliminated} / ${this.s.enemyCount}</div><p>Enemies eliminated.<br>${!this.s.human.alive ? "Your tank was destroyed." : this.s.match.time === 0 ? "Time ran out." : "Every enemy is down."}</p><button id="restart" class="primary">ANOTHER ROUND</button></section>`;
    else if (phase === "results")
      this.overlay.innerHTML = `<section class="menu compact"><div class="eyebrow">ROUND COMPLETE / ${this.s.mapName}</div><h2>${this.s.match.winner === this.s.humanTeam ? "VICTORY" : "DEFEAT"}</h2><div class="result-score"><span>${this.s.match.scores[0]}</span> : <span>${this.s.match.scores[1]}</span></div><p>${TEAM_NAMES[this.s.match.winner ?? 0]} wins${this.s.match.overtime ? " in overtime" : ""}.<br>You scored ${this.s.human.kills} eliminations · ${this.s.human.deaths} wrecks<br>${this.s.destroyed} pieces of cover demolished.</p><button id="restart" class="primary">ANOTHER ROUND</button></section>`;
    else if (!this.s.human.alive)
      this.overlay.innerHTML = `<section class="menu respawn"><h2>Respawn in <span id="respawn-count">3</span></h2>${this.chooseCards()}</section>`;
    this.bindCards();
    for (const key of ["gameMode", "mapMode"] as const)
      this.overlay.querySelectorAll<HTMLInputElement>(`input[name="${key}"]`).forEach(input =>
        input.addEventListener("change", () => {
          if (key === "gameMode") this.s.gameMode = input.value as Simulation["gameMode"];
          else this.s.mapMode = input.value as Simulation["mapMode"];
          this.show();
        }));

    this.overlay
      .querySelector("#resume")
      ?.addEventListener("click", this.resume);
    this.overlay
      .querySelector("#restart")
      ?.addEventListener("click", this.restart);
    for (const key of ["volume", "tank-speed", "bullet-speed"])
      this.overlay
        .querySelector<HTMLInputElement>("#" + key)
        ?.addEventListener("input", (e) => {
          const value = +(e.target as HTMLInputElement).value;
          this.setting(key, value);
          const output = this.overlay.querySelector(`#${key}-value`);
          if (output) output.textContent = `${Math.round(value * 100)}%`;
        });
  }
  modeOptions() {
    const group = (key: "gameMode" | "mapMode", title: string, options: string[][]) =>
      `<fieldset><legend>${title}</legend>${options.map(([value, title, detail]) =>
        `<label class="mode-option"><input type="radio" name="${key}" value="${value}" ${this.s[key] === value ? "checked" : ""}><span><b>${title}</b><small>${detail}</small></span></label>`).join("")}</fieldset>`;
    return `<div class="mode-options">${group("gameMode", "BATTLE", [
      ["team", "Team Battle", "6 vs 6 · Respawns · First to 50"],
      ["solo", "Solo Assault", "1 vs 20 · 6 at a time · One life"],
    ])}${group("mapMode", "MAP", [
      ["village", "Pine Village", "The original arena"],
      ["random", "Random Map", "Fresh layout every round"],
    ])}</div>`;
  }
  speedSliders() {
    return `<div class="speed-tuning">${(["tank-speed", "bullet-speed"] as const).map((key) =>
      `<label for="${key}"><span>${key === "tank-speed" ? "Tank base speed" : "Projectile base speed"} <output id="${key}-value" for="${key}">${Math.round(speedTuning[key] * 100)}%</output></span><input id="${key}" type="range" min="0.5" max="2" step="0.05" value="${speedTuning[key]}"></label>`
    ).join("")}<small>50–200% · 100% = default speed · Saved automatically</small></div>`;
  }
  event(e: SimEvent) {
    if (e.type === "pickup" && e.id === this.s.human.id) {
      this.toast.textContent = e.label ?? "";
      this.toastTime = 2.4;
      this.toast.classList.add("visible");
    }
    if (e.type === "death" && e.label)
      this.feedRows.unshift({ text: e.label, time: 5 });
  }
  update(dt: number) {
    const s = this.s,
      t = s.human;
    const dead = !t.alive;
    if (this.lastPhase !== s.match.phase || dead !== this.lastDead) {
      this.lastPhase = s.match.phase;
      this.lastDead = dead;
      this.show();
    }
    const set = (id: string, text: string) => {
      const e = document.getElementById(id);
      if (e && e.textContent !== text) e.textContent = text;
    };
    const solo = s.gameMode === "solo";
    const remaining = s.enemyCount - s.enemiesEliminated;
    set("label0", solo ? "ELIMINATED" : "◆ BLUE");
    set("label1", solo ? "REMAINING" : "RED Ⅱ");
    set("objective", solo ? "ONE LIFE · CLEAR ALL" : "FIRST TO 50");
    set("score0", String(solo ? s.enemyCount - remaining : s.match.scores[0]));
    set("score1", String(solo ? remaining : s.match.scores[1]));
    const sec = Math.ceil(s.match.time);
    set(
      "time",
      s.match.overtime
        ? "NEXT KILL"
        : `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`,
    );
    set("hp", String(Math.ceil(t.hp)));
    set("vehicle-name", VEHICLES[t.kind].name);
    set(
      "weapon",
      WEAPONS[equippedWeapon(t)].name.toUpperCase() +
        ((t.rocket || t.spread) > 0 ? ` · ${Math.ceil(t.rocket || t.spread)}s` : ""),
    );
    set(
      "mine",
      t.mineCooldown > 0
        ? `MINE ${t.mineCooldown.toFixed(1)}s`
        : "MINE READY · RMB",
    );
    set(
      "effects",
      [
        t.spread > 0 && t.rocket > 0 ? `SPREAD ${Math.ceil(t.spread)}s` : null,
        t.protection > 0 ? "SPAWN SHIELD" : null,
        t.shield > 0 ? `◇ SHIELD ${Math.ceil(t.shieldPoints)} HP · ${Math.ceil(t.shield)}s` : null,
        t.rapid > 0 ? `» RAPID ${Math.ceil(t.rapid)}s` : null,
        t.ricochet > 0 ? `↗ RICOCHET ${Math.ceil(t.ricochet)}s` : null,
        t.speed > 0 ? `ϟ BOOST ${Math.ceil(t.speed)}s` : null,
      ]
        .filter(Boolean)
        .join("  "),
    );
    set("respawn-count", String(Math.ceil(t.respawn)));
    const health = healthBarState(t.hp, VEHICLES[t.kind].health, t.team);
    const hpbar = document.getElementById("hpbar")!;
    hpbar.style.width = `${health.ratio * 100}%`;
    hpbar.style.backgroundColor = `#${health.color.toString(16).padStart(6, "0")}`;
    this.toastTime -= dt;
    if (this.toastTime <= 0) this.toast.classList.remove("visible");
    for (const row of this.feedRows) row.time -= dt;
    this.feedRows = this.feedRows.filter((r) => r.time > 0).slice(0, 4);
    const feed = this.feedRows.map((r) => `<div>${r.text}</div>`).join("");
    if (this.feed.innerHTML !== feed) this.feed.innerHTML = feed;
  }
}
