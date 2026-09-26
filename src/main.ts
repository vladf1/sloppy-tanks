import { initialGameOptions, type GameOptions } from "./game/game-options";
import { bindPlayModes, initialPlayMode } from "./game/play-modes";
import { JoinScreen, restoreChoices, takeSetupView, type SetupView } from "./game/join-screen";
import { StartMenu } from "./game/start-menu";
import { startupErrorMessage } from "./game/startup-error";
import { PICKUP_ATLAS_PATH } from "./game/pickup-atlas";
import type { RoomSelection } from "./net/pending-join";
import type { PlayerVehicleKind } from "./game/types";
import "./style.css";

const root = document.querySelector<HTMLDivElement>("#app")!;
const PLAYER_KINDS: readonly string[] = [
  "scout",
  "balanced",
  "heavy",
] satisfies PlayerVehicleKind[];
// A room link opens Battle Setup with that room selected. When Battle Setup reloads
// into a room it chose, it stays up while the room loads, until the arena can draw.
const linkedRoom = new URLSearchParams(location.search).get("room")?.toUpperCase();
const setupView = linkedRoom ? takeSetupView(linkedRoom) : undefined;
const joiningSetup = document.querySelector<HTMLElement>("#startup-overlay .start");
if (linkedRoom && setupView?.joining && joiningSetup) {
  startMultiplayer(JoinScreen.resume(joiningSetup, { ...setupView, room: linkedRoom }));
} else {
  startBattleSetup(linkedRoom, setupView);
}

function startMultiplayer(joining: JoinScreen, selection?: RoomSelection): void {
  void import("./net/client")
    .then(({ startMultiplayer }) => startMultiplayer(root, joining, selection))
    .catch((error: unknown) => {
      console.error("Multiplayer startup failed", error);
      root.textContent = "Multiplayer could not load. Reload to try again.";
    });
}

function preloadImages(options: GameOptions): void {
  // Start scene image downloads alongside the engine/WASM request, before model
  // construction discovers them. Small late requests otherwise delay warm-up.
  // Image preloads share TextureLoader's browser cache; no second fetch/decode path.
  for (const path of [
    PICKUP_ATLAS_PATH,
    "textures/tanks/armor-wear.webp",
    "textures/ground/packed-dirt.webp",
    "textures/houses/siding.webp",
    "textures/walls/weathered-concrete.webp",
    "textures/barrels/painted-drum.webp",
    ...(options.mapMode === "village"
      ? [
          "textures/ground/dry-grass.webp",
          "textures/trees/conifer-spray.webp",
          "textures/water/normals.webp",
          "textures/houses/shingles.webp",
          "textures/trees/birch.webp",
          "textures/trees/leaves.webp",
        ]
      : []),
    ...(options.mapMode === "harbor"
      ? ["textures/harbor/dock.webp", "textures/harbor/steel.webp", "textures/water/normals.webp"]
      : []),
    ...(options.mapMode === "quarry" ? ["textures/quarry/sandstone.webp"] : []),
  ]) {
    const link = document.createElement("link");
    link.rel = "preload";
    link.as = "image";
    link.crossOrigin = "anonymous";
    link.href = `${import.meta.env.BASE_URL}${path}`;
    document.head.append(link);
  }
}

/** `linkedRoom` comes from a room link; `view` holds the choices of a player who left
 * that room or could not join it, and why. */
function startBattleSetup(linkedRoom?: string, view?: Partial<SetupView>): void {
  const seed = Math.floor(Math.random() * 1000000);
  const options = initialGameOptions(
    seed,
    location.search,
    localStorage.getItem("sloppy-difficulty"),
    localStorage.getItem("sloppy-map"),
  );
  if (view?.kind && PLAYER_KINDS.includes(view.kind)) {
    options.humanKind = view.kind as PlayerVehicleKind;
  }
  const autoStart =
    document.documentElement.dataset.scenario !== undefined ||
    new URLSearchParams(location.search).has("autoplay");
  const load = async (onStage: (stage: string) => void = () => {}) => {
    onStage("Downloading game files…");
    const { prepareGame } = await import("./game");
    return prepareGame(root, seed, () => options, onStage);
  };

  if (autoStart) {
    preloadImages(options);
    const setup = document.querySelector<HTMLElement>("#startup-overlay");
    if (setup) {
      setup.style.display = "none";
    }
    void load()
      .then(async (game) => {
        await game.start(options);
        setup?.remove();
      })
      .catch((error: unknown) => {
        console.error("Game startup failed", error);
        const loading = document.querySelector("#loading p");
        const message = startupErrorMessage(
          error,
          "The arena could not load. Please reload to try again.",
        );
        if (loading) {
          loading.textContent = message;
        } else {
          root.textContent = message;
        }
      });
    return;
  }
  const menu = new StartMenu(root, options, load);
  const setup = menu.overlay.querySelector<HTMLElement>(".start")!;
  if (view) {
    restoreChoices(setup, view);
  }
  // Single player builds its arena behind the menu; a page that has one reloads
  // into a multiplayer room rather than running a second renderer.
  let arenaStarted = false;
  const prepareArena = () => {
    if (arenaStarted) {
      return;
    }
    arenaStarted = true;
    preloadImages(options);
    // A second frame leaves a paint opportunity before any engine work begins.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        void menu.prepare().catch(() => {});
      });
    });
  };
  bindPlayModes(
    setup,
    initialPlayMode(location.search),
    {
      choices: () => options,
      single: prepareArena,
      enterRoom(selection, reload) {
        const joining = JoinScreen.start(
          setup,
          selection.room,
          !!selection.choice.create,
          arenaStarted,
        );
        if (arenaStarted) {
          reload();
        } else {
          startMultiplayer(joining, selection);
        }
      },
    },
    linkedRoom ? { room: linkedRoom, notice: view?.notice } : undefined,
  );
}
