// Merged sun-shadow casters (`shadow_merge.rs`): depth only. Each vertex names
// the slot of its part's record among the records written for one model
// instance; the per-instance `base` (an instance-rate vertex attribute) is where
// that instance's records start, so one draw covers every instance of a model.
// Static scenery bakes world space and reads the identity record at 0.

@vertex
fn vs_shadow_merged(
    @location(0) position: vec3f,
    @location(1) slot: u32,
    @location(2) base: u32,
) -> @builtin(position) vec4f {
    let world = instance_at(base + slot).world;
    return frame.view_projection * (world * vec4f(position, 1.0));
}
