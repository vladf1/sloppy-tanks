// Types for the generated engine glue when `pnpm run wasm` has not run yet, so
// type checking does not depend on a Rust toolchain. Once built, TypeScript
// resolves the real `src/generated/engine/engine.d.ts` instead.
declare module "*/generated/engine/engine.js" {
  export class RenderLab {
    static create(canvas: HTMLCanvasElement, assetBase: string): Promise<RenderLab>;
    load_scene(json: string): void;
    set_generated_texture(name: string, width: number, height: number, rgba: Uint8Array): void;
    textures_pending(): number;
    texture_failures(): string[];
    prepare_step(budget: number): Uint32Array;
    warm_up(): void;
    resize(width: number, height: number): void;
    set_camera(position: Float32Array, target: Float32Array): void;
    set_opacity(name: string, opacity: number): void;
    pose_joint(name: string, copy: number, joint: string, yaw: number): boolean;
    set_visible(name: string, visible: boolean): void;
    frame(time: number): void;
    stats(): string;
    pick(x: number, y: number, height: number): Float32Array;
    error(): string | undefined;
    free(): void;
  }
  export default function init(options: { module_or_path: string | URL }): Promise<unknown>;
}
