# Dusty Dig

Third authored map, selectable from the menu or `?map=quarry`, also included in Surprise me. Supports Team Battle and Solo Assault.

## September 2026 art and layout pass

The plan was to replace the uniform orange board and repeated perimeter boulders with a coherent excavation, then improve outcrop silhouettes, storage placement and defensive lines in response to visual review.

- **Work floor:** a retained 2048 × 2048 procedural soil texture combines metre-scaled aggregate, mottling, a rounded haul loop, a central crossing and paired wheel ruts. The playable floor stays flat; only the outer machinery apron slopes down.
- **Geology:** connected quarry benches replace rows of identical boulders. Overlapping terrace elevations prevent sky gaps. Outcrops have irregular 16-point outlines, locally eroded faces, broad broken caps and selective normal smoothing. Three-axis sandstone texture blending removes abrupt projection seams. Each rock uses 208 triangles and shares its actual vertices with the physics collider.
- **Storage bays:** the two outer rock cuts are six metres wide, each containing an aligned 2 × 2 group of individually destructible supply crates. Crates are 2.4 × 2.8 × 2.1 metres with 55 HP each, replacing the oversized 5 × 5.8 metre crates. Clear margins separate storage from stone.
- **Defenses:** each approach has eight dragon’s teeth in two staggered rows on low concrete footings. Two four-hedgehog lines defend the midfield shoulders. Placements retain rotational symmetry, accessible pickups, the open central crossing and navigable outer routes.
- **Site detail:** screening conveyor, aggregate heaps, service-office access and air conditioner, water tank, quarry signage, stacked sawn blocks, scree and sparse scrub. Tall dressing stays outside the arena; loose in-arena chips are only 3–7 cm high.
- **Light and color:** neutral sandy ground, pale sandstone, cooler atmospheric haze and a lower warm sun reveal more surface relief. Other maps restore their existing light intensities on selection.

All scenery is static, batched and retained across rounds. Ground generation and material setup happen once, with no new runtime dependencies, animated scenery or per-frame geometry generation. Rock footings are cached so round resets do not accumulate their source geometries. Existing sandstone, concrete, wood and steel textures are reused; the ground and sign textures are generated locally from code.

## Validation

`npm run check`: lint, formatting, production build and all 180 tests passed. Quarry tests cover deployment/pickup clearance, connected routes, destructible storage cuts, tapered/open barrier collision, both battle modes, resets, Surprise me selection, coherent defensive groups and crate/rock separation. `npm run validate` also passed its ten seeded full matches and reset checks.

Browser inspection covered outcrops and storage, both defensive formations, the machinery/terraces and crate destruction. Fifteen map switches preserved the correct scenery and cover models, with GPU resource counts stable at **464 geometries / 40 textures** after warm-up.

A seeded 240-frame comparison per map, discarding 60 warm-up frames, measured one simulation step and one overview render per frame in the in-app browser. Render counts include shadow passes. These are local CPU submission measurements, not GPU timing or an FPS guarantee; viewport and machine conditions differ from earlier measurements.

| Map | CPU median | CPU p95 | Mean draw calls | Mean triangles |
| --- | ---: | ---: | ---: | ---: |
| Pine Village | 4.10 ms | 4.70 ms | 1,119 | 372,283 |
| Harbor Havoc | 3.90 ms | 4.90 ms | 712 | 1,071,885 |
| Dusty Dig | 3.80 ms | 5.00 ms | 708 | 528,807 |

The interactive inspection and comparison fixture is `tests/quarry.browser.html`, including separate views for dragon’s teeth and the steel barrier.

## Sandstone asset provenance

Saved project asset: `public/textures/quarry/sandstone.webp` (1024 × 1024, 303,904 bytes). Generated with the built-in image generation tool, then resized and encoded as WebP at quality 84. Used for color and subtle bump detail, with mirrored wrapping and mipmaps.

Original generated source retained at `/Users/vlad/.codex/generated_images/01a097cb-7e17-7110-b79e-ad705cbb5091/exec-b910d85b-3443-499c-9e7a-f00ebe6f866a.png`.

Final generation prompt:

> Use case: photorealistic-natural. Asset type: square tileable game material albedo texture, 1024x1024. Create a seamless orthographic close photograph of a weathered desert sandstone quarry face, covering the entire image edge to edge. Natural pale ochre and muted beige stone, subtle warm gray mineral inclusions, fine horizontal sediment layers with slight irregular undulation, fine sandy grain and occasional thin hairline fractures. Restrained low contrast so it tiles across large 3D rock formations without obvious landmarks. Flat diffuse neutral lighting, no directional shadows, no highlights or ambient occlusion baked in. Physically plausible real rock, neither cartoon nor stylized. No perspective, no objects, no border, no labels, no writing, no dramatic deep cracks. Seamless matching edges. This will be mapped to low polygon quarry rocks, the fine texture should carry the realism.
