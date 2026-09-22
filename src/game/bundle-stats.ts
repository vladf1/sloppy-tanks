import type { Renderer } from "three/webgpu";

type Counts = Pick<Renderer["info"]["render"], "drawCalls" | "triangles" | "points" | "lines">;
type Stats = { render: Counts };
export interface BundleRenderer {
  _currentRenderBundle: object | null;
  _renderScene(scene: object, camera: object, useFrameBufferTarget?: boolean): unknown;
}

/** Isolate nested shadow/reflection scenes from the parent's bundle recording.
 * r185 otherwise loses the parent's remaining camera-update records, or records
 * unrelated nested draws in it. Restore the recording when the scene returns. */
export function preserveRenderBundleScope(renderer: BundleRenderer): void {
  const render = renderer._renderScene.bind(renderer);
  renderer._renderScene = (...args) => {
    const previous = renderer._currentRenderBundle;
    renderer._currentRenderBundle = null;
    try {
      return render(...args);
    } finally {
      renderer._currentRenderBundle = previous;
    }
  };
}

export interface BundleExecutionBackend {
  addBundle(context: object, bundle: object): void;
  get(context: object): {
    currentPass: { executeBundles(bundles: object[]): void } | null;
    renderBundles: object[];
    currentSets: {
      attributes: object;
      bindingGroups: unknown[];
      pipeline: unknown;
      index: unknown;
    };
  };
}

/** r185 queues bundles until finishRender, after transparent effects and HUD.
 * Execute at their requested position so opaque models cannot cover those draws.
 * executeBundles clears GPU pass bindings; invalidate Three's matching cache. */
export function submitRenderBundlesInOrder(backend: BundleExecutionBackend): void {
  const add = backend.addBundle.bind(backend);
  backend.addBundle = (context, bundle) => {
    add(context, bundle);
    const state = backend.get(context);
    if (state.currentPass) {
      state.currentPass.executeBundles(state.renderBundles);
      state.renderBundles.length = 0;
      state.currentSets = { attributes: {}, bindingGroups: [], pipeline: null, index: null };
    }
  };
}

export interface BundleBackend {
  beginBundle(context: object): void;
  finishBundle(context: object, bundle: object): void;
  addBundle(context: object, bundle: object): void;
  draw(draw: { context: object }, info: Stats): void;
}

/** r185 counts draws while recording bundles but omits their later executions.
 * Retain per-bundle counts so diagnostics measure submitted work on every frame. */
export function trackRenderBundles(backend: BundleBackend, info: Stats): void {
  const recording = new WeakMap<object, Counts>();
  const counts = new WeakMap<object, Counts>();
  const justRecorded = new WeakSet<object>();
  const begin = backend.beginBundle.bind(backend);
  const finish = backend.finishBundle.bind(backend);
  const add = backend.addBundle.bind(backend);
  const draw = backend.draw.bind(backend);
  backend.beginBundle = (context) => {
    begin(context);
    recording.set(context, { drawCalls: 0, triangles: 0, points: 0, lines: 0 });
  };
  backend.draw = (object, stats) => {
    const count = recording.get(object.context);
    if (!count) {
      draw(object, stats);
      return;
    }
    const { drawCalls, triangles, points, lines } = stats.render;
    draw(object, stats);
    count.drawCalls += stats.render.drawCalls - drawCalls;
    count.triangles += stats.render.triangles - triangles;
    count.points += stats.render.points - points;
    count.lines += stats.render.lines - lines;
  };
  backend.finishBundle = (context, bundle) => {
    finish(context, bundle);
    const count = recording.get(context);
    if (count) {
      counts.set(bundle, count);
      justRecorded.add(bundle);
      recording.delete(context);
    }
  };
  backend.addBundle = (context, bundle) => {
    add(context, bundle);
    if (justRecorded.delete(bundle)) {
      return;
    }
    const count = counts.get(bundle);
    if (!count) {
      return;
    }
    info.render.drawCalls += count.drawCalls;
    info.render.triangles += count.triangles;
    info.render.points += count.points;
    info.render.lines += count.lines;
  };
}
