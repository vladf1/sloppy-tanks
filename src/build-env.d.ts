interface ImportMetaEnv {
  readonly BASE_URL: string;
  readonly DEV: boolean;
  readonly WEBGL_BAKE_WORKER_URL: string;
  readonly WEBGPU_BAKE_WORKER_URL: string;
  readonly MULTIPLAYER_URL: string;
}
interface ImportMeta {
  readonly env: ImportMetaEnv;
}
declare module "*.wasm?url" {
  const url: string;
  export default url;
}
declare module "*.css" {}
