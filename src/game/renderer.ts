import type {
  BufferAttribute,
  Camera,
  InterleavedBufferAttribute,
  Light,
  Material,
  Object3D,
  Scene,
} from "three/webgpu";
import {
  BackSide,
  BoxGeometry,
  DoubleSide,
  FrontSide,
  Mesh,
  MeshLambertNodeMaterial,
  Renderer,
  StandardNodeLibrary,
  WebGPUBackend,
  WGSLNodeBuilder,
  type WebGPURendererParameters,
} from "three/webgpu";
import { lights } from "three/tsl";
import { RenderResources, trackInterleavedBuffers, type DrawResources } from "./renderer-resources";
import {
  compileRuntimePipelinesAsync,
  preserveRenderBundleScope,
  submitRenderBundlesInOrder,
  trackRenderBundles,
  type BundleBackend,
  type BundleExecutionBackend,
  type BundleRenderer,
  type RuntimePipelines,
} from "./bundle-stats";
import { WebGPUUnavailableError } from "./startup-error";
import { withTaskYield } from "./task-yield";

interface RendererCaches {
  _attributes: { delete(attribute: BufferAttribute | InterleavedBufferAttribute): unknown };
  _geometries: { updateForRender(draw: DrawResources): void };
}

interface UniformBuilder {
  globalCache: unknown;
  stableBufferCount?: number;
  getDataFromNode(node: unknown, shaderStage: unknown, cache: unknown): { uniformGPU?: unknown };
  getUniformFromNode: (
    this: UniformBuilder,
    node: unknown,
    type: string,
    shaderStage: unknown,
    name?: string | null,
  ) => { name: string };
}
const BUFFER_UNIFORMS = new Set(["buffer", "storageBuffer", "indirectStorageBuffer"]);

/** r185 names each unnamed WGSL buffer uniform after a global node id, so every
 * build of an instanced or storage-buffer mesh emits unique shader code. The main
 * view, shadow and reflection passes then compile separate copies of one shader,
 * and meshes with identical shaders never share a program or pipeline; Safari
 * compiles each copy from scratch on a first visit. Number them in declaration
 * order instead, as Three already does for every other uniform. */
function nameBufferUniformsByOrder(): void {
  const builder = WGSLNodeBuilder.prototype as unknown as UniformBuilder;
  const getUniform = builder.getUniformFromNode;
  builder.getUniformFromNode = function (node, type, shaderStage, name) {
    const unnamedBuffer =
      !name &&
      BUFFER_UNIFORMS.has(type) &&
      this.getDataFromNode(node, shaderStage, this.globalCache).uniformGPU === undefined;
    const uniform = getUniform.call(this, node, type, shaderStage, name);
    if (unnamedBuffer) {
      this.stableBufferCount = (this.stableBufferCount ?? 0) + 1;
      uniform.name = `NodeBuffer_${this.stableBufferCount}`;
    }
    return uniform;
  };
}
nameBufferUniformsByOrder();

/** Rename `nodeUniformN` identifiers in the order the code first mentions them. */
export function numberUniformsInOrder(code: string): string {
  const numbers = new Map<string, number>();
  return code.replace(/\bnodeUniform(\d+)/g, (_, index: string) => {
    let number = numbers.get(index);
    if (number === undefined) {
      number = numbers.size;
      numbers.set(index, number);
    }
    return `nodeUniform${number}`;
  });
}

interface ThrowawayBuilder {
  context: { material?: Material };
  build(): void;
}

interface StageCode {
  vertexShader: string | null;
  fragmentShader: string | null;
  buildCode: (this: StageCode) => void;
}

/** r185 numbers uniforms across both stages of a build, so a vertex-only uniform
 * (a part batch's matrices, an instance buffer) renames every fragment uniform.
 * Lit fragment shaders that are otherwise identical then compile as separate
 * programs, and Safari compiles each one from scratch (about 0.5 s) on a first
 * visit. Number each stage's uniforms in its own code order instead; WebGPU
 * binds them by group and binding index, never by name. */
