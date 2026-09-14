# Sandstone source

`sandstone.webp` preserves the runtime texture before the September 14 size optimization. This is already lossy artwork; no higher-quality source was present in the repository. Keep this fixed input to avoid repeated lossy recompression of successive runtime outputs.

`npm run optimize:textures` encodes it to `public/textures/quarry/sandstone.webp` at 768×768, quality 75. The source is outside `public` and is not deployed.
