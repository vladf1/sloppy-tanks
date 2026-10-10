// Types for the labs glue when `pnpm run wasm:labs` has not run: `pnpm run build` and
// CI's type check build only the game engines. Once built, TypeScript resolves the real
// `src/generated/engine-labs/engine.d.ts` instead. Only the calls TypeScript makes are
// declared.
declare module "*/generated/engine-labs/engine.js" {
  export class RenderLab {
    static create(canvas: HTMLCanvasElement, assetBase: string): Promise<RenderLab>;
    load_scene(json: string): void;
    add_vehicle(
      name: string,
      kind: string,
      team: number,
      x: number,
      y: number,
      z: number,
      yaw: number,
    ): void;
    set_background(color: number): void;
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
  export class EffectsLab {
    static create(canvas: HTMLCanvasElement, assetBase: string): Promise<EffectsLab>;
    set_seed(seed: number): void;
    set_state(json: string): void;
    event(json: string): void;
    reset(): void;
    frame(alpha: number, dt: number, time: number): void;
    prepare_step(budget: number): Uint32Array;
    warm_up(): void;
    textures_pending(): number;
    set_camera(position: Float32Array, target: Float32Array): void;
    stats(): string;
    error(): string | undefined;
  }
  export default function init(options?: { module_or_path: string | URL }): Promise<unknown>;
}
