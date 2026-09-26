import { showTank, showTankTeam, shownTankTeam } from "./game-options";
import { showPlayMode } from "./play-modes";

/** Battle Setup's choices, kept across a reload into a room or back out of one.
 * Display only: the join itself uses the validated choice from `net/pending-join.ts`. */
export interface SetupView {
  room: string;
  /** The reloaded page keeps joining the room behind the setup. */
  joining: boolean;
  creating?: boolean;
  /** Why the page came back to Battle Setup, shown in the room list. */
  notice?: string;
  name?: string;
  /** A `playerTeam` radio value: "auto", "0" or "1". */
  team?: string;
  kind?: string;
  previewTeam?: 0 | 1;
  roomMap?: string;
  roundMinutes?: string;
  humansOnly?: boolean;
}

const SETUP_VIEW_KEY = "sloppy-setup-view";
/** Everything except the room list's progress message stops taking input. */
const INERT_WHILE_JOINING = [
  ".play-tabs",
  ".tank-setting",
  ".driver-name",
  ".team-choice",
  "#refresh-rooms",
  ".room-actions",
  ".room-create",
].join();

/** A room's link. It opens Battle Setup with the room selected, or the room itself
 * while a join from Battle Setup is under way. */
export function roomAddress(room: string): URL {
  const url = new URL(location.href);
  url.searchParams.delete("multiplayer");
  url.searchParams.set("room", room);
  return url;
}

function storeSetupView(view: SetupView): void {
  try {
    sessionStorage.setItem(SETUP_VIEW_KEY, JSON.stringify(view));
  } catch {
    /* The next page keeps its defaults; the room link still selects the room. */
  }
}

/** Leave the room page for Battle Setup, with the room selected and `view.notice`
 * explaining why. */
export function returnToSetup(view: SetupView): void {
  storeSetupView(view);
  location.replace(roomAddress(view.room));
}

/** The view stored for this room. Read once, so a later reload starts afresh. */
export function takeSetupView(room: string): Partial<SetupView> | undefined {
  try {
    const view = JSON.parse(sessionStorage.getItem(SETUP_VIEW_KEY) ?? "null") as Partial<SetupView>;
    sessionStorage.removeItem(SETUP_VIEW_KEY);
    return view?.room === room ? view : undefined;
  } catch {
    return undefined;
  }
}

/** Put remembered form choices back; a missing one keeps the page's default. */
export function restoreChoices(setup: HTMLElement, view: Partial<SetupView>): void {
  const check = (name: string, value: unknown) => {
    if (typeof value === "string") {
      setup.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`).forEach((input) => {
        input.checked = input.value === value;
      });
    }
  };
  if (typeof view.name === "string") {
    setup.querySelector<HTMLInputElement>("#player-name")!.value = view.name;
  }
  check("playerTeam", view.team);
  check("roomMap", view.roomMap);
  if (typeof view.roundMinutes === "string") {
    setup.querySelector<HTMLInputElement>("#create-round-minutes")!.value = view.roundMinutes;
  }
  if (typeof view.humansOnly === "boolean") {
    setup.querySelector<HTMLInputElement>("#create-humans-only")!.checked = view.humansOnly;
  }
}

function captureSetup(setup: HTMLElement): Omit<SetupView, "room" | "joining"> {
  const value = (selector: string) => setup.querySelector<HTMLInputElement>(selector)?.value;
  return {
    name: value("#player-name"),
    team: value('input[name="playerTeam"]:checked'),
    kind: setup.querySelector<HTMLElement>("[data-kind].selected")?.dataset.kind,
    previewTeam: shownTankTeam(setup),
    roomMap: value('input[name="roomMap"]:checked'),
    roundMinutes: value("#create-round-minutes"),
    humansOnly: setup.querySelector<HTMLInputElement>("#create-humans-only")?.checked,
  };
}

/** Battle Setup while a chosen room loads out of sight. It stays on screen, reporting
 * progress in the room list, until the room page can draw its first frame. */
export class JoinScreen {
  private readonly message: HTMLElement;

  /** A room chosen on this page. With `reloading`, the room page shows this setup too. */
  static start(setup: HTMLElement, room: string, creating: boolean, reloading: boolean) {
    if (reloading) {
      storeSetupView({ ...captureSetup(setup), room, joining: true, creating });
    }
    return new JoinScreen(setup, room, creating);
  }

  /** The room page Battle Setup reloaded into shows that setup again before its
   * first paint. */
  static resume(setup: HTMLElement, view: Partial<SetupView> & { room: string }): JoinScreen {
    showPlayMode(setup, "multiplayer");
    showTank(setup, String(view.kind));
    showTankTeam(setup, view.previewTeam === 1 ? 1 : 0);
    restoreChoices(setup, view);
    const screen = new JoinScreen(setup, view.room, !!view.creating);
    // The browser may style the markup as single player before this script runs; show
    // the restored setup at once rather than animating tabs, cards and buttons from it.
    for (const animation of setup.getAnimations({ subtree: true })) {
      if (animation instanceof CSSTransition) {
        animation.finish();
      }
    }
    return screen;
  }

  private constructor(
    private readonly setup: HTMLElement,
    private readonly room: string,
    private readonly creating: boolean,
  ) {
    setup.dataset.joining = "";
    setup.querySelectorAll<HTMLElement>(INERT_WHILE_JOINING).forEach((part) => {
      part.inert = true;
    });
    // One lit button, as on the page that chose the room: the one that was pressed.
    const pressed = setup.querySelector<HTMLButtonElement>(
      creating ? "#create-room" : "#join-room",
    )!;
    const other = setup.querySelector<HTMLButtonElement>(creating ? "#join-room" : "#create-room")!;
    pressed.disabled = false;
    pressed.textContent = creating ? "CREATING…" : "JOINING…";
    pressed.setAttribute("aria-busy", "true");
    other.disabled = true;
    setup.querySelector("#room-list")!.replaceChildren();
    this.message = setup.querySelector("#rooms-message")!;
    this.status("Connecting to room…");
  }

  status(text: string): void {
    this.message.textContent = `${this.creating ? "Creating" : "Joining"} room ${this.room}\n${text}`;
  }

  /** The room page is showing; Battle Setup is no longer needed. */
  done(): void {
    this.setup.closest("#startup-overlay")?.remove();
  }

  /** The join gave up: reopen Battle Setup as it was, explaining why. */
  fail(notice: string): void {
    returnToSetup({ ...captureSetup(this.setup), room: this.room, joining: false, notice });
  }
}
