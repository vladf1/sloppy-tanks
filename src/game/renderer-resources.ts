import type {
  BufferAttribute,
  BufferGeometry,
  InterleavedBuffer,
  InterleavedBufferAttribute,
  Object3D,
  WebGPURenderer,
} from "three/webgpu";

type Attribute = BufferAttribute | InterleavedBufferAttribute;
/** The r185 renderer's internal draw record; keep the compatibility boundary here. */
export interface DrawResources {
  object: Object3D;
  geometry: BufferGeometry;
  getAttributes(): Attribute[];
  onDispose(): void;
  dispose(): void;
}

/** Shared materials outlive removed meshes. Release their per-object bindings
 * explicitly, while leaving borrowed geometry, textures and materials intact. */
export class RenderResources {
  private objects = new WeakMap<Object3D, Set<DrawResources>>();
  private geometries = new WeakMap<BufferGeometry, Set<Attribute>>();
  private tracked = new WeakSet<DrawResources>();

  constructor(private deleteAttribute: (attribute: Attribute) => void) {}

  track(draw: DrawResources): void {
    if (this.tracked.has(draw)) {
      return;
    }
    this.tracked.add(draw);
    let draws = this.objects.get(draw.object);
    if (!draws) {
      draws = new Set();
      this.objects.set(draw.object, draws);
    }
    draws.add(draw);
    const owned = draws;
    const dispose = draw.onDispose.bind(draw);
    draw.onDispose = () => {
      owned.delete(draw);
      dispose();
    };
    if (draw.geometry.userData.owned) {
      let attributes = this.geometries.get(draw.geometry);
      if (!attributes) {
        attributes = new Set();
        this.geometries.set(draw.geometry, attributes);
        const all = attributes;
        const cleanup = () => {
          // Three only remembers attributes used by the first pass. Include
          // normal/color attributes first encountered in subsequent passes.
          for (const attribute of all) {
            this.deleteAttribute(attribute);
          }
          this.geometries.delete(draw.geometry);
          draw.geometry.removeEventListener("dispose", cleanup);
        };
        draw.geometry.addEventListener("dispose", cleanup);
      }
      for (const attribute of draw.getAttributes()) {
        attributes.add(attribute);
      }
    }
  }

  release(root: Object3D): void {
    root.traverse((object) => {
      for (const draw of this.objects.get(object) ?? []) {
        draw.dispose();
      }
    });
  }
}

/** r185 counts fresh shader attribute views as fresh GPU buffers. Track each
 * shared interleaved backing buffer once, matching the backend's allocation. */
export function trackInterleavedBuffers(info: WebGPURenderer["info"]): void {
  const buffers = new WeakMap<InterleavedBuffer, BufferAttribute>();
  const create = info.createAttribute.bind(info);
  const destroy = info.destroyAttribute.bind(info);
  info.createAttribute = (attribute) => {
    if ("isInterleavedBufferAttribute" in attribute) {
      const view = attribute as unknown as InterleavedBufferAttribute;
      if (buffers.has(view.data)) {
        return;
      }
      buffers.set(view.data, attribute);
    }
    create(attribute);
  };
  info.destroyAttribute = (attribute) => {
    if ("isInterleavedBufferAttribute" in attribute) {
      const view = attribute as unknown as InterleavedBufferAttribute;
      const first = buffers.get(view.data);
      if (first) {
        buffers.delete(view.data);
        destroy(first);
      }
    } else {
      destroy(attribute);
    }
  };
}
