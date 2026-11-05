//! A standing cottage's parts that a shell sets moving: each broken eave section
//! ([`HOUSE_EAVE`]) swings down about the ridge line to the angle its model draws it
//! at, rather than appearing there. The model is rebuilt from the cottage's marks
//! after every shell and draws a section already turned as far as its breach lets
//! it; presentation keeps the angle each section is shown at and eases the
//! section's node from there, so it falls like a member let go (gathering speed,
//! landing with a small bounce) and a later shell that deepens the drop moves it
//! on from where it hung.
//!
//! The simulation never sees any of this: a collapse's pieces are cut at the
//! sections' full angle, which they reach a few hundred milliseconds after a shell
//! (shells come a second or more apart), so the first frame of a collapse matches
//! what the cottage showed.

use glam::{Mat4, Vec3};
use sloppy_core::models::EaveSwing;

#[cfg(any(target_arch = "wasm32", feature = "native"))]
pub use live::HouseMotion;

/// How fast (rad/s²) a broken eave section gathers speed as it swings down: about
/// what a 2.5 m slab hinged at its top end and let go at a shallow angle does, so a
/// typical drop of 0.1-0.2 rad takes 250-400 ms.
const SWING_ACCEL: f64 = 3.0;
/// The share of its speed a section keeps, rebounding, when it lands on the stop
/// that holds it (the broken wall's top, the rafters' seat).
const SWING_BOUNCE: f64 = 0.3;
/// Below this speed (rad/s) a landing section stays down.
const SWING_REST: f64 = 0.08;
/// The longest frame (s) a swing steps by at once: a stalled frame does not throw a
/// section through its stop.
const SWING_STEP: f64 = 1.0 / 30.0;
/// The most (s) of a frame a swing advances by: after a long stall it goes on from
/// where it was rather than landing unseen.
#[cfg_attr(not(any(target_arch = "wasm32", feature = "native")), allow(dead_code))]
const SWING_FRAME: f64 = 0.1;

/// One section's swing: the angle it is shown at and how fast it is turning, toward
/// the angle its model draws it at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    pub angle: f64,
    pub speed: f64,
    pub target: f64,
}

impl Swing {
    /// A swing at rest at `angle` toward `target`.
    pub fn at(angle: f64, target: f64) -> Self {
        Self {
            angle: angle.min(target),
            speed: 0.0,
            target,
        }
    }

    pub fn moving(&self) -> bool {
        self.angle != self.target || self.speed != 0.0
    }

    /// Advance by `dt` seconds: down under its own weight, bouncing off its stop at
    /// the target until it stays there.
    pub fn step(&mut self, dt: f64) {
        let mut left = dt.max(0.0);
        while left > 0.0 && self.moving() {
            let dt = left.min(SWING_STEP);
            left -= dt;
            self.speed += SWING_ACCEL * dt;
            self.angle += self.speed * dt;
            if self.angle >= self.target {
                self.angle = self.target;
                self.speed = if self.speed > SWING_REST {
                    -self.speed * SWING_BOUNCE
                } else {
                    0.0
                };
            }
        }
    }
}

/// The local transform of a section's node shown at `angle` where its model draws it
/// at `shape.angle`: turned back about the ridge line by the difference.
#[cfg_attr(not(any(target_arch = "wasm32", feature = "native")), allow(dead_code))]
fn section_pose(rest: Mat4, shape: &EaveSwing, angle: f64) -> Mat4 {
    let hinge = Vec3::new(0.0, shape.ridge as f32, 0.0);
    let turn = (-shape.side * (angle - shape.angle)) as f32;
    rest * Mat4::from_translation(hinge)
        * Mat4::from_rotation_z(turn)
        * Mat4::from_translation(-hinge)
}

/// Whether two sections are the same piece of roof seen in two looks: on the same
/// slope and overlapping along it.
#[cfg_attr(not(any(target_arch = "wasm32", feature = "native")), allow(dead_code))]
fn same_section(a: &EaveSwing, b: &EaveSwing) -> bool {
    a.side == b.side && a.from < b.to && b.from < a.to
}

#[cfg(any(target_arch = "wasm32", feature = "native"))]
mod live {
    use glam::Mat4;
    use sloppy_core::models::{EaveSwing, HOUSE_EAVE, house_eave_swings};
    use sloppy_core::sim::CoverKind;
    use sloppy_core::sim::render_state::RenderCover;

    use super::{SWING_FRAME, Swing, same_section, section_pose};
    use crate::gpu::{InstanceId, ModelId, Renderer};

    /// A section's node of the cottage's model and where it turns.
    #[derive(Clone, Copy, Debug)]
    struct Section {
        joint: usize,
        /// The node's own transform, which draws it at its target.
        rest: Mat4,
        swing: Swing,
        shape: EaveSwing,
    }

    /// One cottage's moving parts, on its current instance.
    #[derive(Debug)]
    struct Cottage {
        cover: u32,
        instance: InstanceId,
        sections: Vec<Section>,
    }

    /// The cottages' moving parts: one entry for each cottage shown this round (a
    /// handful of sections each at most), replaced as each new look is installed.
    #[derive(Debug, Default)]
    pub struct HouseMotion {
        cottages: Vec<Cottage>,
    }

    impl HouseMotion {
        pub fn clear(&mut self) {
            self.cottages.clear();
        }

