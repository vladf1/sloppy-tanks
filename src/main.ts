import { initialGameOptions } from "./game/game-options";
import { StartMenu } from "./game/start-menu";
import { startupErrorMessage } from "./game/startup-error";
import { PICKUP_ATLAS_PATH } from "./game/pickup-atlas";
import "./style.css";

const root = document.querySelector<HTMLDivElement>("#app")!;
const seed = Math.floor(Math.random() * 1000000);
const options = initialGameOptions(
  seed,
  location.search,
  localStorage.getItem("sloppy-difficulty"),
);
const autoStart =
  document.documentElement.dataset.scenario === "stress-test" ||
  new URLSearchParams(location.search).has("autoplay");
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
const load = async (onStage: (stage: string) => void = () => {}) => {
  onStage("Downloading game files…");
  const { prepareGame } = await import("./game");
  return prepareGame(root, seed, () => options, onStage);
};

if (autoStart) {
  const setup = document.querySelector<HTMLElement>("#startup-overlay");
  if (setup) {
    setup.style.display = "none";
  }
  void load()
    .then(async (start) => {
      await start(options);
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
} else {
  const menu = new StartMenu(root, options, load);
  // A second frame leaves a paint opportunity before any engine work begins.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void menu.prepare().catch(() => {});
    });
  });
}
