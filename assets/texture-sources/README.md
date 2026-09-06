# Generated environment textures

## Perimeter concrete

Created with the built-in image generation tool. Original: `weathered-concrete.png` (1254 × 1254). Runtime: `public/textures/walls/weathered-concrete.webp`, encoded with `cwebp -q 88 -m 6 -metadata none`. All four perimeter walls share the image, with a four-metre tile size on faces and tops. The saved asset ships directly; neither browser nor CI regenerates it.

Prompt: Create a seamless tileable square high-resolution albedo texture for weathered poured cement perimeter walls in a colorful top-down 3D tank game. Orthographic straight-on flat surface filling entire image, no perspective. Warm light gray concrete, irregular broad cloudy cement variation, fine aggregate and pores, a few short subtle hairline cracks, faint formwork impressions. Readable medium-scale mottling at game distance with detailed grain up close. Moderately worn but structurally sound, clean light cement overall, restrained contrast. Seamless all four edges, uniform neutral diffuse lighting, no directional shadows, no ambient occlusion, no lighting gradients, no objects, no ground, no sky, no text, no logos, no borders, no large dramatic fissures, no graffiti. Single continuous material texture, not a photo of a wall in an environment.

Created with the built-in image generation tool on 2026-09-05. Original outputs are preserved here; game assets are WebP quality 88 under `public/textures/ground/`. The tool returned 1254 × 1254 images despite the requested 2048 × 2048; they are used at native resolution, without upscaling.

## Dry grass prompt

Use case: stylized-concept
Asset type: production seamless tileable game ground base-color texture, square 2048x2048 pixels.
Primary request: A richly detailed top-down texture of short dry meadow grass over sandy soil, for a sunny stylized toy tank village. Olive and sage green small blades interspersed with straw-yellow blades, minute bare tan earth flecks and a few tiny embedded pebbles. Dense fine hand-painted natural detail, restrained organic variation at medium scale, readable and calm behind brightly colored tanks. Ground coverage about 80% grass and 20% soil. One tile represents about 8 by 8 meters, so all blades and pebbles must be small.
Composition/framing: exact orthographic overhead, texture fills the entire square edge to edge, no perspective.
Lighting: flat diffuse albedo only, even exposure all over, no directional light, no cast shadows, no ambient occlusion or vignette.
Color palette: muted medium sage/olive greens with warm dry straw and earthy tan accents, avoid pale washed-out cream.
Constraints: perfectly seamless left/right AND top/bottom edges, no conspicuous focal patches or clusters that reveal repetition, no paths, flowers, large rocks, objects, text, borders, grid, watermarks. Output the texture alone in high resolution.

## Packed dirt prompt

Use case: stylized-concept
Asset type: production seamless tileable game road base-color texture, square 2048x2048 pixels.
Primary request: Finely detailed warm compacted sandy earth with tiny embedded grit and scattered small muted ochre/taupe pebbles, for broad dirt roads in a sunny stylized toy tank village. Subtle irregular weathering, small worn soil flakes, very faint old granular scuffs with no preferred direction. Hand-painted natural surface detail, restrained contrast, calm under bright tanks. Tile covers about 8 by 8 meters, pebbles only a few centimeters; ground material only, not a picture of a road.
Composition/framing: exact orthographic overhead, fill the whole square edge to edge.
Lighting: flat diffuse albedo only, even exposure, no directional lighting, shadows, vignette, or ambient occlusion.
Color palette: medium warm sand and ochre, muted biscuit tan, earthy brown flecks, avoid white or bright yellow.
Constraints: perfectly seamless left/right and top/bottom, uniform material distribution, no large dark patches that reveal repetition. No grass, no road boundaries, no lane marks, no tire tracks, no large rocks, objects, footprints, text, border, grid, watermarks. Output texture alone at high resolution.

## Encoding

```sh
cwebp -q 88 -m 6 -metadata none assets/texture-sources/dry-grass.png -o public/textures/ground/dry-grass.webp
cwebp -q 88 -m 6 -metadata none assets/texture-sources/packed-dirt.png -o public/textures/ground/packed-dirt.webp
```
