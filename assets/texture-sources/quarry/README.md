# Sandstone source

`sandstone.webp` preserves the runtime texture before the September 14 size optimization. This is already lossy artwork; no higher-quality source was present in the repository. Keep this fixed input to avoid repeated lossy recompression of successive runtime outputs.

`npm run optimize:textures` encodes it to `public/textures/quarry/sandstone.webp` at 768×768, quality 75. The source is outside `public` and is not deployed.

## Provenance

Preserved source: `assets/texture-sources/quarry/sandstone.webp` (1024 × 1024, 303,904 bytes). Generated with the built-in image generation tool, then resized and encoded as WebP at quality 84. Used for color and subtle bump detail, with mirrored wrapping and mipmaps.

Original generated output was recorded at the following machine-local path (not included in the repository): `/Users/vlad/.codex/generated_images/01a097cb-7e17-7110-b79e-ad705cbb5091/exec-b910d85b-3443-499c-9e7a-f00ebe6f866a.png`.

Final generation prompt:

> Use case: photorealistic-natural. Asset type: square tileable game material albedo texture, 1024x1024. Create a seamless orthographic close photograph of a weathered desert sandstone quarry face, covering the entire image edge to edge. Natural pale ochre and muted beige stone, subtle warm gray mineral inclusions, fine horizontal sediment layers with slight irregular undulation, fine sandy grain and occasional thin hairline fractures. Restrained low contrast so it tiles across large 3D rock formations without obvious landmarks. Flat diffuse neutral lighting, no directional shadows, no highlights or ambient occlusion baked in. Physically plausible real rock, neither cartoon nor stylized. No perspective, no objects, no border, no labels, no writing, no dramatic deep cracks. Seamless matching edges. This will be mapped to low polygon quarry rocks, the fine texture should carry the realism.
