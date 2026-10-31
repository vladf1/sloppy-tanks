# Generated environment textures

## Perimeter concrete

Created with the built-in image generation tool. Original: `weathered-concrete.webp` (1254 × 1254). Runtime: `public/textures/walls/weathered-concrete.webp`, encoded with `cwebp -q 88 -m 6 -metadata none`. All four perimeter walls share the image, with a four-metre tile size on faces and tops. The saved asset ships directly; neither browser nor CI regenerates it.

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

## Pine Village ground plan

`public/textures/ground/village-ground.webp` is data, not artwork: the village ground
shader's baked plan of worn dirt, trampled grass, cottage gardens, contact shade and
the farmland outside the wall (`crates/core/src/models/village_ground.rs` documents
the channels). It is baked from the arena layout in Rust and stored losslessly; rerun
`node --import tsx scripts/generate-village-ground.ts` after changing the layout,
roads, cottages or fields.

## Home Screen icons

`public/icons/` (`apple-touch-icon.png`, `icon-192.png`, `icon-512.png`) are drawn by
`pnpm run generate:app-icons`: Chrome renders the favicon's tank on the menu's navy, and
[oxipng](https://github.com/shssoichiro/oxipng) (`brew install oxipng`) then recompresses
them losslessly. Keep them PNG: iOS takes only a PNG Home Screen icon.

## macOS app icon

`assets/desktop/mac-app.icns` is generated from the existing `public/icons/icon-512.png`
by `scripts/generate-desktop-icon.mjs`, using the project's Node canvas library and
a portable ICNS writer. It retains the original app icon's rounded square, margin and
shadow, halving the canvas at each step to keep small entries smooth. Run
`pnpm run generate:desktop-icon` on any supported OS after changing the source icon
or drawing recipe; `pnpm run generate:app-icons` regenerates it automatically after
the web icons. Commit the `.icns` and adjacent `mac-app-icon.json` together. The JSON
records source, drawing recipe and output hashes; bump the recipe's version when
changing the drawing algorithm. Comment, formatting and verifier edits do not stale
the icon. Packaging and a page-shell test reject stale assets and name the changed
inputs. Normal desktop builds only verify and copy it, without running an image
generator or compiling a Rust example. Desktop-only assets stay outside `public/`
so web deployments do not carry them.

## Display font

The page draws its headings in Anton Wide, which `scripts/build-fonts.py` (fontTools)
makes from Anton (`assets/fonts/Anton-Regular.ttf`, SIL Open Font License 1.1, its
license beside both) and packs with Roboto Regular and Bold (`assets/fonts/`, Apache
2.0) into `public/fonts/interface.ttc`, one download that Brotli makes 12% smaller than
the three files apart. The page set its headings in Impact,
which may not be redistributed; Anton is narrower, so the script stretches its outlines,
advances and kerning by 11% (Impact's letters over Anton's across the interface's
uppercase strings), gives the space Impact's width and drops the hinting. The native apps
draw the system's Impact. Rerun
`uv run --with fonttools python scripts/build-fonts.py` after changing the fit.

## Battle Setup logo

The logo's letters (`LOGO_SLOPPY` and `LOGO_TANKS` in `crates/desktop/src/ui/icons.rs`)
are path data baked from Anton (`assets/fonts/Anton-Regular.ttf`, SIL Open Font License
1.1, its license beside it) by `scripts/generate-logo-outlines.ts`. The page set the
logo in Impact, whose outlines may not be redistributed; until the vector logo it was
`public/previews/logo.webp`, a 3x capture of the production page's `.setup-brand`.
The generator stretches each Anton capital into the ink box its Impact letter took
there (Impact's metrics as Chrome laid the logo out, registered against that capture
to a tenth of a pixel) and grows the outlines by 0.75% of a letter's height, which
matched Impact's heavier strokes best. The shell rasterizes the paths once at start
with the page's tilt and `text-shadow`s, so no build embeds the font for the logo.
Rerun `node --import tsx scripts/generate-logo-outlines.ts` after changing the fit.

## Encoding

The current runtime versions are generated by `pnpm run optimize:textures`. Grass uses 768×768 at quality 75; dirt and concrete use 512×512 at quality 80. Their lossless originals remain at 1254×1254. Quarry sandstone uses 768×768 at quality 75 from the preserved pre-optimization WebP in `quarry/`; no lossless sandstone original was available in the repository. The earlier quality-88 commands below are source history, not current settings. Tank wear and conifer lossless WebP sources are also preserved here; the optimizer keeps their dimensions and writes WebP runtime copies, with lossless foliage alpha.

```sh
cwebp -q 88 -m 6 -metadata none assets/texture-sources/dry-grass.webp -o public/textures/ground/dry-grass.webp
cwebp -q 88 -m 6 -metadata none assets/texture-sources/packed-dirt.webp -o public/textures/ground/packed-dirt.webp
```
