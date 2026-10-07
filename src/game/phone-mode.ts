/** Phones get a limited edition of the game: single player on Easy, multiplayer as one
 * action that joins the busiest open room or creates one, and only the drive stick over
 * the arena, which a touch aims and fires at. The head script in `index.html`
 * adds the `phone` class before Battle Setup paints (a coarse pointer on a small
 * screen, or `?phone` to try it on any device), so the page's styles and modules
 * agree on one answer. */
export function isPhone(): boolean {
  return document.documentElement.classList.contains("phone");
}
