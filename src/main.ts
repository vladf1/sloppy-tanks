import { initialGameOptions } from "./game/game-options";
import { StartMenu } from "./game/start-menu";
import { startupErrorMessage } from "./game/startup-error";
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
// Start the few critical image downloads alongside the engine/WASM request.
// Image preloads share TextureLoader's browser cache; no second fetch/decode path.
for (const path of [
  "tanks/armor-wear",
  "ground/packed-dirt",
  ...(options.mapMode === "village"
    ? ["ground/dry-grass", "trees/conifer-spray", "water/normals"]
    : []),
]) {
  const link = document.createElement("link");
  link.rel = "preload";
  link.as = "image";
  link.crossOrigin = "anonymous";
  link.href = `${import.meta.env.BASE_URL}textures/${path}.webp`;
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
