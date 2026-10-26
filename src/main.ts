import { loadGameOptions, type GameOptions } from "./game/game-options";
import { savedPreference, preferredGameMode } from "./game/player-preferences";
import { bindPlayModes, initialPlayMode } from "./game/play-modes";
import { JoinScreen, restoreChoices, takeSetupView, type SetupView } from "./game/join-screen";
import { StartMenu } from "./game/start-menu";
import { showFatalError, startupErrorMessage } from "./game/startup-error";
import { debugPage, isExtraLevel, mapOption } from "./game/map-options";
import { showExtraLevels } from "./game/map-picker";
import { isPhone } from "./game/phone-mode";
import { fitPanels, keepPanelsFitted } from "./game/panel-fit";
import type { RoomSelection } from "./net/pending-join";
import { isPlayerKind } from "./net/room-protocol";
import "./style.css";

const root = document.querySelector<HTMLDivElement>("#app")!;
// A room link opens Battle Setup with that room selected. When Battle Setup reloads
// into a room it chose, it stays up while the room loads, until the arena can draw.
const linkedRoom = new URLSearchParams(location.search).get("room")?.toUpperCase();
const setupView = linkedRoom ? takeSetupView(linkedRoom) : undefined;
// Offer the extra levels before any remembered choice picks one.
const startupOverlay = document.querySelector<HTMLElement>("#startup-overlay");
if (startupOverlay && debugPage(location.search)) {
  showExtraLevels(startupOverlay);
}
keepPanelsFitted();
fitPanels();
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
      showFatalError(
        root,
        "MULTIPLAYER DIDN'T LOAD",
        "The room's game files could not load. Check your connection and reload.",
      );
    });
}

declare global {
  interface Window {
    /** Scene texture files decoding by URL, each taken over once by the engine's texture
     * store (`texture_image` in crates/render/src/gpu/textures.rs). */
    sloppyTextureImages?: Map<string, Promise<HTMLImageElement>>;
  }
}

function startTextureDownloads(options: GameOptions): void {
  // Surfaces of the cottages and watchtowers, on the maps that build them. Kept local:
  // this module's startup code runs before its later top-level constants exist.
  const buildingTextures = [
    "textures/houses/shingles.webp",
    "textures/houses/clapboard.webp",
    "textures/houses/brick.webp",
    "textures/houses/stone.webp",
  ];
  const buildings = ["village", "stress-test", "superstress"].includes(options.mapMode);
  // Start scene image downloads and decodes alongside the engine request, before the
  // engine's scenery discovers them: they decode off the main thread while it builds
  // the arena, and small late requests otherwise delay warm-up. From script rather than
  // <link rel=preload>: WebKit never handed a preload response to the engine's later
  // fetch(), so it downloaded every texture twice.
  const images = (window.sloppyTextureImages ??= new Map());
  for (const path of [
    "textures/pickups/atlas.webp",
    "textures/tanks/armor-camo.webp",
    "textures/tanks/armor-wear.webp",
    // The wrecks' burnt paint, and the bark and rings of loose logs.
    "textures/tanks/armor-scorch.webp",
    "textures/trees/bark.webp",
    "textures/trees/rings.webp",
    // Every ground but the harbor's concrete quay: roads, yards and quarry grit.
    ...(options.mapMode === "harbor" ? [] : ["textures/ground/packed-dirt.webp"]),
    "textures/houses/siding.webp",
    "textures/walls/weathered-concrete.webp",
    "textures/barrels/painted-drum.webp",
    ...(options.mapMode === "village"
      ? [
          "textures/ground/dry-grass.webp",
          "textures/ground/village-ground.webp",
          // Crowns draw from the tree atlas; trunks and stumps from their own bark.
          "textures/trees/atlas.webp",
          "textures/water/normals.webp",
          "textures/trees/birch.webp",
          "textures/wood/timber.webp",
        ]
      : []),
    ...(options.mapMode === "harbor"
      ? [
          "textures/harbor/dock.webp",
          "textures/harbor/steel.webp",
          "textures/water/normals.webp",
          "textures/harbor/container.webp",
          "textures/harbor/container-ribbed.webp",
          "textures/wood/timber.webp",
        ]
      : []),
    ...(options.mapMode === "quarry"
      ? [
          "textures/quarry/sandstone.webp",
          "textures/harbor/steel.webp",
          "textures/wood/timber.webp",
        ]
      : []),
    ...(isExtraLevel(options.mapMode)
      ? ["textures/ground/dry-grass.webp", "textures/wood/timber.webp"]
      : []),
    ...(buildings ? buildingTextures : []),
  ]) {
    const url = `${import.meta.env.BASE_URL}${path}`;
    if (!images.has(url)) {
      const image = new Image();
      image.src = url;
      const decoded = image.decode().then(() => image);
      // The engine decodes a texture again if its early decode fails.
      decoded.catch(() => {});
      images.set(url, decoded);
    }
  }
}

/** `linkedRoom` comes from a room link; `view` holds the choices of a player who left
 * that room or could not join it, and why. */
function startBattleSetup(linkedRoom?: string, view?: Partial<SetupView>): void {
  const seed = Math.floor(Math.random() * 1000000);
  const options = loadGameOptions(seed, location.search);
  // A player back from a room keeps the map Battle Setup showed them, over a link's
  // `?map=`, so GO and a new room play the map the restored menu shows.
  const restoredMap = mapOption(view?.map ?? null)?.id;
  if (restoredMap && (debugPage(location.search) || !isExtraLevel(restoredMap))) {
    options.mapMode = restoredMap;
    options.gameMode = isExtraLevel(restoredMap)
      ? "team"
      : preferredGameMode(savedPreference("game-mode"));
  }
  if (isPlayerKind(view?.kind)) {
    options.humanKind = view.kind;
  }
  if (isPhone()) {
    // Battle Setup on a phone offers the tank, map and format but no difficulty: its
    // bots are always Easy, and the saved difficulty stays untouched.
    options.difficulty = "easy";
  }
  const autoStart = new URLSearchParams(location.search).has("autoplay");
  const load = async (onStage: (stage: string) => void = () => {}) => {
    onStage("Downloading game files…");
    const { prepareGame } = await import("./game");
    return prepareGame(root, seed, () => options, onStage);
  };

  if (autoStart) {
    startTextureDownloads(options);
    const setup = document.querySelector<HTMLElement>("#startup-overlay");
    if (setup) {
      setup.style.display = "none";
    }
    void load()
      .then(async (game) => {
        await game.start(options, () => setup?.remove());
      })
      .catch((error: unknown) => {
        console.error("Game startup failed", error);
        showFatalError(
          root,
          "THE ARENA DIDN'T LOAD",
          startupErrorMessage(error, "The arena could not load. Check your connection and reload."),
        );
      });
    return;
  }
  const menu = new StartMenu(root, options, load, () => playModes.close());
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
    startTextureDownloads(options);
    // A frame leaves Battle Setup a paint before any engine work begins. The engine
    // compiles, the arena's texture bakes start in workers once it has, and the arena
    // builds as soon as they have loaded: waiting a second frame cost WebKit 40-60 ms
    // of an idle main thread. A failed load is the arena's to report.
    requestAnimationFrame(() => {
      void import("./game")
        .then(({ loadArenaEngine }) => loadArenaEngine(options.mapMode))
        .catch(() => {})
        .then(() => menu.prepare())
        .catch(() => {});
    });
  };
  const playModes = bindPlayModes(
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
