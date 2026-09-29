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
  export class EffectsLab {
    static create(canvas: HTMLCanvasElement, assetBase: string): Promise<EffectsLab>;
    set_seed(seed: number): void;
    set_state(json: string): void;
    event(json: string, playerHit: boolean): void;
    reset(): void;
    frame(alpha: number, dt: number, time: number): void;
    prepare_step(budget: number): Uint32Array;
    warm_up(): void;
    textures_pending(): number;
    set_camera(position: Float32Array, target: Float32Array): void;
    resize(width: number, height: number): void;
    stats(): string;
    error(): string | undefined;
    free(): void;
  }
  export class Game {
    static create(canvas: HTMLCanvasElement, configJson: string): Promise<Game>;
    set_options(optionsJson: string): boolean;
    prepare_step(budget: number): Float64Array;
    start(): void;
    resume(): void;
    pause(): void;
    restart(): void;
    end_battle(): void;
    frame(now: number, input: Float32Array): Float32Array;
    drain_events(): string;
    hud_json(): string;
    stats_json(): string;
    resize(cssWidth: number, cssHeight: number, pixelRatio: number, exact: boolean): void;
    toggle_first_person(): boolean;
    set_speed(key: string, value: number): number;
    error(): string | undefined;
    debug_json(): string;
    debug_snapshot(): string;
    debug_set_autoplay(value: boolean): boolean;
    debug_set_overview(value: boolean): void;
    debug_set_auto_rounds(value: boolean): boolean;
    debug_set_zoom(zoom: number): number;
    debug_collapse(): void;
    debug_stress(): void;
    debug_soak(seconds: number): string;
    free(): void;
  }
  export default function init(options: { module_or_path: string | URL }): Promise<unknown>;
}