function numberUniformsPerStage(): void {
  const builder = WGSLNodeBuilder.prototype as unknown as StageCode;
  const buildCode = builder.buildCode;
  builder.buildCode = function () {
    buildCode.call(this);
    if (this.vertexShader !== null && this.fragmentShader !== null) {
      this.vertexShader = numberUniformsInOrder(this.vertexShader);
      this.fragmentShader = numberUniformsInOrder(this.fragmentShader);
    }
  };
}
numberUniformsPerStage();

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
  private pendingPipelinesReady: () => Promise<void> = async () => {};

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
    this.pendingPipelinesReady = compileRuntimePipelinesAsync(
      (this as unknown as { _pipelines: RuntimePipelines })._pipelines,
    );
    const update = caches._geometries.updateForRender.bind(caches._geometries);
    caches._geometries.updateForRender = (draw) => {
      update(draw);
      resources.track(draw);
    };
    return this;
  }

  async waitForPipelineCompilation(): Promise<void> {
    await this.pendingPipelinesReady();
  }

  override async compileAsync(...args: Parameters<Renderer["compileAsync"]>): Promise<void> {
    const [scene, camera, targetScene] = args;
    this.primeLitBuild(targetScene ?? scene, camera);
    // Node building stays sequential (Three shares builder state), but GPU
    // compilation overlaps subsequent builds instead of awaiting each pipeline.
    // Its yields between stages must not wait a frame each; see task-yield.ts.
    // r185 queues every draw before its first await; see renderObject().
    this.queueingCompile = true;
    let compiled: Promise<void>;
    try {
      compiled = withTaskYield(() => super.compileAsync(...args));
    } finally {
      this.queueingCompile = false;
    }
    await compiled;
    // Compile each two-pass object once per side, as the passes of a frame draw
    // it; the frame then finds both shaders built instead of building them itself.
    const twoPass = [...this.twoPassObjects];
    this.twoPassObjects.clear();
    const frameScene = targetScene ?? ("isScene" in scene ? (scene as Scene) : null);
    for (const side of [BackSide, FrontSide]) {
      for (const [object, material] of twoPass) {
        material.side = side;
        try {
          await withTaskYield(() => super.compileAsync(object, camera, frameScene));
        } finally {
          material.side = DoubleSide;
        }
      }
    }
    await this.waitForPipelineCompilation();
  }

  private queueingCompile = false;
  private twoPassObjects = new Map<Object3D, Material>();
  private primedFog?: unknown;
  /** r185 creates a shadowed light's filter uniforms during every lit build, but
   * the scene fog's uniforms and the light's shadow graph only in the first build
   * that needs them. That first build therefore orders its code differently, and
   * its program never matches the same material built again, for example in the
   * water reflection; Safari compiles one more lit shader on a first visit. Create
   * both in a throwaway node build first. It never reaches the GPU. */
  private primeLitBuild(scene: Object3D, camera: Camera): void {
    const nodes = (this as unknown as { _nodes: { getFogNode(scene: Object3D): unknown } })._nodes;
    const fogNode = nodes.getFogNode(scene);
    if (this.primedFog !== undefined && this.primedFog === fogNode) {
      return;
    }
    this.primedFog = fogNode;
    const sceneLights: Light[] = [];
    scene.traverseVisible((object) => {
      if ("isLight" in object) {
        sceneLights.push(object as Light);
      }
    });
    // The cheapest lit material that receives shadows; the graphs are shared.
    const material = new MeshLambertNodeMaterial();
    const mesh = new Mesh(new BoxGeometry(), material);
    mesh.receiveShadow = true;
    const backend = this.backend as unknown as {
      createNodeBuilder(object: Object3D, renderer: Renderer): ThrowawayBuilder;
    };
    const builder = backend.createNodeBuilder(mesh, this);
    Object.assign(builder, { scene, camera, material, fogNode, lightsNode: lights(sceneLights) });
    builder.context.material = material;
    builder.build();
    mesh.geometry.dispose();
    material.dispose();
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
    // r185 draws a double-sided transparent material as a back pass and a front
    // pass, but compileAsync() builds the back pass after restoring the double
    // side, deriving a third shader that no later draw shares. Set these aside;
    // compileAsync() then compiles them with each pass's side.
    const material = args[4];
    if (
      this.queueingCompile &&
      material.transparent &&
      material.side === DoubleSide &&
      !material.forceSinglePass
    ) {
      this.twoPassObjects.set(object, material);
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
