import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";

export type StartGame = (options: GameOptions) => void;

export class StartMenu {
  readonly overlay: HTMLDivElement;
  private readonly button: HTMLButtonElement;
  private readonly status: HTMLElement;
  private preparation?: Promise<StartGame>;
  private starting = false;
  private failed = false;

  constructor(
    root: HTMLElement,
    readonly options: GameOptions,
    private readonly load: () => Promise<StartGame>,
  ) {
    this.overlay = root.querySelector<HTMLDivElement>("#startup-overlay")!;
    this.button = this.overlay.querySelector<HTMLButtonElement>("#start")!;
    this.status = this.overlay.querySelector("#startup-status")!;
    syncGameOptions(this.overlay, options);
    bindGameOptions(this.overlay, options);
    this.button.addEventListener("click", () => {
      void this.start();
    });
  }

  prepare(): Promise<StartGame> {
    this.preparation ??= this.load()
      .then((start) => {
        this.overlay.dataset.state = this.starting ? "starting" : "ready";
        this.status.textContent = this.starting ? "Starting your round…" : "Ready to play";
        return start;
      })
      .catch((error: unknown) => {
        this.showFailure(error);
        throw error;
      });
    return this.preparation;
  }

  private async start(): Promise<void> {
    if (this.failed) {
      location.reload();
      return;
    }
    if (this.starting) {
      return;
    }
    this.starting = true;
    this.button.disabled = true;
    this.button.textContent = "…";
    this.overlay.dataset.state = "starting";
    this.status.textContent = "Your round will start as soon as it is ready.";
    try {
      const start = await this.prepare();
      start(this.options);
      this.overlay.remove();
    } catch (error) {
      if (!this.failed) {
        this.showFailure(error);
      }
      this.starting = false;
    }
  }

  private showFailure(error: unknown): void {
    console.error("Game startup failed", error);
    this.failed = true;
    this.overlay.dataset.state = "error";
    this.status.textContent = "The arena could not load. Please try again.";
    this.button.disabled = false;
    this.button.textContent = "TRY AGAIN";
    this.button.setAttribute("aria-label", "Try loading the arena again");
  }
}
