import { bindGameOptions, syncGameOptions, type GameOptions } from "./game-options";
import { startupErrorMessage } from "./startup-error";

export interface PreparedGame {
  /** Rebuild the hidden arena for these choices ahead of GO; `onShaders` reports
   * compilation only when the choices need shaders that are not ready yet. */
  prepare(options: GameOptions, onShaders: (stage: string) => void): Promise<void>;
  start(options: GameOptions): Promise<void>;
}

export class StartMenu {
  readonly overlay: HTMLDivElement;
  private readonly button: HTMLButtonElement;
  private readonly status: HTMLElement;
  private preparation?: Promise<PreparedGame>;
  private game?: PreparedGame;
  private starting = false;
  private failed = false;

  constructor(
    root: HTMLElement,
    readonly options: GameOptions,
    private readonly load: (onStage: (stage: string) => void) => Promise<PreparedGame>,
  ) {
    this.overlay = root.querySelector<HTMLDivElement>("#startup-overlay")!;
    this.button = this.overlay.querySelector<HTMLButtonElement>("#start")!;
    this.status = this.overlay.querySelector("#startup-status")!;
    syncGameOptions(this.overlay, options);
    bindGameOptions(this.overlay, options);
    this.button.addEventListener("click", () => {
      void this.start();
    });
    // bindGameOptions updates the choices first; its listeners are on the controls.
    this.overlay.addEventListener("change", () => void this.preload());
    this.overlay.addEventListener("click", (event) => {
      if (event.target instanceof Element && event.target.closest("[data-kind]")) {
        void this.preload();
      }
    });
  }

  prepare(): Promise<PreparedGame> {
    this.preparation ??= this.load((stage) => {
      this.status.textContent = stage;
    })
      .then((game) => {
        this.game = game;
        if (this.starting) {
          this.overlay.dataset.state = "starting";
          this.status.textContent = "Starting your round…";
          this.hint("Your round will start automatically.");
        } else {
          this.showReady();
          // Catch up with choices made while the first arena was loading.
          void this.preload();
        }
        return game;
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
      const game = await this.prepare();
      await game.start(this.options);
      this.overlay.remove();
    } catch (error) {
      if (!this.failed) {
        this.showFailure(error);
      }
      this.starting = false;
    }
  }

  /** Prepare the chosen arena while the player is still choosing, so GO is instant. */
  private async preload(): Promise<void> {
    if (!this.game || this.starting || this.failed) {
      return;
    }
    try {
      await this.game.prepare(this.options, (stage) => {
        if (!this.starting) {
          delete this.overlay.dataset.state;
          this.status.textContent = stage;
        }
      });
    } catch (error) {
      this.showFailure(error);
      return;
    }
    if (!this.starting && !this.failed && this.overlay.dataset.state !== "ready") {
      this.showReady();
    }
  }

  private showReady(): void {
    this.overlay.dataset.state = "ready";
    this.status.textContent = "Ready to play";
    this.hint("All set. Hit GO when you’re ready.");
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
