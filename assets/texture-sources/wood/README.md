# Timber

`timber.webp` is the lossless source of the timber wall atlas, drawn by
`node --import tsx scripts/generate-wood-texture.ts`: three rows of flat-sawn pine
plank faces (rings, cathedral arches, fibres and knots), each tileable left to
right, over a row of four end-grain cells (rings around an off-centre pith, drying
checks, saw marks). `crates/core/src/models/timber_model.rs` maps the plank rows
along each member at 2.4 m per atlas width and the cells onto its ends; the image
also serves as the members' bump map.

`pnpm run optimize:textures` writes the runtime copy, `public/textures/wood/timber.webp`
(1024 px, quality 82).
