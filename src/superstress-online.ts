import { multiplayerAvailable } from "./game/play-modes";

/** The single-player yard opens its page's Battle Setup multiplayer tab, which lists,
 * creates and joins only Scrap Yard rooms. */
export function offerOnlinePlay(root: HTMLElement): void {
  const actions = root.querySelector(".hud-actions");
  if (!multiplayerAvailable() || !actions || actions.querySelector("#play-online")) {
    return;
  }
  const button = document.createElement("button");
  button.id = "play-online";
  button.className = "quiet";
  button.type = "button";
  button.textContent = "PLAY ONLINE";
  button.title = "Find or create a Scrap Yard room to play with friends";
  button.addEventListener("click", () => {
    const url = new URL(location.href);
    url.searchParams.set("multiplayer", "");
    location.assign(url);
  });
  actions.prepend(button);
}
