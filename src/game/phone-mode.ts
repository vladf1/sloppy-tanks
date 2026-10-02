/** Phones get a limited edition of the game: single player on Easy, with only the
 * thumb sticks and a fire button over the arena. The head script in `index.html`
 * adds the `phone` class before Battle Setup paints (a coarse pointer on a small
 * screen, or `?phone` to try it on any device), so the page's styles and modules
 * agree on one answer. */
export function isPhone(): boolean {
  return document.documentElement.classList.contains("phone");
}
