// The texture-bake worker for the WebGPU engine (`texture-bake-service.ts`).
import init, { bake_texture } from "../generated/engine/engine.js";
import { serveTextureBakes } from "./texture-bake-service";

serveTextureBakes(init, bake_texture);
