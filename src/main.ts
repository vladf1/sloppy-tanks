import { initialGameOptions } from "./game/game-options";
import { StartMenu } from "./game/start-menu";
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
const load = async () => {
  const { prepareGame } = await import("./game");
  return prepareGame(root, seed, () => options);
};

if (autoStart) {
  void load()
    .then((start) => start(options))
    .catch((error: unknown) => {
      console.error("Game startup failed", error);
      const loading = document.querySelector("#loading p");
      if (loading) {
        loading.textContent = "The arena could not load. Please reload to try again.";
      }
    });
} else {
  const menu = new StartMenu(root, options, load);
  // Let the interactive menu paint before evaluating or constructing the engine.
  requestAnimationFrame(() => {
    const loading = document.querySelector<HTMLElement>("#loading");
    loading?.classList.add("leaving");
    window.setTimeout(() => loading?.remove(), 180);
    window.setTimeout(() => {
      void menu.prepare().catch(() => {});
    }, 0);
  });
}
