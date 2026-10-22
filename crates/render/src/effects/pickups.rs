//! Pickup and promotion feedback (`presentation.ts` `pickupEffect`): a soft
//! shockwave spreading over the ground from the pickup and a band of light rising
//! around the collecting tank, both in the pickup's color. Also the glow meshes a
//! pad's idle light column shares with them: additive surfaces of revolution
//! whose vertex colors fade them out, so no edge reads as a flat cut-out card.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::{Mat4, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Blending, Material, Side};
use sloppy_core::sim::RenderState;
use sloppy_core::sim::data::vehicle;

use super::pool::{PoolBuffer, record};
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` and `FEEDBACK.pickupSeconds` (view-settings.ts).
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = 0.8;
const RING_HEIGHT: f32 = 0.06;
/// The shockwave's radius at the start and how far it spreads, decelerating.
const RING_GROWTH: f64 = 3.0;
const RING_OPACITY: f64 = 0.95;
/// The band of light around a tank of scale 1: its radius, which tightens as it
/// rises, and its height, which it reaches early and then fades at.
const BAND_RADIUS: f64 = 1.75;
const BAND_TIGHTEN: f64 = 0.18;
const BAND_HEIGHT: f64 = 2.6;
const BAND_OPACITY: f64 = 0.9;
/// The impact flash: a bright dome over the pickup that swells and is gone in the
/// effect's first moments.
const FLASH_SECONDS: f64 = 0.18;
const FLASH_RADIUS: f64 = 1.1;
const FLASH_GROWTH: f64 = 0.6;
const FLASH_HEIGHT: f64 = 1.8;
const FLASH_OPACITY: f64 = 0.75;
/// Segments around every glow mesh: smooth from the battle camera, few triangles.
const GLOW_SEGMENTS: u32 = 40;

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

/// A pad's idle light: a pool of light on its deck with a brighter lens ring, and
/// a column rising from it toward the floating gem, fading as it climbs. Its base
/// is at y = 0; instances stand it on the deck.
pub fn pickup_beam_geometry() -> Mesh {
    let mut pool = glow_lathe(
        &[
            [0.0, 0.0, 0.5],
            [0.4, 0.0, 0.3],
            [0.56, 0.0, 1.0],
            [0.64, 0.0, 0.32],
            [0.84, 0.0, 0.0],
        ],
        GLOW_SEGMENTS,
    );
    let column = glow_lathe(
        &[
            [0.74, 0.0, 0.7],
            [0.75, 0.1, 0.48],
            [0.78, 0.5, 0.16],
            [0.82, 1.05, 0.0],
        ],
        GLOW_SEGMENTS,
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
        GLOW_SEGMENTS,
    );
    append(&mut pool, column);
    append(&mut pool, spill);
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

/// The collection shockwave, flat on the ground with radius 1: a bright leading
/// edge and a soft wash trailing inside it.
pub fn shockwave_geometry() -> Mesh {
    glow_lathe(
        &[
            [0.6, 0.0, 0.0],
            [0.9, 0.0, 0.18],
            [0.975, 0.0, 0.6],
            [0.995, 0.0, 1.0],
            [1.015, 0.0, 0.0],
        ],
        GLOW_SEGMENTS,
    )
}

/// The rising band around the collecting tank, radius and height 1: brightest
/// at the ground, gone at the top.
pub fn band_geometry() -> Mesh {
    glow_lathe(
        &[
            [1.0, 0.0, 0.85],
            [1.0, 0.12, 0.6],
            [1.0, 0.5, 0.2],
            [1.0, 1.0, 0.0],
        ],
        GLOW_SEGMENTS,
    )
}

/// The collection's impact flash, radius and height 1: a dome of light, hottest
/// at its crown, fading to nothing at the ground.
pub fn flash_geometry() -> Mesh {
    glow_lathe(
        &[
            [0.0, 1.0, 1.0],
            [0.35, 0.92, 0.85],
            [0.7, 0.66, 0.45],
            [0.92, 0.35, 0.15],
            [1.0, 0.0, 0.0],
        ],
        GLOW_SEGMENTS,
    )
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
            rings: PoolBuffer::new(MAX_PICKUP_EFFECTS),
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
            let fade = (1.0 - progress).powi(2);
            let spread = ease_out(progress);
            let [r, g, b] = effect.color;
            let ring =
                Mat4::from_translation(Vec3::new(effect.x as f32, RING_HEIGHT, effect.z as f32))
                    * Mat4::from_scale(Vec3::splat((1.0 + spread * RING_GROWTH) as f32));
            self.rings.push(record(
                ring,
                [r, g, b, (RING_OPACITY * fade) as f32],
                [0.0; 4],
            ));
            if effect.age < FLASH_SECONDS {
                let flash = effect.age / FLASH_SECONDS;
                let radius = FLASH_RADIUS * (1.0 + FLASH_GROWTH * ease_out(flash));
                let height = FLASH_HEIGHT * (1.0 - 0.4 * flash);
                // White-hot at first, settling into the kind's color.
                let white = (1.0 - flash) as f32 * 0.7;
                self.flashes.push(record(
                    Mat4::from_translation(Vec3::new(effect.x as f32, 0.0, effect.z as f32))
                        * Mat4::from_scale(Vec3::new(radius as f32, height as f32, radius as f32)),
                    [
                        r + white,
                        g + white,
                        b + white,
                        (FLASH_OPACITY * (1.0 - flash).powi(2)) as f32,
                    ],
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
            let height = BAND_HEIGHT * scale * (0.35 + 0.65 * spread);
            self.glows.push(record(
                Mat4::from_translation(base)
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
        assert_eq!(effects.rings.len(), MAX_PICKUP_EFFECTS);
        assert_eq!(effects.glows.len(), MAX_PICKUP_EFFECTS);
        let first = &effects.rings.records()[0];
        assert_eq!(
            first.translation().x,
            6.0,
            "the oldest effects were replaced"
        );
        assert!((first.tint[3] - (RING_OPACITY * 0.25) as f32).abs() < 1e-6);
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
