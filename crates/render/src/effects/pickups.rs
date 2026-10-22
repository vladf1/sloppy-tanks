//! Pickup and promotion feedback (`presentation.ts` `pickupEffect`): a pillar of
//! light bursting up from the pickup, two crisp shockwaves racing over the ground
//! and a band of light rising around the collecting tank, all in the pickup's
//! color. Also the glow mesh of a pad's idle light: additive surfaces of
//! revolution whose vertex colors fade them out, so no edge reads as a flat
//! cut-out card.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::{Mat4, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Blending, Effect, Material, Side};
use sloppy_core::sim::RenderState;
use sloppy_core::sim::data::vehicle;

use super::pool::{PoolBuffer, record};
use crate::color::hex_to_linear;
use crate::presentation::models::PICKUP_BEAM_EFFECT;

/// `FEEDBACK.maxPickupEffects` and `FEEDBACK.pickupSeconds` (view-settings.ts).
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = 0.8;
/// Two shockwaves per collection.
pub const RING_CAPACITY: usize = MAX_PICKUP_EFFECTS * 2;
const RING_HEIGHT: f32 = 0.06;
/// The leading shockwave: its radius at the start, how far it spreads,
/// decelerating, and its brightness, hotter than the kind's color at first.
const RING_GROWTH: f64 = 3.6;
const RING_OPACITY: f64 = 1.0;
const RING_HEAT: f32 = 0.6;
/// The second, slower shockwave follows this many seconds behind, smaller.
const ECHO_DELAY: f64 = 0.12;
const ECHO_GROWTH: f64 = 2.2;
const ECHO_OPACITY: f64 = 0.7;
/// The ring of light that climbs the collecting tank of scale 1: its radius,
/// which tightens as it climbs, its own height, and how high it climbs.
const BAND_RADIUS: f64 = 1.85;
const BAND_TIGHTEN: f64 = 0.3;
const BAND_HEIGHT: f64 = 0.8;
const BAND_CLIMB: f64 = 2.6;
const BAND_OPACITY: f64 = 1.0;
/// The impact flash: a pillar of light that shoots up from the pickup, narrowing
/// as it climbs, white-hot at first and gone in the effect's first moments.
const FLASH_SECONDS: f64 = 0.32;
const FLASH_RADIUS: f64 = 1.25;
const FLASH_NARROW: f64 = 0.7;
const FLASH_HEIGHT: [f64; 2] = [2.2, 8.5];
const FLASH_OPACITY: f64 = 1.0;
const FLASH_HEAT: f32 = 1.2;
/// Segments around every glow mesh: smooth from the battle camera, few triangles.
/// A pad's light, standing on every pad, uses fewer.
const GLOW_SEGMENTS: u32 = 40;
const PAD_SEGMENTS: u32 = 32;