        /// A cottage's new look is shown as `instance` of `model`: its sections start
        /// where the last look showed them (a new one level, as the roof stood, unless
        /// this is the first look the cottage is shown in, as for a late joiner) and
        /// swing on to where the new look draws them.
        pub fn install(
            &mut self,
            renderer: &Renderer,
            cover: &RenderCover,
            model: ModelId,
            instance: InstanceId,
        ) {
            if cover.kind != CoverKind::House {
                return;
            }
            let index = match self.cottages.iter().position(|c| c.cover == cover.id) {
                Some(index) => index,
                None => {
                    self.cottages.push(Cottage {
                        cover: cover.id,
                        instance,
                        sections: Vec::new(),
                    });
                    // First shown: as built, nothing to swing from.
                    let cottage = self.cottages.last_mut().expect("pushed");
                    cottage.sections = sections(renderer, cover, model, &[], false);
                    return;
                }
            };
            // The new look is posed by the next [`Self::update`], before it draws.
            let cottage = &mut self.cottages[index];
            let previous = std::mem::take(&mut cottage.sections);
            cottage.instance = instance;
            cottage.sections = sections(renderer, cover, model, &previous, true);
        }

        /// Swing every moving section on by `dt` seconds.
        pub fn update(&mut self, renderer: &mut Renderer, dt: f64) {
            for cottage in &mut self.cottages {
                for section in &mut cottage.sections {
                    if !section.swing.moving() {
                        continue;
                    }
                    section.swing.step(dt.min(SWING_FRAME));
                    let pose = section
                        .swing
                        .moving()
                        .then(|| section_pose(section.rest, &section.shape, section.swing.angle));
                    renderer.set_node_transform(cottage.instance, section.joint, pose);
                }
            }
        }
    }

    /// The sections of a cottage's look, each starting from the matching one of the
    /// last look's (`previous`), or level when `swings` (a section new to a cottage
    /// already shown) or at its angle (the cottage's first look).
    fn sections(
        renderer: &Renderer,
        cover: &RenderCover,
        model: ModelId,
        previous: &[Section],
        swings: bool,
    ) -> Vec<Section> {
        let nodes = renderer.model_nodes(model);
        house_eave_swings(
            cover.x,
            cover.z,
            cover.w,
            cover.d,
            cover.h,
            &cover.timber_hits,
        )
        .into_iter()
        .enumerate()
        .filter_map(|(k, shape)| {
            let joint = renderer.model_node(model, &format!("{HOUSE_EAVE}{k}"))?;
            let from = previous
                .iter()
                .find(|old| same_section(&old.shape, &shape))
                .map(|old| (old.swing.angle, old.swing.speed));
            let swing = match from {
                Some((angle, speed)) => Swing {
                    speed: if angle < shape.angle { speed } else { 0.0 },
                    ..Swing::at(angle, shape.angle)
                },
                None if swings => Swing::at(0.0, shape.angle),
                None => Swing::at(shape.angle, shape.angle),
            };
            Some(Section {
                joint,
                rest: nodes[joint].rest,
                swing,
                shape,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section let go falls in a few hundred milliseconds, gathering speed, comes
    /// to rest exactly at its angle after a small bounce, and never past it.
    #[test]
    fn a_broken_eave_swings_down_and_settles_at_its_angle() {
        let mut swing = Swing::at(0.0, 0.15);
        let mut angles = Vec::new();
        for _ in 0..60 {
            swing.step(1.0 / 60.0);
            angles.push(swing.angle);
            assert!(swing.angle <= 0.15 + 1e-12);
        }
        let landed = angles.iter().position(|&a| a >= 0.15).expect("lands");
        assert!((12..=27).contains(&landed), "landed at frame {landed}");
        // Faster at the end than at the start.
        assert!(angles[landed - 1] - angles[landed - 2] > 3.0 * (angles[1] - angles[0]));
        // A bounce: back up a little after landing, then down again.
        let after = &angles[landed..];
        assert!(after.iter().any(|&a| a < 0.15 && a > 0.12), "{after:?}");
        assert!(!swing.moving());
        assert_eq!(swing.angle, 0.15);
    }

    /// A later shell deepening the drop sets a resting section moving on from where
    /// it hung, not from level.
    #[test]
    fn a_deeper_drop_moves_on_from_where_the_section_hung() {
        let mut swing = Swing::at(0.1, 0.1);
        assert!(!swing.moving());
        swing = Swing {
            speed: 0.0,
            ..Swing::at(swing.angle, 0.2)
        };
        swing.step(1.0 / 60.0);
        assert!(swing.angle > 0.1 && swing.angle < 0.11);
        for _ in 0..60 {
            swing.step(1.0 / 60.0);
        }
        assert_eq!(swing.angle, 0.2);
    }

    #[test]
    fn a_pose_at_the_target_is_the_rest_pose() {
        let shape = EaveSwing {
            side: 1.0,
            from: -1.0,
            to: 1.0,
            ridge: 4.5,
            angle: 0.2,
        };
        let rest = Mat4::from_rotation_y(0.7);
        let pose = section_pose(rest, &shape, 0.2);
        assert!(pose.abs_diff_eq(rest, 1e-6));
        // Level: the eave end (out along +x) comes back up.
        let level = section_pose(Mat4::IDENTITY, &shape, 0.0);
        let eave = level.transform_point3(Vec3::new(2.8, 4.5 - 2.8 * 0.6, 0.0));
        assert!(eave.y > 4.5 - 2.8 * 0.6);
    }
}
