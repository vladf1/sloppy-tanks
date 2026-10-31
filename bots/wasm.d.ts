/** Wrangler bundles an imported `.wasm` file as a compiled module (`CompiledWasm`). */
declare module "*.wasm" {
  const module: WebAssembly.Module;
  export default module;
}