/// An additive glow surface of revolution around +y: each row is (radius, height,
/// brightness); consecutive rows join into a band whose vertex colors carry the
/// brightness, so the additive blend fades it to nothing where a row is 0.
pub fn glow_lathe(rows: &[[f64; 3]], segments: u32) -> Mesh {
    let mut mesh = Mesh::default();
    let columns = segments + 1;
    for &[radius, height, brightness] in rows {
        for step in 0..columns {
            let angle = TAU * f64::from(step) / f64::from(segments);
            let (sin, cos) = angle.sin_cos();
            mesh.positions
                .push([(radius * cos) as f32, height as f32, (radius * sin) as f32]);
            mesh.normals.push([cos as f32, 0.0, sin as f32]);
            mesh.uvs
                .push([f64::from(step) as f32 / segments as f32, height as f32]);
            mesh.colors.push([brightness as f32; 3]);
        }
    }
    let mut indices = Vec::new();
    for row in 0..rows.len().saturating_sub(1) as u32 {
        for step in 0..segments {
            let a = row * columns + step;
            let b = a + columns;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    mesh.indices = Some(indices);
    mesh
}

/// The additive, unlit, double-sided paint every pickup glow uses; instance tints
/// color it.
pub fn glow_material() -> Material {
    Material {
        transparent: true,
        depth_write: false,
        side: Side::Double,
        blending: Blending::Additive,
        vertex_colors: true,
        ..Material::basic(0xffffff)
    }
}

/// How the pad light's halos climb (`pickup_beam.wgsl`): per second of the one
/// clock, how high, how many rings share a column, and how much they narrow.
const HALO_RATE: f32 = 0.42;
const HALO_CLIMB: f32 = 3.0;
const HALO_RINGS: u32 = 2;
const HALO_NARROW: f32 = 0.38;

/// The pad light's paint: the shared glow with the halo-climbing effect.
pub fn beam_material() -> Material {
    Material {
        effect: Effect::Custom {
            name: PICKUP_BEAM_EFFECT,
            params: vec![HALO_RATE, HALO_CLIMB, HALO_RINGS as f32, HALO_NARROW],
        },
        ..glow_material()
    }
}

/// A pad's idle light: a pool of light on its deck with a brighter lens ring, a
/// beacon column rising from it past the floating gem, fading as it climbs, light
/// spilling onto the ground around the pad, and halo rings the beam effect lifts
/// up the column. Its base is at y = 0; instances stand it on the deck.
pub fn pickup_beam_geometry() -> Mesh {
    let mut pool = glow_lathe(
        &[
            [0.0, 0.0, 0.5],
            [0.4, 0.0, 0.3],
            [0.56, 0.0, 1.0],
            [0.64, 0.0, 0.32],
            [0.84, 0.0, 0.0],
        ],
        PAD_SEGMENTS,
    );
    let column = glow_lathe(
        &[
            [0.74, 0.0, 0.75],
            [0.75, 0.1, 0.5],
            [0.77, 0.5, 0.2],
            [0.74, 1.6, 0.08],
            [0.62, 3.4, 0.0],
        ],
        PAD_SEGMENTS,
    );
    // Light spilling onto the ground around the housing (radius 1.14), just above
    // the ground rather than the deck.
    let spill = glow_lathe(
        &[
            [1.12, SPILL_DROP, 0.0],
            [1.2, SPILL_DROP, 0.42],
            [1.5, SPILL_DROP, 0.16],
            [2.1, SPILL_DROP, 0.0],
        ],
        PAD_SEGMENTS,
    );
    append(&mut pool, column);
    append(&mut pool, spill);
    for ring in 0..HALO_RINGS {
        let mut halo = glow_lathe(
            &[[0.8, 0.0, 0.0], [0.93, 0.03, 1.0], [1.0, 0.0, 0.0]],
            PAD_SEGMENTS,
        );
        // The effect knows a halo vertex by its u past 1.5, and which ring by
        // its whole part.
        for uv in &mut halo.uvs {
            uv[0] = 2.0 + ring as f32;
        }
        append(&mut pool, halo);
    }
    pool
}

/// The ground spill's height relative to the pad light's base on the deck.
const SPILL_DROP: f64 = -0.17;

fn append(mesh: &mut Mesh, part: Mesh) {
    let offset = mesh.positions.len() as u32;
    mesh.positions.extend(part.positions);
    mesh.normals.extend(part.normals);
    mesh.uvs.extend(part.uvs);
    mesh.colors.extend(part.colors);
    let part_indices = part.indices.unwrap_or_default();
    if let Some(indices) = &mut mesh.indices {
        indices.extend(part_indices.into_iter().map(|index| index + offset));
    }
}

/// The collection shockwave, flat on the ground with radius 1: a crisp, bright
/// leading edge and a faint wash trailing just inside it.
pub fn shockwave_geometry() -> Mesh {
    glow_lathe(
        &[
            [0.8, 0.0, 0.0],
            [0.93, 0.0, 0.07],
            [0.97, 0.0, 0.35],
            [0.985, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.012, 0.0, 0.0],
        ],
        GLOW_SEGMENTS,
    )
}

/// The ring of light climbing the collecting tank, radius and height 1: a bright
/// leading edge near its top with a glow trailing below it.
pub fn band_geometry() -> Mesh {
    glow_lathe(
        &[
            [1.0, 0.0, 0.0],
            [1.0, 0.6, 0.4],
            [1.0, 0.88, 1.0],
            [1.0, 1.0, 0.0],
        ],
        GLOW_SEGMENTS,
    )
}

/// The collection's light pillar, radius and height 1: a sleeve of light hottest
/// at the ground, tapering and fading to nothing at its top, around a brighter
/// narrow core.
pub fn flash_geometry() -> Mesh {
    let mut sleeve = glow_lathe(
        &[
            [1.0, 0.0, 0.0],
            [1.0, 0.04, 1.0],
            [0.95, 0.3, 0.7],
            [0.8, 0.7, 0.25],
            [0.6, 1.0, 0.0],
        ],
        GLOW_SEGMENTS,
    );
    let core = glow_lathe(
        &[[0.32, 0.0, 1.0], [0.3, 0.5, 0.8], [0.2, 1.0, 0.0]],
        GLOW_SEGMENTS / 2,
    );
    append(&mut sleeve, core);
    sleeve
}

/// Ease-out cubic: fast at first, settling at the end.
fn ease_out(progress: f64) -> f64 {
    1.0 - (1.0 - progress).powi(3)
}

#[derive(Clone, Copy, Debug)]
struct PickupEffect {
    x: f64,
    z: f64,
    age: f64,
    tank: Option<u32>,
    color: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct PickupEffects {
    effects: VecDeque<PickupEffect>,
    pub rings: PoolBuffer,
    pub glows: PoolBuffer,
    pub flashes: PoolBuffer,
}

impl Default for PickupEffects {
    fn default() -> Self {
        Self {
            effects: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            rings: PoolBuffer::new(RING_CAPACITY),
            glows: PoolBuffer::new(MAX_PICKUP_EFFECTS),
            flashes: PoolBuffer::new(MAX_PICKUP_EFFECTS),
        }
    }
}

impl PickupEffects {
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn reset(&mut self) {
        self.effects.clear();
        self.rings.clear();
        self.glows.clear();
        self.flashes.clear();
    }

    /// Start a ring at a pickup, following `tank` with a glow while it lives.
    pub fn add(&mut self, x: f64, z: f64, color: u32, tank: Option<u32>) {
        if self.effects.len() >= MAX_PICKUP_EFFECTS {
            self.effects.pop_front();
        }
        self.effects.push_back(PickupEffect {
            x,
            z,
            age: 0.0,
            tank,
            color: hex_to_linear(color),
        });
    }

    pub fn update(&mut self, state: &RenderState, alpha: f64, dt: f64) {
        self.rings.clear();
        self.glows.clear();
        self.flashes.clear();
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
        for effect in &self.effects {
            let progress = effect.age / PICKUP_SECONDS;
            let spread = ease_out(progress);
            let [r, g, b] = effect.color;
            let at = Vec3::new(effect.x as f32, RING_HEIGHT, effect.z as f32);
            // White-hot at first, settling into the kind's color.
            let heat = |amount: f32, fade: f64| {
                let white = amount * fade as f32;
                [r + white, g + white, b + white]
            };
            let [hr, hg, hb] = heat(RING_HEAT, (1.0 - progress * 3.0).max(0.0));
            self.rings.push(record(
                Mat4::from_translation(at)
                    * Mat4::from_scale(Vec3::splat((1.0 + spread * RING_GROWTH) as f32)),
                [
                    hr,
                    hg,
                    hb,
                    (RING_OPACITY * (1.0 - progress).powf(1.6)) as f32,
                ],
                [0.0; 4],
            ));
            if effect.age > ECHO_DELAY {
                let echo = (effect.age - ECHO_DELAY) / (PICKUP_SECONDS - ECHO_DELAY);
                self.rings.push(record(
                    Mat4::from_translation(at)
                        * Mat4::from_scale(Vec3::splat(
                            (0.8 + ease_out(echo) * ECHO_GROWTH) as f32,
                        )),
                    [r, g, b, (ECHO_OPACITY * (1.0 - echo).powi(2)) as f32],
                    [0.0; 4],
                ));
            }
            if effect.age < FLASH_SECONDS {
                let flash = effect.age / FLASH_SECONDS;
                let radius = FLASH_RADIUS * (1.0 - FLASH_NARROW * ease_out(flash));
                let height =
                    FLASH_HEIGHT[0] + (FLASH_HEIGHT[1] - FLASH_HEIGHT[0]) * ease_out(flash);
                let [fr, fg, fb] = heat(FLASH_HEAT, 1.0 - flash);
                self.flashes.push(record(
                    Mat4::from_translation(Vec3::new(effect.x as f32, 0.0, effect.z as f32))
                        * Mat4::from_scale(Vec3::new(radius as f32, height as f32, radius as f32)),
                    [fr, fg, fb, (FLASH_OPACITY * (1.0 - flash).powf(1.5)) as f32],
                    [0.0; 4],
                ));
            }
            let Some(tank) = effect
                .tank
                .and_then(|id| state.tanks.iter().find(|t| t.id == id && t.alive))
            else {
                continue;
            };
            let p = tank.position;
            let base = Vec3::new(
                (tank.previous.x + (p.x - tank.previous.x) * alpha) as f32,
                0.0,
                (tank.previous.z + (p.z - tank.previous.z) * alpha) as f32,
            );
            let scale = vehicle(tank.kind).scale;
            let radius = BAND_RADIUS * scale * (1.0 - BAND_TIGHTEN * spread);
            let height = BAND_HEIGHT * scale;
            let lift = Vec3::Y * (BAND_CLIMB * scale * spread) as f32;
            self.glows.push(record(
                Mat4::from_translation(base + lift)
                    * Mat4::from_scale(Vec3::new(radius as f32, height as f32, radius as f32)),
                [r, g, b, (BAND_OPACITY * (1.0 - progress).powf(1.5)) as f32],
                [0.0; 4],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::render_state::RenderTank;

    #[test]
    fn rings_expand_fade_and_expire_and_the_pool_keeps_the_newest() {
        let mut state = RenderState::default();
        state.tanks.push(RenderTank {
            id: 7,
            alive: true,
            ..RenderTank::default()
        });
        let mut effects = PickupEffects::default();
        for i in 0..30 {
            effects.add(i as f64, 0.0, 0xffcf54, Some(7));
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(&state, 1.0, 0.1);
        assert_eq!(effects.flashes.len(), MAX_PICKUP_EFFECTS);
        effects.update(&state, 1.0, 0.3);
        assert!(effects.flashes.is_empty());
        assert_eq!(
            effects.rings.len(),
            RING_CAPACITY,
            "a shockwave and its echo"
        );
        assert_eq!(effects.glows.len(), MAX_PICKUP_EFFECTS);
        let first = &effects.rings.records()[0];
        assert_eq!(
            first.translation().x,
            6.0,
            "the oldest effects were replaced"
        );
        assert!((first.tint[3] - (RING_OPACITY * 0.5f64.powf(1.6)) as f32).abs() < 1e-6);
        state.tanks[0].alive = false;
        effects.update(&state, 1.0, 0.1);
        assert!(effects.glows.is_empty(), "no band without a living tank");
        effects.update(&state, 1.0, 0.3);
        assert!(effects.is_empty() && effects.rings.is_empty());
        assert!(
            effects.flashes.is_empty(),
            "the flash is gone well before the ring"
        );
    }
}
