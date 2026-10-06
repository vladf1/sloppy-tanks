// WebGPU: the instance records in a storage buffer.

struct InstanceRecord {
    world_rows: array<vec4f, 3>,
    tint: vec4f,
    data: vec4f,
}

@group(0) @binding(5) var<storage, read> instances: array<InstanceRecord>;

fn instance_at(index: u32) -> Instance {
    let record = instances[index];
    return expand_instance(
        record.world_rows[0],
        record.world_rows[1],
        record.world_rows[2],
        record.tint,
        record.data,
    );
}
