// The texture-bake worker for the WebGL engine (`texture-bake-service.ts`).
import init, { bake_texture } from "../generated/engine-webgl/engine-webgl.js";
import { serveTextureBakes } from "./texture-bake-service";

serveTextureBakes(init, bake_texture);
