//! Values that can change a fragment's drawn appearance. Remaining lifetime by
//! itself is not a visual change until sinking starts; settled debris can keep
//! its instance state while wreck aging and cleanup still advance.

use glam::{Quat, Vec3};
use sloppy_core::models::wreck_brightness;
use sloppy_core::sim::FragmentShape;
use sloppy_core::sim::debris_cleanup::debris_cleanup_progress;
use sloppy_core::sim::render_state::RenderFragment;

use super::posing::{quat, vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FragmentAppearance {
    pub position: Vec3,
    pub rotation: Quat,
    size: f32,
    dimensions: Option<Vec3>,
    shape: Option<FragmentShape>,
    color: u32,
    pub cleanup: f32,
    pub brightness: f32,
}

impl FragmentAppearance {
    pub fn new(fragment: &RenderFragment, elapsed: f64) -> Self {
        Self {
            position: vec3(fragment.position),
            rotation: quat(fragment.rotation),
            size: fragment.size as f32,
            dimensions: fragment.dimensions.map(vec3),
            shape: fragment.shape,
            color: fragment.color,
            cleanup: debris_cleanup_progress(fragment.life) as f32,
            brightness: if fragment.wreck.is_some() {
                wreck_brightness(elapsed - fragment.created_at.unwrap_or(elapsed)) as f32
            } else {
                1.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::VehicleKind;
    use sloppy_core::sim::math::Point3;

    #[test]
    fn settled_debris_stays_cached_until_it_moves_or_cleans_up() {
        let mut fragment = RenderFragment {
            life: 30.0,
            size: 1.0,
            ..Default::default()
        };
        let initial = FragmentAppearance::new(&fragment, 0.0);
        fragment.life = 2.0;
        assert_eq!(initial, FragmentAppearance::new(&fragment, 28.0));
        fragment.life = 0.5;
        let sinking = FragmentAppearance::new(&fragment, 29.5);
        assert_ne!(initial, sinking);
        assert_eq!(sinking.cleanup, 0.5);
        fragment.position.x += 1.0;
        assert_ne!(sinking, FragmentAppearance::new(&fragment, 29.5));
    }

    #[test]
    fn appearance_changes_include_rotation_size_color_and_wreck_aging() {
        let mut fragment = RenderFragment {
            life: 30.0,
            size: 1.0,
            ..Default::default()
        };
        let mut previous = FragmentAppearance::new(&fragment, 0.0);
        for change in 0..5 {
            match change {
                0 => fragment.rotation.y = 0.5,
                1 => fragment.size = 2.0,
                2 => {
                    fragment.dimensions = Some(Point3 {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                    })
                }
                3 => fragment.color = 0xff0000,
                _ => fragment.shape = Some(FragmentShape::Shard),
            }
            let next = FragmentAppearance::new(&fragment, 0.0);
            assert_ne!(previous, next);
            previous = next;
        }
        fragment.wreck = Some(VehicleKind::Balanced);
        fragment.created_at = Some(0.0);
        assert_ne!(
            FragmentAppearance::new(&fragment, 0.0),
            FragmentAppearance::new(&fragment, 1.0)
        );
        assert_eq!(
            FragmentAppearance::new(&fragment, 10.0),
            FragmentAppearance::new(&fragment, 20.0)
        );
    }
}
