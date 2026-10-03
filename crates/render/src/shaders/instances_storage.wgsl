// WebGPU: the instance records in a storage buffer.

@group(0) @binding(5) var<storage, read> instances: array<Instance>;

fn instance_at(index: u32) -> Instance {
    return instances[index];
}
