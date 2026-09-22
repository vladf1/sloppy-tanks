/** Act on individual fingers, including non-primary touches while both sticks are held. */
export function bindPress(button: HTMLElement, action: () => void): void {
  button.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || button.matches(":disabled")) {
      return;
    }
    event.preventDefault();
    action();
  });
  button.addEventListener("click", (event) => {
    // Pointer presses were already handled; keyboard/assistive clicks have no click count.
    if (event.detail === 0 && !button.matches(":disabled")) {
      action();
    }
  });
}
