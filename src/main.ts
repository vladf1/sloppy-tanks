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
  const setup = document.querySelector<HTMLElement>("#startup-overlay");
  if (setup) {
    setup.style.display = "none";
  }
  void load()
    .then((start) => {
      start(options);
      setup?.remove();
    })
    .catch((error: unknown) => {
      console.error("Game startup failed", error);
      const loading = document.querySelector("#loading p");
      if (loading) {
        loading.textContent = "The arena could not load. Please reload to try again.";
      } else {
        root.textContent = "The arena could not load. Please reload to try again.";
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
