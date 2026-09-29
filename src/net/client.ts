import init, { NetGame } from "../generated/engine/engine.js";
import wasmUrl from "../generated/engine/engine_bg.wasm?url";
import { Controls } from "../game/controls";
import { AudioSystem } from "../game/audio";
import { Cockpit } from "../game/cockpit";
import { TouchModeController, type TouchState } from "../game/touch-mode";
import { returnToSetup, type JoinScreen } from "../game/join-screen";
import { nextPrepareStep } from "../game/task-yield";
import { startTextureBake } from "../game/texture-bake";
import { INPUT, type MatchState as Match } from "../game/engine-api";
import { NerdStats } from "../game/nerd-stats";
import { NetworkUI, type Hud, type HudEvent } from "./network-ui";
import { networkStatsSections, type NetworkStatsSource } from "./network-stats";
import {
  ROOM_CODE,
  TRANSPORT_DELAY_PARAMS,
  type ConnectionEnd,
  type JoinChoice,
  type Lobby,
} from "./room-protocol";
import { serverAddress } from "./server-address";
import { roomAddress, takePendingJoin, type RoomSelection } from "./pending-join";

/** `NetGame.frame` result slots (`net_frame_slot` in `crates/web/src/net_game.rs`). */
const FRAME = {
  phase: 0,
  cockpit: 2,
  hullAngle: 3,
  events: 4,
  hudDue: 6,
  simMs: 7,
  renderMs: 8,
  firstPerson: 9,
  dt: 10,
  drawn: 11,
  pointerFree: 12,
} as const;
/** The engine asks for timers at least this often (heartbeat, reconnect, dev delay). */
const POLL_MS = 250;
/** How often, in frames, to ask the engine for a recorded GPU error. */
const ERROR_CHECK_EVERY_FRAMES = 30;
/** A socket this far behind on sends is closed and reconnected instead. */
const MAX_BUFFERED_BYTES = 16_384;
/** Pipelines compiled per task while preparing an arena. */
const PREPARE_BUDGET = 4;
/** Renderer resolution cap from the display pixel ratio (`CAMERA.max_pixel_ratio`). */
const MAX_PIXEL_RATIO = 1.5;

type Action =
  | { type: "open"; socket: number; url: string }
  | { type: "send"; socket: number; text: string }
  | { type: "close"; socket: number }
  | { type: "saveSeat"; token: string; roomEpoch: string }
  | { type: "forgetSeat" };
type Notice =
  | { type: "status"; text: string; connected: boolean }
  | { type: "notice"; text: string }
  | ({ type: "ended" } & ConnectionEnd)
  | { type: "lobby"; lobby: Lobby; playerId: string }
  | { type: "result"; match: Match; team: number }
  | { type: "resetFeedback" | "clearInput" | "reveal" | "prepare" | "baselineShown" }
  | { type: "arenaFailed"; error: string };
interface DrainedEvents {
  listener: { x: number; z: number };
  listenerRight: { x: number; z: number };
  events: HudEvent[];
}

let engine: Promise<unknown> | undefined;

/** Join the room chosen on Battle Setup. The room page builds out of sight and replaces
 * `setup` only once its arena can draw, so the arena's first stalled frames never show;
 * a join or room that gives up returns to Battle Setup instead. Without `selection`, the
 * choices come from the page that reloaded into the room.
 *
 * The Rust engine (`NetGame`) runs the connection, replication, interpolation, input and
 * drawing; this page adapter owns the sockets, timers, storage and DOM. */
