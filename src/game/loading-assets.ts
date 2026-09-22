import { DefaultLoadingManager, type LoadingManager } from "three/webgpu";

/** Track the loaders already used by scenery, without fetching images twice.
 * Failed textures also call onLoad after itemEnd, so a fallback cannot deadlock. */
export function trackAssetLoading(manager: LoadingManager): () => Promise<void> {
  let pending = Promise.resolve();
  let finish = () => {};
  const onStart = manager.onStart;
  const onLoad = manager.onLoad;
  manager.onStart = (...args) => {
    pending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    onStart?.(...args);
  };
  manager.onLoad = () => {
    finish();
    onLoad?.();
  };
  return async () => {
    let current: Promise<void>;
    do {
      current = pending;
      await current;
    } while (current !== pending);
  };
}

export const waitForAssets = trackAssetLoading(DefaultLoadingManager);
