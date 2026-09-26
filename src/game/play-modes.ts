import { showTankTeam, type GameOptions } from "./game-options";
import type { RoomBrowser } from "../net/room-browser";
import type { RoomSelection } from "../net/pending-join";
import type { RoomLink } from "../net/room-browser";

/** Battle Setup's two tabs. This module runs in the inline startup script, so all
 * multiplayer code stays behind the dynamic import below. */
export type PlayMode = "single" | "multiplayer";

export interface PlayModeHandlers {
  /** The tank and single-player team the setup currently shows. */
  choices(): Pick<GameOptions, "humanKind" | "humanTeam">;
  /** Runs whenever the single-player tab is shown. */
  single(): void;
  /** A room was chosen; `reload` enters it through a fresh page. */
  enterRoom(selection: RoomSelection, reload: () => void): void;
}

const TAB_KEYS = ["ArrowLeft", "ArrowRight", "Home", "End"];

export function multiplayerAvailable(): boolean {
  return (
    !!import.meta.env.VITE_MULTIPLAYER_URL || ["localhost", "127.0.0.1"].includes(location.hostname)
  );
}

/** A `?multiplayer` link or reload, or a room link, opens the multiplayer tab. */
export function initialPlayMode(search: string): PlayMode {
  const params = new URLSearchParams(search);
  return multiplayerAvailable() && (params.has("multiplayer") || params.has("room"))
    ? "multiplayer"
    : "single";
}

/** Keep a reload on the chosen tab. The flags also tell the page's head scripts not to
 * download single-player physics, so they must match the tab the page opens on. */
function rememberPlayMode(mode: PlayMode): void {
  const url = new URL(location.href);
  if (mode === "multiplayer") {
    url.searchParams.set("multiplayer", "");
  } else {
    url.searchParams.delete("multiplayer");
    url.searchParams.delete("room");
  }
  history.replaceState(history.state, "", url);
}

/** Pages without rooms (no game server, the stress test) offer only single player. */
export function removeMultiplayerTab(setup: HTMLElement): void {
  setup.querySelector(".play-tabs")?.remove();
  setup.querySelector("#multiplayer-panel")?.remove();
}

/** Show one tab's panel, following the WAI-ARIA tabs pattern. */
export function showPlayMode(setup: HTMLElement, mode: PlayMode): void {
  setup.dataset.play = mode;
  for (const tab of setup.querySelectorAll<HTMLButtonElement>('[role="tab"][data-play]')) {
    const selected = tab.dataset.play === mode;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
    const panel = setup.querySelector<HTMLElement>(`#${tab.getAttribute("aria-controls")}`);
    if (panel) {
      panel.hidden = !selected;
    }
  }
}

/** Switch panels from the tabs by pointer or keyboard; `change` runs after a switch. */
function bindTabs(setup: HTMLElement, initial: PlayMode, change: (mode: PlayMode) => void): void {
  const tabs = [...setup.querySelectorAll<HTMLButtonElement>('[role="tab"][data-play]')];
  const open = (tab: HTMLButtonElement) => {
    const mode = tab.dataset.play === "multiplayer" ? "multiplayer" : "single";
    if (setup.dataset.play !== mode) {
      showPlayMode(setup, mode);
      change(mode);
    }
  };
  showPlayMode(setup, initial);
  for (const tab of tabs) {
    tab.addEventListener("click", () => open(tab));
    tab.addEventListener("keydown", (event) => {
      if (!TAB_KEYS.includes(event.key)) {
        return;
      }
      event.preventDefault();
      const index = tabs.indexOf(tab);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? tabs.length - 1
            : (index + (event.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length;
      tabs[next].focus();
      open(tabs[next]);
    });
  }
}

/** Switch Battle Setup between its tabs. The room list loads on the first visit to
 * the multiplayer tab and polls only while that tab is shown; `close` stops it. A room
 * `link` is selected once the list shows it. */
export function bindPlayModes(
  setup: HTMLElement,
  initial: PlayMode,
  handlers: PlayModeHandlers,
  link?: RoomLink,
): { close(): void } {
  if (!multiplayerAvailable()) {
    removeMultiplayerTab(setup);
  }
  const panel = setup.querySelector<HTMLElement>("#multiplayer-panel");
  if (!panel) {
    handlers.single();
    return { close() {} };
  }
  let rooms: Promise<RoomBrowser> | undefined;
  let closed = false;
  // Auto keeps the single-player colour; a chosen team repaints the tank previews.
  const previewTeam = () => {
    const side = panel.querySelector<HTMLInputElement>('input[name="playerTeam"]:checked')?.value;
    return side === "0" ? 0 : side === "1" ? 1 : handlers.choices().humanTeam;
  };
  const openRooms = () => {
    rooms ??= import("../net/room-browser").then((lobby) => {
      const address = lobby.serverAddress();
      if (!address) {
        throw new Error("This site has no multiplayer server");
      }
      return new lobby.RoomBrowser(
        panel,
        address,
        () => handlers.choices().humanKind,
        (selection) => handlers.enterRoom(selection, () => lobby.joinAfterReload(selection)),
        link,
      );
    });
    rooms
      .then((browser) => {
        if (!closed && setup.dataset.play === "multiplayer") {
          browser.show();
        }
      })
      .catch((error: unknown) => {
        console.error("Room list could not load", error);
        panel.querySelector("#rooms-message")!.textContent =
          "Multiplayer could not load. Reload to try again.";
      });
  };
  const show = (mode: PlayMode) => {
    if (mode === "multiplayer") {
      showTankTeam(setup, previewTeam());
      openRooms();
    } else {
      showTankTeam(setup, handlers.choices().humanTeam);
      void rooms?.then((browser) => browser.hide());
      handlers.single();
    }
  };
  bindTabs(setup, initial, (mode) => {
    rememberPlayMode(mode);
    show(mode);
  });
  panel.querySelectorAll('input[name="playerTeam"]').forEach((input) => {
    input.addEventListener("change", () => showTankTeam(setup, previewTeam()));
  });
  show(initial);
  return {
    close() {
      closed = true;
      void rooms?.then((browser) => browser.close());
    },
  };
}
