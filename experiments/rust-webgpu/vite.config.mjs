import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const output = new URL("../../artifacts/performance/rust-webgpu/build/", import.meta.url);
export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  base: "./",
  publicDir: false,
  resolve: { alias: { "@rust-lab": fileURLToPath(new URL("pkg", output)) } },
  server: { host: "127.0.0.1", port: 5188, strictPort: true },
  build: { outDir: fileURLToPath(new URL("dist", output)), emptyOutDir: true, target: "esnext" },
});
