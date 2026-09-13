# Dusty Dig

Third authored map, selectable from the menu or `?map=quarry`, also included in Surprise me. Supports Team Battle and Solo Assault.

The broad central crossing stays open. Staggered sandstone outcrops protect approaches, and two crate-blocked outer cuts open when their crates are destroyed. Six concrete dragon's teeth and four steel hedgehogs provide permanent anti-tank cover without sealing routes. Their colliders follow tapered concrete and open I-beams, allowing shells through visible gaps. Sandstone collision follows the same vertices as its visible mesh.

Models include stratified sandstone outcrops, an excavator with tracks, cab glazing, hydraulic rams and a toothed bucket, a loaded haul truck, a site office and terraced quarry walls. Large scenery stays outside the playable arena. All quarry scenery is static, batched and retained across rounds; rock geometry and surface materials are shared. Each rock uses 70 triangles. Existing dirt, concrete, wood and worn-steel textures are reused. No new dependencies or per-frame scenery effects.

## Validation

`npm run check`: lint, formatting, production build and all 179 tests passed. Four new simulation tests cover pickup/spawn clearance, navigation and shortcuts, accurate barrier collision, both battle modes, reset restoration and Surprise me selection.

Built-in-browser checks at a 1280 × 720 viewport: models and crate destruction inspected; 15 map switches preserved the correct scenery and cover models, with retained GPU resources stable at 473 geometries / 38 textures after warm-up.

A seeded 240-frame comparison per map, discarding 60 warm-up frames, measured one simulation step and an overview render per frame. Render counts include shadow passes. These are local CPU submission measurements, not GPU timing or an FPS guarantee.

| Map | CPU median | CPU p95 | Mean draw calls | Mean triangles |
| --- | ---: | ---: | ---: | ---: |
| Pine Village | 4.00 ms | 5.10 ms | 1,197 | 385,564 |
| Harbor Havoc | 2.80 ms | 3.50 ms | 796 | 1,215,829 |
| Dusty Dig | 2.80 ms | 3.30 ms | 687 | 157,446 |

Dusty Dig used 14% fewer draw calls than Harbor Havoc in this comparison. The interactive inspection and comparison fixture is `tests/quarry.browser.html`.

## Sandstone asset provenance

Saved project asset: `public/textures/quarry/sandstone.webp` (1024 × 1024, 303,904 bytes). Generated with the built-in image generation tool, then resized and encoded as WebP at quality 84. Used for color and subtle bump detail, with mirrored wrapping and mipmaps.

Original generated source retained at `/Users/vlad/.codex/generated_images/01a097cb-7e17-7110-b79e-ad705cbb5091/exec-b910d85b-3443-499c-9e7a-f00ebe6f866a.png`.

Final generation prompt:

> Use case: photorealistic-natural. Asset type: square tileable game material albedo texture, 1024x1024. Create a seamless orthographic close photograph of a weathered desert sandstone quarry face, covering the entire image edge to edge. Natural pale ochre and muted beige stone, subtle warm gray mineral inclusions, fine horizontal sediment layers with slight irregular undulation, fine sandy grain and occasional thin hairline fractures. Restrained low contrast so it tiles across large 3D rock formations without obvious landmarks. Flat diffuse neutral lighting, no directional shadows, no highlights or ambient occlusion baked in. Physically plausible real rock, neither cartoon nor stylized. No perspective, no objects, no border, no labels, no writing, no dramatic deep cracks. Seamless matching edges. This will be mapped to low polygon quarry rocks, the fine texture should carry the realism.
