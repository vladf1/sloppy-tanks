import type { BufferAttribute, InterleavedBufferAttribute, Material, Object3D } from "three/webgpu";
import {
  Renderer,
  StandardNodeLibrary,
  WebGPUBackend,
  type WebGPURendererParameters,
} from "three/webgpu";
import { RenderResources, trackInterleavedBuffers, type DrawResources } from "./renderer-resources";
import {
  preserveRenderBundleScope,
  submitRenderBundlesInOrder,
  trackRenderBundles,
  type BundleBackend,
  type BundleExecutionBackend,
  type BundleRenderer,
} from "./bundle-stats";
import { WebGPUUnavailableError } from "./startup-error";

interface RendererCaches {
  _attributes: { delete(attribute: BufferAttribute | InterleavedBufferAttribute): unknown };
  _geometries: { updateForRender(draw: DrawResources): void };
}

/** r185 shares one shadow material across cutout foliage and solid meshes. Each
 * alpha-test toggle increments its version, invalidating every caster's cache.
 * Keep a stable variant per cutoff; all shadow nodes and draw behavior stay in Three. */
export class ShadowMaterials {
  private variants = new WeakMap<Material, Map<number, Material>>();

  get(source: Material, alphaTest: number): Material {
    let variants = this.variants.get(source);
    if (!variants) {
      variants = new Map();
      this.variants.set(source, variants);
      const owned = variants;
      const dispose = () => {
        for (const variant of owned.values()) {
          variant.dispose();
        }
        owned.clear();
        this.variants.delete(source);
        source.removeEventListener("dispose", dispose);
      };
      source.addEventListener("dispose", dispose);
    }
    let variant = variants.get(alphaTest);
    if (!variant) {
      variant = source.clone();
      Object.assign(variant, { isShadowPassMaterial: true });
      variant.alphaTest = alphaTest;
      variants.set(alphaTest, variant);
    }
    return variant;
  }
}

export class GameRenderer extends Renderer {
  override library = new StandardNodeLibrary();
  readonly isWebGPURenderer = true;
  private shadows = new ShadowMaterials();
  private resources?: RenderResources;

  constructor(parameters: Omit<WebGPURendererParameters, "forceWebGL" | "getFallback"> = {}) {
    super(new WebGPUBackend(parameters), parameters);
  }

  override async init(): Promise<this> {
    try {
      await super.init();
    } catch (error) {
      throw new WebGPUUnavailableError(error);
    }
    if (this.resources) {
      return this;
    }
    // r185 has no public per-object release API. Keep this version-specific hook
    // isolated, and verify real GPU allocations in the rendered reset check.
    const caches = this as unknown as RendererCaches;
    const resources = new RenderResources((attribute) => caches._attributes.delete(attribute));
    this.resources = resources;
    trackInterleavedBuffers(this.info);
    preserveRenderBundleScope(this as unknown as BundleRenderer);
    submitRenderBundlesInOrder(this.backend as unknown as BundleExecutionBackend);
    trackRenderBundles(this.backend as unknown as BundleBackend, this.info);
    const update = caches._geometries.updateForRender.bind(caches._geometries);
    caches._geometries.updateForRender = (draw) => {
      update(draw);
      resources.track(draw);
    };
    return this;
  }

  releaseObjects(root: Object3D): void {
    this.resources?.release(root);
  }

  releaseStorage(attribute: BufferAttribute): void {
    (this as unknown as RendererCaches)._attributes.delete(attribute);
  }

  override dispose(): void {
    if (this.initialized) {
      super.dispose();
    } else {
      // r185 Renderer.dispose() calls setAnimationLoop(null), which retries
      // initialization and rejects without a handler when startup already failed.
      (this.backend as unknown as { dispose(): void }).dispose();
    }
  }

  override renderObject(...args: Parameters<Renderer["renderObject"]>): void {
    const object = args[0];
    const geometry = args[3];
    // Effects are populated before rendering. Empty pools/refill arcs need no
    // shader work, buffer uploads or draw submission (including shadow passes).
    if (
      ("isInstancedMesh" in object &&
        object.isInstancedMesh &&
        "count" in object &&
        object.count === 0) ||
      geometry.drawRange.count === 0 ||
      geometry.index?.count === 0
    ) {
      return;
    }
    const scene = args[1];
    const source = scene.overrideMaterial;
    if (!source || !("isShadowPassMaterial" in source) || !source.isShadowPassMaterial) {
      super.renderObject(...args);
      return;
    }
    scene.overrideMaterial = this.shadows.get(source, args[4].alphaTest);
    try {
      super.renderObject(...args);
    } finally {
      scene.overrideMaterial = source;
    }
  }
}
