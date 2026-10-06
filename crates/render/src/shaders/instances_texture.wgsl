// WebGL2 has no storage buffers: the instance records are rows of an RGBA32F
// texture, `RECORDS_PER_ROW` per row and one texel per vec4 (`InstanceStore` in
// `gpu/webgl/resources.rs`).

@group(0) @binding(5) var instance_records: texture_2d<f32>;

const RECORDS_PER_ROW: u32 = 256u;
const RECORD_TEXELS: u32 = 5u;

fn instance_at(index: u32) -> Instance {
    let x = i32((index % RECORDS_PER_ROW) * RECORD_TEXELS);
    let y = i32(index / RECORDS_PER_ROW);
    return expand_instance(
        textureLoad(instance_records, vec2i(x, y), 0),
        textureLoad(instance_records, vec2i(x + 1, y), 0),
        textureLoad(instance_records, vec2i(x + 2, y), 0),
        textureLoad(instance_records, vec2i(x + 3, y), 0),
        textureLoad(instance_records, vec2i(x + 4, y), 0),
    );
}
