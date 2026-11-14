# Cottage surfaces

Lossless sources drawn by `bun scripts/generate-house-textures.ts`;
`bun run optimize:textures` writes the 512 px runtime copies (the solar cells' at their own 128 px) to
`public/textures/houses/`. Each image tiles in both directions and is also its own
bump map, so joints and shadowed laps are darker (lower) than the faces.

| Source          | Content                                                       | Mapped by                                                           |
| --------------- | ------------------------------------------------------------- | ------------------------------------------------------------------- |
| `clapboard.webp` | 12 courses of painted lap siding, near white for paint tints | `building_kit.rs`, 2.4 m per tile on cottage and lookout walls      |
| `brick.webp`     | 16 courses of running-bond brick in dark joints               | `building_kit.rs`, 1.2 m per tile on chimneys                       |
| `stone.webp`     | Mortared fieldstone                                           | `building_kit.rs`, 1.6 m per tile on plinths and steps              |
| `shingles.webp`  | 8 courses of asphalt shingles, legacy unflipped row order     | `house_surfaces.rs`; the cottages map one row per shingle course    |
| `stucco.webp`   | Fine dash-finish stucco: tiny rounded bumps, near white        | `house_model.rs`, 0.6 m per tile on the Neighborhood's walls          |
| `solar.webp`    | 8 x 8 solar cells, dark navy in silver gaps, 128 px, in its own colours | `building_kit.rs`, 0.71 m per tile on the Neighborhood's solar arrays |

`clapboard.webp`'s average colour is `CLAPBOARD_AVERAGE` in `building_kit.rs`, and
`stucco.webp`'s is `STUCCO_AVERAGE`; the generator prints them, so update the constant
when an image changes. `siding.webp`
(crates, sheds, the watermill's walls) still comes from `generate-textures.ts`.
