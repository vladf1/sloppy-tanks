// A puff's card (`puff.wgsl`), a shared vertex hook (`EffectDefinition::shared_vertex`).
fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {
    let corner = (*w).position - v.instance_origin;
    // The card stands at the front of the ball it draws, pulled toward the eye
    // by most of its radius, so a hull or the ground the puff envelops cuts its
    // far side rather than slicing through its middle. A low puff is pulled
    // further, until its card's lowest edge clears the ground: a card that dips
    // into the ground would be cut along every step of the terrain. Fire stands
    // where it burns: a hull in front of a lobe hides it, so a vehicle's blast
    // shows the hull it erupts from.
    let radius = abs(corner.x);
    let height = abs(corner.y);
    let to_eye_far = frame.camera_position.xyz - v.instance_origin;
    let to_eye = normalize(to_eye_far);
    // The card keeps the world's up rather than the camera's, so a rolling or
    // shaking camera does not turn every billow with it.
    var up = vec3f(0.0, 1.0, 0.0) - to_eye * to_eye.y;
    if dot(up, up) < 1e-4 {
        up = frame.camera_up.xyz;
    }
    up = normalize(up);
    let right = normalize(cross(up, to_eye));
    let lowest = v.instance_origin.y - abs(up.y) * height;
    let lift = max(0.06 - lowest, 0.0) / max(to_eye.y, 0.2);
    let far = length(to_eye_far);
    // However near the camera a billow drifts, its card never spans more than
    // this share of its distance, so smoke can veil part of the view but never
    // blank it out.
    let pull = min(max(radius * 0.6, lift), far * 0.4);
    let cap = min((far - pull) * PUFF_SCREEN_SHARE / max(radius, 1e-3), 1.0);
    (*w).position = v.instance_origin + to_eye * pull + (right * corner.x + up * corner.y) * cap;
}

// The widest a puff's card may be, as a share of its distance from the eye.
const PUFF_SCREEN_SHARE: f32 = 0.2;
