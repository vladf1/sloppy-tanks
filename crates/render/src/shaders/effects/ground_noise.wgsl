// The ground effects' shared noise tile (`effects::ground_noise`), bound in the
// emissive map's slot of their materials: red and green two smooth value-noise
// fields on a unit lattice, blue and alpha white noise, one pair per texel. A
// filtered lookup stands in for hashing four lattice corners, which cost every
// pixel of the ground dozens of instructions per noise.

// Lattice cells per tile edge, and the tile's edge in texels (`GROUND_NOISE_SIZE`,
// `GROUND_NOISE_CELL`).
const GROUND_NOISE_CELLS = 32.0;
const GROUND_NOISE_TEXELS = 256;

// Two independent value noises in [0, 1] at `p` (lattice units), repeating every
// `GROUND_NOISE_CELLS`. The finest level, as the hashed noise it replaces: callers
// fade what a pixel no longer resolves.
fn ground_noise(p: vec2f) -> vec2f {
    return textureSampleLevel(emissive_texture, emissive_sampler, p * (1.0 / GROUND_NOISE_CELLS), 0.0).xy;
}

// Two random numbers in [0, 1] for the lattice cell holding `cell`, repeating every
// `GROUND_NOISE_TEXELS` cells.
fn ground_hash(cell: vec2f) -> vec2f {
    return textureLoad(emissive_texture, vec2i(floor(cell)) & vec2i(GROUND_NOISE_TEXELS - 1), 0).zw;
}