export async function startMultiplayer(
  app: HTMLElement,
  setup: JoinScreen,
  selection?: RoomSelection,
): Promise<void> {
  const room = selection?.room ?? new URLSearchParams(location.search).get("room")?.toUpperCase();
  if (!room || !ROOM_CODE.test(room)) {
    // Battle Setup's multiplayer tab lists the rooms that exist.
    const url = new URL(location.href);
    url.searchParams.delete("room");
    url.searchParams.set("multiplayer", "");
    location.replace(url);
    return;
  }
  const address = serverAddress();
  if (!address) {
    app.textContent =
      "Multiplayer isn't enabled on this site yet. Open the development site to play with friends.";
    return;
  }
  if (selection) {
    history.replaceState(null, "", roomAddress(room));
  }
  engine ??= init({ module_or_path: wasmUrl });
  await engine;
  const pending = selection ? undefined : takePendingJoin();
  const choiceJson = selection
    ? JSON.stringify(selection.choice)
    : NetGame.pending_join(pending, room);
  if (!choiceJson) {
    setup.fail("Choose your tank and join the room again.");
    return;
  }
  const selectedChoice = JSON.parse(choiceJson) as JoinChoice;
  const server = address.href.replace(/\/$/, "");
  const seatKey = "sloppy-seat:" + server + ":" + room;
  let joining: JoinScreen | undefined = setup;
  const root = app.appendChild(document.createElement("div"));
  root.hidden = true;
  /** Battle Setup, with this room selected, is where to join again. */
  const backToSetup = (notice: string) => {
    if (joining) {
      joining.fail(notice);
    } else {
      returnToSetup({
        room,
        joining: false,
        notice,
        name: selectedChoice.name,
        kind: selectedChoice.kind,
        team: selectedChoice.team === undefined ? "auto" : String(selectedChoice.team),
      });
    }
  };
  const ui = new NetworkUI(root, room, {
    choose(choice) {
      game.choose(choice.team ?? -1, choice.kind, performance.now());
      pump();
    },
    settings(mapMode, difficulty, humansOnly, roundMinutes) {
      try {
        game.settings(
          JSON.stringify({ mapMode, difficulty, humansOnly, roundMinutes }),
          performance.now(),
        );
      } catch (error) {
        console.error("Invalid room settings", error);
      }
      pump();
    },
    start() {
      controls.clear();
      game.start(performance.now());
      pump();
    },
    pause,
    resume,
    end() {
      controls.clear();
      game.end(performance.now());
      pump();
    },
    rejoin() {
      // A seat the server still holds resumes; otherwise this takes a new one in the room.
      game.rejoin(performance.now());
      pump();
    },
    setup: backToSetup,
    leave() {
      game.leave(performance.now());
      pump();
      const url = new URL(location.href);
      for (const key of ["room", ...TRANSPORT_DELAY_PARAMS]) {
        url.searchParams.delete(key);
      }
      url.searchParams.set("multiplayer", "");
      location.assign(url);
    },
    ammo(weapon) {
      game.select_ammo(weapon);
    },
    volume(value) {
      audio.volume(value);
      try {
        localStorage.setItem("sloppy-volume", String(value));
      } catch {
        /* Optional preference. */
      }
    },
  });
  const cssSize = (): [number, number] => {
    const canvas = ui.canvas;
    // A room still joining behind Battle Setup is hidden; it draws at window size.
    return canvas.clientWidth && canvas.clientHeight
      ? [canvas.clientWidth, canvas.clientHeight]
      : [innerWidth, innerHeight];
  };
  const params = new URLSearchParams(location.search);
  const delay = import.meta.env.DEV
    ? Object.fromEntries(
        TRANSPORT_DELAY_PARAMS.filter((key) => params.has(key)).map((key) => [
          key,
          params.get(key),
        ]),
      )
    : {};
  let game: NetGame;
  try {
    const [width, height] = cssSize();
    game = await NetGame.create(
      ui.canvas,
      JSON.stringify({
        server,
        room,
        savedSeat: savedSeat(seatKey),
        ...delay,
        assetBase: import.meta.env.BASE_URL,
        cssWidth: width,
        cssHeight: height,
        pixelRatio: devicePixelRatio,
      }),
    );
  } catch (error) {
    console.error("Multiplayer graphics failed", error);
    root.remove();
    backToSetup("The arena could not load. Try again, or play single player.");
    return;
  }
  const audio = new AudioSystem();
  let hud: Hud | undefined;
  let hudDt = 0;
  let zoom = 0;
  let phase: Lobby["phase"] = "lobby";
  let lastResult: Float32Array = new Float32Array(0);
  const input = new Float32Array(INPUT.length);
  const sockets = new Map<number, WebSocket>();
  /** Checks can observe displayed events (dev builds only). */
  let onEvent: ((event: HudEvent) => void) | undefined;
  const activeInput = () => game.active_input();
  function pause(): void {
    if (joining || !game.pause(performance.now())) {
      return;
    }
    ui.setMenu(true);
    controls.clear();
    pump();
  }
  function resume(): void {
    controls.clear();
    const open = game.resume(performance.now());
    ui.setMenu(open);
    // A click right after RESUME takes the pointer back in first person.
    if (lastResult.length) {
      controls.holdPointer(lastResult[FRAME.firstPerson] === 1, open);
    }
    pump();
  }
  const controls = new Controls(ui.canvas, pause, (amount) => (zoom += amount), activeInput, false);
  const toggleView = () => {
    if (phase !== "playing" || ui.menu) {
      return;
    }
    controls.holdPointer(game.toggle_first_person());
    controls.capturePointer();
  };
  controls.toggleView = toggleView;
  const cockpit = new Cockpit(ui.root, toggleView);
  const touch = new TouchModeController(
    root,
    controls,
    {
      get human() {
        return { mineCooldown: hud?.human.mineCooldown ?? 0 };
      },
      get match(): TouchState["match"] {
        return { phase: ui.menu ? "paused" : phase === "playing" ? "playing" : "ready" };
      },
    },
    (amount) => (zoom += amount),
  );
  const stats = new NerdStats(
    root,
    networkStatsSections(
      (now) => JSON.parse(game.stats_json(now)) as NetworkStatsSource,
      () => Math.min(devicePixelRatio, MAX_PIXEL_RATIO),
    ),
    () => lastResult[FRAME.drawn] === 1 && !ui.menu && phase === "playing",
    { network: true },
  );
  const reveal = () => {
    if (joining) {
      joining.done();
      joining = undefined;
      root.hidden = false;
      resize();
    }
  };
  const showStatus = (text: string, connected: boolean) => {
    ui.status(text, connected);
    joining?.status(text);
  };

  const openSocket = (id: number, url: string) => {
    const socket = new WebSocket(url);
    sockets.set(id, socket);
    socket.onopen = () => {
      game.socket_opened(id, performance.now());
      pump();
    };
    socket.onmessage = (event) => {
      // Binary frames are never valid; the engine rejects the placeholder.
      game.socket_message(
        id,
        typeof event.data === "string" ? event.data : "\u0000",
        performance.now(),
      );
      pump();
    };
    socket.onclose = (event) => {
      sockets.delete(id);
      game.socket_closed(id, event.code, performance.now());
      pump();
    };
    socket.onerror = () => {
      /* onclose owns retry; browsers hide failed-upgrade details. */
    };
  };
  const perform = (action: Action) => {
    switch (action.type) {
      case "open":
        openSocket(action.socket, action.url);
        break;
      case "send": {
        const socket = sockets.get(action.socket);
        if (socket?.readyState !== WebSocket.OPEN) {
          break;
        }
        if (socket.bufferedAmount > MAX_BUFFERED_BYTES) {
          socket.close();
        } else {
          socket.send(action.text);
        }
        break;
      }
      case "close":
        sockets.get(action.socket)?.close();
        break;
      case "saveSeat":
        try {
          sessionStorage.setItem(
            seatKey,
            JSON.stringify({ token: action.token, roomEpoch: action.roomEpoch }),
          );
        } catch {
          /* Session can continue without storage. */
        }
        break;
      case "forgetSeat":
        try {
          sessionStorage.removeItem(seatKey);
        } catch {
          /* Optional storage. */
        }
        break;
    }
  };
  const handle = (notice: Notice) => {
    switch (notice.type) {
      case "status":
        showStatus(notice.text, notice.connected);
        break;
      case "notice":
        ui.notice(notice.text);
        break;
      case "ended":
        controls.clear();
        // A join still behind Battle Setup reports there; a room on screen says what
        // happened over the frozen arena and offers the way back that fits.
        if (joining) {
          joining.fail(notice.text);
        } else {
          ui.ended(notice);
        }
        break;
      case "lobby":
        phase = notice.lobby.phase;
        ui.lobby(notice.lobby, notice.playerId);
        break;
      case "result":
        ui.result(notice.match, notice.team);
        break;
      case "resetFeedback":
        ui.resetFeedback();
        break;
      case "clearInput":
        controls.clear();
        break;
      case "reveal":
        reveal();
        break;
      case "prepare":
        void prepare();
        break;
      case "arenaFailed":
        console.error("Multiplayer graphics failed", notice.error);
        backToSetup("The arena could not load. Try again, or play single player.");
        break;
      case "baselineShown":
        ui.canvas.focus();
        break;
    }
  };
  let pumping = false;
  /** Perform the engine's socket and storage work, then its UI notices, until both are
   * empty; handlers may call back into the engine. */
  function pump(): void {
    if (pumping) {
      return;
    }
    pumping = true;
    try {
      for (;;) {
        const actions = JSON.parse(game.take_actions()) as Action[];
        actions.forEach(perform);
        const notices = JSON.parse(game.take_notices()) as Notice[];
        notices.forEach(handle);
        if (!actions.length && !notices.length) {
          break;
        }
      }
    } finally {
      pumping = false;
    }
  }
  let preparing = false;
  /** Compile the arena between tasks, so the join screen keeps painting. */
  async function prepare(): Promise<void> {
    if (preparing) {
      return;
    }
    preparing = true;
    try {
      let gpuPending = false;
      for (;;) {
        await nextPrepareStep(gpuPending);
        startTextureBake(game, wasmUrl);
        const [, , , done, waitingForGpu] = game.prepare_step(PREPARE_BUDGET, performance.now());
        if (done) {
          break;
        }
        gpuPending = waitingForGpu === 1;
      }
    } finally {
      preparing = false;
    }
    pump();
  }
  const resize = () => {
    const [width, height] = cssSize();
    game.resize(width, height, devicePixelRatio, false);
  };
  const route = (drained: DrainedEvents) => {
    audio.listenerRight = drained.listenerRight;
    for (const event of drained.events) {
      audio.event(event, drained.listener, event.playerHit, event.own);
      if (hud) {
        ui.event(event, hud);
      }
      onEvent?.(event);
    }
  };
  let stopped = false;
  let frames = 0;
  /** A GPU validation error or device loss: stop drawing, give up the connection and
   * say so, rather than throwing every frame behind a frozen view. */
  const fail = (error: unknown) => {
    stopped = true;
    console.error("The room page stopped", error);
    clearInterval(poll);
    controls.clear();
    controls.holdPointer(false, true);
    game.stop();
    pump();
    ui.ended({
      cause: "renderer",
      text: "The renderer stopped. Reload the page to play again.",
    });
  };
  const loop = (now: number) => {
    if (stopped) {
      return;
    }
    requestAnimationFrame(loop);
    controls.takeInput(input);
    input[INPUT.zoom] = zoom;
    zoom = 0;
    let result: Float32Array;
    try {
      result = game.frame(now, input);
      // Most GPU errors arrive asynchronously, so the engine records them for polling.
      if (frames++ % ERROR_CHECK_EVERY_FRAMES === 0) {
        const error = game.error();
        if (error) {
          throw new Error(error);
        }
      }
    } catch (error) {
      fail(error);
      return;
    }
    lastResult = result;
    // Any menu (including ones the server opens) and disconnects free the pointer;
    // a death keeps it captured for the respawn. Mouse travel while input is off is
    // dropped so the turret never jumps. Held after the frame, so RESUME and V set
    // it at once and no stale frame overrides them.
    controls.holdPointer(result[FRAME.firstPerson] === 1, result[FRAME.pointerFree] === 1);
    pump();
    if (result[FRAME.drawn]) {
      hudDt += result[FRAME.dt];
      if (result[FRAME.hudDue]) {
        hud = (JSON.parse(game.hud_json()) as Hud | null) ?? hud;
      }
      if (result[FRAME.events] > 0) {
        route(JSON.parse(game.drain_events()) as DrainedEvents);
      }
      if (hud && result[FRAME.hudDue]) {
        ui.update(hud, hudDt, game.connected());
        hudDt = 0;
      }
      cockpit.update(
        result[FRAME.cockpit] === 1,
        result[FRAME.hullAngle],
        controls.aimWaitsForClick,
      );
      stats.frame(now, result[FRAME.simMs], result[FRAME.renderMs]);
    } else if (result[FRAME.events] > 0) {
      game.drain_events();
    }
    touch.update();
  };
  const poll = setInterval(() => {
    game.poll(performance.now());
    pump();
  }, POLL_MS);
  new ResizeObserver(resize).observe(ui.canvas);
  window.addEventListener("resize", resize);
  document.addEventListener("visibilitychange", () => {
    controls.clear();
    game.set_hidden(document.hidden, performance.now());
    pump();
  });
  window.addEventListener(
    "pagehide",
    () => {
      clearInterval(poll);
      game.stop();
      pump();
    },
    { once: true },
  );
  requestAnimationFrame(loop);
  audio.volume(Number(localStorage.getItem("sloppy-volume") ?? 0.6));
  audio.start();
  game.connect(choiceJson, performance.now());
  pump();
  if (import.meta.env.DEV) {
    const debug = () =>
      JSON.parse(game.debug_json()) as {
        connection: Record<string, unknown>;
        tick: number;
        control: unknown;
        display: unknown;
        prepared: boolean;
        view: unknown;
      };
    const current = () => [...sockets.values()].at(-1);
    Object.assign(window, {
      sloppyMultiplayer: {
        game,
        get connection() {
          return { ...debug().connection, socket: current() };
        },
        get mirror() {
          return { tick: debug().tick };
        },
        get control() {
          return debug().control ?? undefined;
        },
        get display() {
          return debug().display ?? undefined;
        },
        /** The arena, once prepared. */
        get view() {
          const state = debug();
          return state.prepared ? state.view : undefined;
        },
        get hud() {
          return hud;
        },
        set onEvent(listener: ((event: HudEvent) => void) | undefined) {
          onEvent = listener;
        },
        controls,
        ui,
        pause,
        resume,
      },
    });
  }
}

/** A seat this tab held in the room before a reload; the engine checks it. */
function savedSeat(key: string): { token: string; roomEpoch: string } | undefined {
  try {
    const saved = JSON.parse(sessionStorage.getItem(key) ?? "null") as {
      token?: unknown;
      roomEpoch?: unknown;
    } | null;
    return typeof saved?.token === "string" && typeof saved.roomEpoch === "string"
      ? { token: saved.token, roomEpoch: saved.roomEpoch }
      : undefined;
  } catch {
    return undefined;
  }
}
