// Flag cloth (`flags.ts`): the cloth stays attached to the pole while gusts
// carry ripples toward its free edge. Replaces the plane with the posed cloth in
// its local frame and rebuilds the normal from the same neighbouring grid
// triangles computeVertexNormals() would sum. instance_data = (gust strength,
// wind x, wind z, phase); frame.camera_position.w is the clock.

fn flag_point(u: f32, v: f32, t: f32, data: vec4f) -> vec3f {
    let gust = data.x;
    let along_x = data.y;
    let along_z = data.z;
    let phase = data.w;
    let ripple = sin(u * 9.0 - t * 5.0 + phase + v * 1.8);
    let flutter = sin(u * 19.0 - t * 8.0 + phase) * 0.035 * (u * u);
    let reach = u * (gust * 0.3 + 1.05);
    let sideways = u * (gust * 0.09 + 0.1) * ripple + flutter;
    return vec3f(
        reach * along_x + sideways * along_z,
        v * -0.9 - u * u * (0.45 - gust * 0.22) + u * 0.045 * ripple,
        reach * along_z - sideways * along_x,
    );
}

fn effect_vertex(v: ptr<function, EffectVertex>) {
    let data = (*v).instance_data;
    let t = frame.camera_position.w;
    let u = (*v).uv.x;
    let w = 1.0 - (*v).uv.y;
    let du = 1.0 / 16.0;
    let dv = 1.0 / 6.0;
    let p = flag_point(u, w, t, data);
    let up = flag_point(u, w - dv, t, data);
    let down = flag_point(u, w + dv, t, data);
    let left = flag_point(u - du, w, t, data);
    let right = flag_point(u + du, w, t, data);
    let north_east = flag_point(u + du, w - dv, t, data);
    let south_west = flag_point(u - du, w + dv, t, data);
    var normal = vec3f(0.0);
    if u < 1.0 && w < 1.0 {
        normal += cross(down - p, right - p);
    }
    if u < 1.0 && w > 0.0 {
        normal += cross(p - up, north_east - up);
        normal += cross(right - p, north_east - p);
    }
    if u > 0.0 && w > 0.0 {
        normal += cross(p - left, up - left);
    }
    if u > 0.0 && w < 1.0 {
        normal += cross(south_west - left, p - left);
        normal += cross(down - south_west, p - south_west);
    }
    (*v).position = p;
    (*v).normal = normalize(normal);
}
