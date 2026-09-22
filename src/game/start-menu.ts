import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";
import { startupErrorMessage } from "./startup-error";

export type StartGame = (options: GameOptions) => void | Promise<void>;

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
    private readonly load: (onStage: (stage: string) => void) => Promise<StartGame>,
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
    this.preparation ??= this.load((stage) => {
      this.status.textContent = stage;
    })
      .then((start) => {
        this.overlay.dataset.state = this.starting ? "starting" : "ready";
        this.status.textContent = this.starting ? "Starting your round…" : "Ready to play";
        this.hint(
          this.starting
            ? "Your round will start automatically."
            : "All set. Hit GO when you’re ready.",
        );
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
    this.button.textContent = "WAIT";
    this.button.setAttribute("aria-busy", "true");
    this.overlay.dataset.state = "starting";
    this.status.textContent = "Preparing your round…";
    this.hint("GO received. Your round will start automatically.");
    try {
      const start = await this.prepare();
      await start(this.options);
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
    this.status.textContent = startupErrorMessage(
      error,
      "The arena could not load. Please try again.",
    );
    this.button.disabled = false;
    this.button.removeAttribute("aria-busy");
    this.button.textContent = "TRY AGAIN";
    this.button.setAttribute("aria-label", "Try loading the arena again");
    this.hint("Check your connection, then try again.");
  }

  private hint(message: string): void {
    const hint = this.overlay.querySelector(".startup-hint");
    if (hint) {
      hint.textContent = message;
    }
  }
}
