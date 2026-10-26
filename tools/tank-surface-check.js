// scripts/generate-previews.mjs packs `window.tankPreviewAssets` (or reports
// `window.tankPreviewError`).
import { PREVIEW_KINDS, renderTankPreviews } from "./tank-previews";
let previews;
try {
  previews = window.tankPreviewAssets = await renderTankPreviews();
} catch (error) {
  window.tankPreviewError = String(error?.stack ?? error);
  document.querySelector("main").textContent = String(error?.message ?? error);
  throw error;
}
for (const team of [0, 1])
  for (const kind of PREVIEW_KINDS) {
    const card = document.createElement("article");
    const title = document.createElement("h2");
    title.textContent = `${team ? "Red" : "Blue"} · ${kind}`;
    card.append(title);
    for (const size of ["large", "small"]) {
      const img = new Image();
      img.className = size;
      img.src = previews[`${team}-${kind}`];
      card.append(img);
    }
    document.querySelector("main").append(card);
  }
