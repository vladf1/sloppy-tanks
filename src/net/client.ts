import { Presentation } from "../game/presentation";
import { NerdStats } from "../game/nerd-stats";
import { Controls } from "../game/controls";
import { AudioSystem } from "../game/audio";
import { TouchModeController, type TouchState } from "../game/touch-mode";
import { loadTankSurface } from "../game/tank-surfaces";
import { AMMO_ORDER, hasAmmo } from "../game/ammunition";
import { CAMERA } from "../game/view-settings";
import type { RenderState } from "../game/render-state";
import type { Weapon } from "../game/types";
import { encodeInput, type ControlInput } from "./player-controls";
import { InputCadence } from "./input-cadence";
import { Connection } from "./connection";
import { StateMirror } from "./replication";
import { NetworkTimeline } from "./interpolation";
import { NetworkUI } from "./network-ui";
import { ROOM_CODE, lobbyReader, controlReader, settingsReader, type Control } from "./protocol";
import { id } from "./schema";
import { browseRooms } from "./room-browser";
import type { JoinChoice } from "./connection";

const MAX_ACTIONS = 8;
export async function startMultiplayer(root: HTMLElement): Promise<void> {
  const params = new URLSearchParams(location.search);
  let room = params.get("room")?.toUpperCase();
  const local = ["localhost", "127.0.0.1"].includes(location.hostname);
  const configured: unknown = import.meta.env.VITE_MULTIPLAYER_URL;
  const endpoint =
    (import.meta.env.DEV ? params.get("server") : null) ||
    (typeof configured === "string" ? configured : "") ||
    (local ? "ws://127.0.0.1:8787" : "");
  if (!endpoint) {
    root.textContent =
      "Multiplayer isn't enabled on this site yet. Open the development site to play with friends.";
    return;
  }
  const address = new URL(endpoint);
  if (
    !["ws:", "wss:"].includes(address.protocol) ||
    address.username ||
    address.password ||
    address.search ||
    address.hash ||
    (!local && address.protocol !== "wss:")
  ) {
    throw new Error("Invalid multiplayer server configuration");
  }
  let selectedChoice: JoinChoice | undefined;
  if (!room || !ROOM_CODE.test(room)) {
    const selection = await browseRooms(root, address);
    room = selection.room;
    selectedChoice = selection.choice;
    const url = new URL(location.href);
    url.searchParams.delete("multiplayer");
    url.searchParams.set("room", room);
    history.replaceState(null, "", url);
  }
  const mirror = new StateMirror();
  const timeline = new NetworkTimeline();
  let view: Presentation | undefined;
  let stats: NerdStats | undefined;
  let lastSnapshotMs = 0;
  let receivedUpdates = 0;
  let statsSampleMs = 0;
  let statsSampleUpdates = 0;
  let appliedInput = 0;
  let audio: AudioSystem | undefined;
  let control: Control | undefined;
  let display: RenderState | undefined;
  let readyRound = 0;
  let preparing = false;
  let active = false;
  let seq = 0;
  let pending: ControlInput["actions"] = [];
  let pendingWeapon: Weapon | undefined;
  let last = performance.now();
  const inputCadence = new InputCadence();
  let requestedFull = false;
  let lastResumeMs = -Infinity;
  let phase = "lobby";
  const clearInput = () => {
    controls?.clear();
    pending = [];
    pendingWeapon = undefined;
  };
  const activeInput = () =>
    !!(
      active &&
      connection.connected &&
      !ui.menu &&
      !document.hidden &&
      phase === "playing" &&
      control?.driver === "human" &&
      display?.viewer.alive &&
      display.viewer.life === control.life &&
      !mirror.needsFull
    );
  const zoom = (amount: number) => {
    if (view) {
      view.zoom = Math.max(CAMERA.minZoom, Math.min(CAMERA.maxZoom, view.zoom + amount));
    }
  };
  const pause = () => {
    if (phase !== "playing" || ui.menu) {
      return;
    }
    ui.setMenu(true);
    clearInput();
    connection.send("suspend");
  };
  const resume = () => {
    if (!connection.connected) {
      return;
    }
    clearInput();
    ui.setMenu(false);
    active = false;
    lastResumeMs = performance.now();
    connection.send("resume");
  };
  const join = (choice: JoinChoice) => {
    audio ??= new AudioSystem();
    audio.volume(Number(localStorage.getItem("sloppy-volume") ?? 0.6));
    audio.start();
    // Manual retries keep the original create/join intent after a connection error.
    void connection.connect({
      ...choice,
      create: selectedChoice?.create,
      existingRoom: selectedChoice?.existingRoom,
    });
  };
  const ui = new NetworkUI(root, room, {
    join,
    choose(choice) {
      connection.send("choose", { team: choice.team, kind: choice.kind });
    },
    settings(mapMode, difficulty, humansOnly, roundMinutes) {
      connection.settings(settingsReader.read({ mapMode, difficulty, humansOnly, roundMinutes }));
    },
    start() {
      clearInput();
      connection.send("start");
    },
    pause,
    resume,
    end() {
      clearInput();
      connection.send("end");
    },
    leave() {
      connection.leave();
      const url = new URL(location.href);
      for (const key of ["room", "latency", "jitter"]) {
        url.searchParams.delete(key);
      }
      url.searchParams.set("multiplayer", "");
      location.assign(url);
    },
    ammo(weapon) {
      if (activeInput()) {
        controls.ammoSelection = weapon;
      }
    },
    volume(value) {
      audio?.volume(value);
      try {
        localStorage.setItem("sloppy-volume", String(value));
      } catch {
        /* Optional preference. */
      }
    },
  });
  const controls = new Controls(ui.canvas, pause, zoom, activeInput, false);
  const touch = new TouchModeController(
    root,
    controls,
    {
      get human() {
        return display?.viewer ?? { mineCooldown: 0 };
      },
      get match(): TouchState["match"] {
        return { phase: ui.menu ? "paused" : phase === "playing" ? "playing" : "ready" };
      },
    },
    zoom,
  );
  const resetDisplay = () => {
    if (!control || !mirror.state) {
      return;
    }
    display = mirror.render(control.tankId);
    timeline.reset(display, mirror.tick, performance.now());
  };
  const requestFull = () => {
    if (!requestedFull) {
      requestedFull = true;
      active = false;
      clearInput();
      connection.send("resync");
    }
  };
  const prepare = async () => {
    if (preparing || !control || !mirror.state) {
      return;
    }
    preparing = true;
    active = false;
    connection.send("suspend");
    ui.status("Preparing the arena…", true);
    const round = connection.roundId;
    const epoch = connection.roomEpoch;
    try {
      if (!view) {
        [view] = await Promise.all([Presentation.create(ui.canvas), loadTankSurface()]);
        stats = new NerdStats(
          root,
          () => {
            if (!display) {
              return undefined;
            }
            const now = performance.now();
            const rate = statsSampleMs
              ? ((receivedUpdates - statsSampleUpdates) * 1000) / (now - statsSampleMs)
              : 0;
            statsSampleMs = now;
            statsSampleUpdates = receivedUpdates;
            connection.measureEdge();
            const { edgeRtt, edgeToRoomRtt, gameEdgeColo, statsEdgeColo } = connection;
            const edgeMatches = !statsEdgeColo || !gameEdgeColo || statsEdgeColo === gameEdgeColo;
            const edgeLeg = (ms: number | undefined) =>
              ms === undefined ? "—" : `${Math.round(ms)} ms${edgeMatches ? "" : " ⚠"}`;
            return {
              state: display,
              rows: [
                [
                  "RTT",
                  `${Math.round(connection.rtt)} ms`,
                  "Measured round-trip time to the game server.",
                ],
                [
                  "Game edge",
                  gameEdgeColo ?? "—",
                  "Cloudflare location the game connection entered through.",
                ],
                [
                  "Stats edge",
                  statsEdgeColo === undefined ? "—" : `${statsEdgeColo}${edgeMatches ? "" : " ⚠"}`,
                  "Cloudflare location of the separate stats connection that measures the edge legs while this panel is open. ⚠ means it differs from the game edge, so the edge legs do not describe the game path.",
                ],
                [
                  "Edge RTT",
                  edgeLeg(edgeRtt),
                  "Round trip to the Cloudflare edge over the stats connection, answered by the Worker without contacting the room. Sampled once a second only while this panel is open.",
                ],
                [
                  "Edge → room",
                  edgeLeg(edgeToRoomRtt),
                  "Round trip from the Cloudflare edge to the room over an open connection, timed by the Worker, including the room's reply time. Sampled with Edge RTT.",
                ],
                [
                  "Updates received",
                  receivedUpdates,
                  "Full-state messages and snapshot batches received during this page session. A batch can contain several simulation snapshots.",
                ],
                [
                  "Update rate",
                  `${rate.toFixed(1)} /s`,
                  "Full-state messages and snapshot batches received per second, not rendered FPS.",
                ],
                [
                  "Snapshot age",
                  `${Math.round(now - lastSnapshotMs)} ms`,
                  "Time since the last full state or snapshot arrived.",
                ],
                [
                  "Playout buffer",
                  `${Math.round(timeline.clock.bufferMs)} ms`,
                  "How far other tanks are drawn behind the fastest recent snapshot arrival. It grows when snapshots arrive late and shrinks slowly afterwards.",
                ],
                [
                  "Buffered ahead",
                  `${Math.round(timeline.marginMs)} ms`,
                  "Received simulation not yet displayed. Negative means snapshots are late and other tanks are briefly extrapolated.",
                ],
                [
                  "Underrun",
                  `${(timeline.clock.underrun * 100).toFixed(1)} %`,
                  "Share of recent frames drawn past the newest snapshot. Sustained values mean visible stutter.",
                ],
                ["Server tick", mirror.tick, "Latest authoritative simulation tick received."],
                [
                  "Input seq sent / ack",
                  `${seq} / ${appliedInput}`,
                  "Latest input sequence sent and acknowledged by the server. Active input sends up to 20/s; unchanged idle input refreshes once/s to retain your seat. These are not received state updates.",
                ],
                [
                  "Connection",
                  connection.connected ? "Connected" : "Reconnecting",
                  "Current game-server connection state.",
                ],
              ],
            };
          },
          view,
          () => active && !ui.menu && phase === "playing",
        );
      }
      if (!mirror.state || !control) {
        return;
      }
      const state = mirror.render(control.tankId);
      view.reset(state);
      await view.prepare(state, (stage) => ui.status(stage, true));
      if (round !== connection.roundId || epoch !== connection.roomEpoch) {
        return;
      }
      readyRound = round;
      resetDisplay();
      active = true;
      ui.canvas.focus();
      ui.status("Connected", true);
      lastResumeMs = performance.now();
      connection.send("resume");
    } catch (error) {
      console.error("Multiplayer graphics failed", error);
      ui.status("The arena could not load. Reload to try again.", false);
      connection.send("suspend");
    } finally {
      preparing = false;
      // The host may start a new round while GPU compilation for the old one is pending.
      if (
        mirror.state &&
        control &&
        connection.connected &&
        (round !== connection.roundId || epoch !== connection.roomEpoch)
      ) {
        void prepare();
      }
    }
  };
  const connection = new Connection(address.href.replace(/\/$/, ""), room, {
    clearInput,
    status(text, connected) {
      if (!connected) {
        active = false;
        clearInput();
      }
      ui.status(text, connected);
    },
    message(message) {
      if (message.type === "welcome") {
        if (mirror.roomEpoch !== connection.roomEpoch) {
          readyRound = 0;
          ui.resetFeedback();
        }
        mirror.needsFull = true;
        active = false;
        control = undefined;
        requestedFull = false;
      } else if (message.type === "lobby") {
        const lobby = lobbyReader.read(message);
        if (lobby.roomEpoch !== connection.roomEpoch) {
          return;
        }
        if (lobby.roundId !== connection.roundId) {
          connection.roundId = lobby.roundId;
          connection.observedTick = 0;
          mirror.needsFull = true;
          active = false;
          control = undefined;
          clearInput();
          ui.resetFeedback();
        }
        phase = lobby.phase;
        ui.lobby(lobby, connection.playerId);
        if (phase !== "playing") {
          clearInput();
        }
      } else if (message.type === "control") {
        const next = controlReader.read(message);
        if (next.roomEpoch !== connection.roomEpoch || next.roundId !== connection.roundId) {
          return;
        }
        if (next.controlEpoch !== control?.controlEpoch || next.life !== control.life) {
          clearInput();
        }
        control = next;
        if (
          next.driver !== "human" &&
          !ui.menu &&
          !document.hidden &&
          readyRound === connection.roundId &&
          performance.now() - lastResumeMs > 1000
        ) {
          lastResumeMs = performance.now();
          connection.send("resume");
        }
      } else if (message.type === "full") {
        lastSnapshotMs = performance.now();
        appliedInput = 0;
        mirror.applyFull(message, { roomEpoch: connection.roomEpoch, roundId: connection.roundId });
        receivedUpdates++;
        connection.observedTick = mirror.tick;
        requestedFull = false;
        clearInput();
        resetDisplay();
        if (readyRound !== connection.roundId) {
          void prepare();
        } else if (view && display) {
          view.reset(display);
          active = true;
          ui.canvas.focus();
        }
      } else if (message.type === "snapshot") {
        appliedInput = id.read(message.ack);
        lastSnapshotMs = performance.now();
        if (!Array.isArray(message.snapshots) || message.snapshots.length > 8) {
          throw new Error("Invalid frame batch");
        }
        receivedUpdates++;
        if (message.roundId !== connection.roundId) {
          return;
        }
        let pushed = false;
        for (const snapshot of message.snapshots) {
          const result = mirror.applySnapshot(snapshot);
          if (!result) {
            requestFull();
            break;
          }
          connection.observedTick = mirror.tick;
          if (control && readyRound === connection.roundId && !document.hidden && !ui.menu) {
            try {
              timeline.push(
                mirror.render(control.tankId),
                mirror.tick,
                result.events,
                result.traces,
              );
              pushed = true;
            } catch {
              requestFull();
              break;
            }
          }
        }
        if (pushed) {
          timeline.arrive(lastSnapshotMs);
        }
      }
    },
  });
  const collect = (now: number) => {
    if (!activeInput() || !view || !display || !control) {
      return;
    }
    const position = display.viewer.position;
    const target = controls.touch.aiming
      ? view.touchAim(position, controls.touch.aimX, controls.touch.aimY)
      : view.aim(controls.nx, controls.ny);
    const angle = Math.atan2(target.x - position.x, target.z - position.z);
    const command = controls.command(angle);
    // Aim is immediate presentation feedback; only the server decides what the shot hits.
    display = {
      ...display,
      viewer: { ...display.viewer, aim: angle },
      tanks: display.tanks.map((tank) =>
        tank.id === display!.viewerId ? { ...tank, aim: angle } : tank,
      ),
    };
    if (command.mine) {
      pending.push({ type: "mine" });
    }
    if (command.ammoSelection !== undefined) {
      let weapon =
        typeof command.ammoSelection === "string"
          ? command.ammoSelection
          : (pendingWeapon ?? display.viewer.selectedAmmo);
      if (typeof command.ammoSelection === "number") {
        const index = AMMO_ORDER.indexOf(weapon as (typeof AMMO_ORDER)[number]);
        for (let offset = 1; offset <= AMMO_ORDER.length; offset++) {
          const candidate =
            AMMO_ORDER[
              (index + command.ammoSelection * offset + AMMO_ORDER.length) % AMMO_ORDER.length
            ];
          if (hasAmmo(display.viewer, candidate)) {
            weapon = candidate;
            break;
          }
        }
      }
      pendingWeapon = weapon;
      pending.push({ type: "ammo", weapon });
    }
    if (pending.length > MAX_ACTIONS) {
      pending = pending.slice(-MAX_ACTIONS);
    }
    const input = {
      controlEpoch: control.controlEpoch,
      moveX: command.moveX,
      moveZ: command.moveZ,
      aim: controls.touch.aiming ? { angle } : { x: target.x, z: target.z },
      fire: command.fire,
      actions: pending,
    };
    if (!inputCadence.due(input, now)) {
      return;
    }
    if (
      connection.send("input", encodeInput({ ...input, seq: seq + 1, observedTick: mirror.tick }))
    ) {
      seq++;
      inputCadence.sent(input, now);
      pending = [];
      pendingWeapon = undefined;
    }
  };
  const loop = (now: number) => {
    const dt = Math.min(0.1, Math.max(0, (now - last) / 1000));
    last = now;
    if (active && view && control && mirror.state && !document.hidden) {
      const updateStart = performance.now();
      if (!ui.menu) {
        const sample = timeline.read(now, connection.rtt, dt);
        display = sample.state;
        for (const event of sample.events) {
          const playerHit =
            (event.type === "hurt" || event.type === "death") &&
            event.owner === display.viewerId &&
            event.team !== display.viewer.team;
          view.event(event, playerHit);
          audio?.event(event, display.viewer.position, playerHit, event.id === display.viewerId);
          ui.event(event, display, view.damageAngle(event));
        }
      }
      if (display) {
        collect(now);
        const renderStart = performance.now();
        view.render(display, 1, dt);
        const renderCost = performance.now() - renderStart;
        ui.update(display, dt, connection.connected);
        stats?.frame(now, renderStart - updateStart, renderCost);
      }
    }
    touch.update();
    requestAnimationFrame(loop);
  };
  window.addEventListener("resize", () => view?.resize());
  document.addEventListener("visibilitychange", () => {
    clearInput();
    if (document.hidden) {
      connection.send("suspend");
      active = false;
    } else if (phase === "playing" && !ui.menu) {
      resume();
    }
  });
  window.addEventListener("pagehide", () => connection.stop(), { once: true });
  requestAnimationFrame(loop);
  if (selectedChoice) {
    join(selectedChoice);
  }
  if (import.meta.env.DEV) {
    Object.assign(window, {
      sloppyMultiplayer: {
        connection,
        mirror,
        get control() {
          return control;
        },
        get display() {
          return display;
        },
        get view() {
          return view;
        },
        controls,
        ui,
        pause,
        resume,
      },
    });
  }
}
