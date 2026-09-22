import assert from "node:assert/strict";
import { mock, test } from "node:test";
import {
  preserveRenderBundleScope,
  submitRenderBundlesInOrder,
  trackRenderBundles,
  type BundleBackend,
  type BundleExecutionBackend,
  type BundleRenderer,
} from "../src/game/bundle-stats";

test("nested shadow bundles retain the parent's remaining camera-update records", () => {
  const main = {},
    shadow = {};
  const records: (object | null)[] = [];
  const renderer: BundleRenderer = {
    _currentRenderBundle: null,
    _renderScene(scene, camera) {
      assert.equal(this._currentRenderBundle, null, "nested ordinary draws have no parent bundle");
      this._currentRenderBundle = camera;
      if (camera === main) {
        records.push(this._currentRenderBundle);
        this._renderScene(scene, shadow);
        records.push(this._currentRenderBundle);
      }
      this._currentRenderBundle = null;
      return camera;
    },
  };
  preserveRenderBundleScope(renderer);
  assert.equal(renderer._renderScene({}, main), main);
  assert.deepEqual(records, [main, main]);
  assert.equal(renderer._currentRenderBundle, null);
});

test("opaque bundles execute before subsequent transparent draws and reset pass bindings", () => {
  const commands: object[] = [];
  const model = {},
    hud = {};
  const state: ReturnType<BundleExecutionBackend["get"]> = {
    currentPass: { executeBundles: (bundles) => commands.push(...bundles) },
    renderBundles: [],
    currentSets: { attributes: { position: {} }, bindingGroups: [{}], pipeline: {}, index: {} },
  };
  const backend: BundleExecutionBackend = {
    get: () => state,
    addBundle: (_context, bundle) => state.renderBundles.push(bundle),
  };
  submitRenderBundlesInOrder(backend);
  backend.addBundle({}, model);
  commands.push(hud);
  assert.deepEqual(commands, [model, hud]);
  assert.deepEqual(state.renderBundles, [], "finishRender must not replay the model over the HUD");
  assert.deepEqual(state.currentSets, {
    attributes: {},
    bindingGroups: [],
    pipeline: null,
    index: null,
  });
});

test("bundle executions count their recorded draws once on initial and later frames", () => {
  const info = { render: { drawCalls: 0, triangles: 0, points: 0, lines: 0 } };
  const backend: BundleBackend = {
    beginBundle: mock.fn(),
    finishBundle: mock.fn(),
    addBundle: mock.fn(),
    draw: (_draw, stats) => {
      stats.render.drawCalls++;
      stats.render.triangles += 12;
    },
  };
  trackRenderBundles(backend, info);
  const context = {},
    bundle = {};
  backend.beginBundle(context);
  backend.draw({ context }, info);
  backend.draw({ context }, info);
  backend.finishBundle(context, bundle);
  backend.addBundle(context, bundle);
  assert.equal(info.render.drawCalls, 2);
  assert.equal(info.render.triangles, 24);
  info.render.drawCalls = info.render.triangles = 0;
  backend.addBundle(context, bundle);
  assert.equal(info.render.drawCalls, 2);
  assert.equal(info.render.triangles, 24);
});

test("nested shadow recording and independent draws stay out of the parent bundle", () => {
  const info = { render: { drawCalls: 0, triangles: 0, points: 0, lines: 0 } };
  const backend: BundleBackend = {
    beginBundle: mock.fn(),
    finishBundle: mock.fn(),
    addBundle: mock.fn(),
    draw: (_draw, stats) => {
      stats.render.drawCalls++;
      stats.render.triangles += 10;
    },
  };
  trackRenderBundles(backend, info);
  const main = {},
    shadow = {},
    a = {},
    b = {};
  backend.beginBundle(main);
  backend.draw({ context: main }, info);
  backend.draw({ context: shadow }, info);
  backend.beginBundle(shadow);
  backend.draw({ context: shadow }, info);
  backend.draw({ context: shadow }, info);
  backend.finishBundle(shadow, b);
  backend.addBundle(shadow, b);
  backend.finishBundle(main, a);
  backend.addBundle(main, a);
  assert.equal(info.render.drawCalls, 4);
  info.render.drawCalls = info.render.triangles = 0;
  backend.addBundle(main, a);
  assert.equal(info.render.drawCalls, 1);
  backend.addBundle(shadow, b);
  assert.equal(info.render.drawCalls, 3);
  assert.equal(info.render.triangles, 30);
});
