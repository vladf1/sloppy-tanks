// Build values the page reads from `process.env`, which never reaches the browser itself:
// production builds define them (scripts/site-build.ts), and the dev server's plugin
// writes them into the modules (scripts/dev-pages-plugin.ts).
declare module "bun" {
  interface Env {
    /** The page's base path, ending in `/`. */
    readonly PUBLIC_BASE_URL: string;
    /** `"true"` in development builds, which offer the debug API. */
    readonly PUBLIC_DEV: "true" | "false";
    /** The site's game server, or empty where only local pages use the local one. */
    readonly PUBLIC_MULTIPLAYER_URL: string;
    readonly PUBLIC_WEBGPU_BAKE_WORKER_URL: string;
    readonly PUBLIC_WEBGL_BAKE_WORKER_URL: string;
  }
}
declare module "*.wasm?url" {
  const url: string;
  export default url;
}
declare module "*.css" {}
