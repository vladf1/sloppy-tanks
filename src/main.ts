// Keep this entry small so the HTML loading screen can paint during engine loading.
void import("./game").catch((error: unknown) => {
  console.error("Game startup failed", error);
  const loading = document.querySelector<HTMLElement>("#loading");
  if (loading) {
    loading.innerHTML =
      '<h1>SLOPPY TANKS</h1><p>The arena could not load.</p><button type="button">Try again</button>';
    loading.querySelector("button")!.addEventListener("click", () => location.reload());
  }
});
