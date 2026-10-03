// WebGL2 has no storage buffers: the instance records are rows of an RGBA32F
// texture, `RECORDS_PER_ROW` per row and one texel per vec4 (`instance_store.rs`).

@group(0) @binding(5) var instance_records: texture_2d<f32>;

const RECORDS_PER_ROW: u32 = 256u;

fn instance_at(index: u32) -> Instance {
    let x = i32((index % RECORDS_PER_ROW) * 6u);
    let y = i32(index / RECORDS_PER_ROW);
    var instance: Instance;
    instance.world = mat4x4f(
        textureLoad(instance_records, vec2i(x, y), 0),
        textureLoad(instance_records, vec2i(x + 1, y), 0),
        textureLoad(instance_records, vec2i(x + 2, y), 0),
        textureLoad(instance_records, vec2i(x + 3, y), 0),
    );
    instance.tint = textureLoad(instance_records, vec2i(x + 4, y), 0);
    instance.data = textureLoad(instance_records, vec2i(x + 5, y), 0);
    return instance;
}
