//! Stable cosmetic/aftermath choices for a destroyed tank, without consuming the combat RNG.

use super::math::to_int32;

fn destruction_hash(seed: i32, id: u32, deaths: u32) -> u32 {
    let mut hash = (seed
        ^ (id as i32).wrapping_mul(0x9e3779b1u32 as i32)
        ^ (deaths as i32).wrapping_mul(0x85ebca6bu32 as i32)) as u32;
    hash = ((hash ^ (hash >> 16)) as i32).wrapping_mul(0x7feb352d) as u32;
    hash = ((hash ^ (hash >> 15)) as i32).wrapping_mul(0x846ca68bu32 as i32) as u32;
    hash ^ (hash >> 16)
}

/// Whether this death is a quiet burnout rather than a violent breakup.
pub fn tank_burnout(seed: f64, id: u32, deaths: u32) -> bool {
    destruction_hash(to_int32(seed), id, deaths).is_multiple_of(5)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumveeTumble {
    pub height: f64,
    pub pitch: f64,
    pub roll: f64,
    pub yaw: f64,
    pub damping: f64,
}

/// A destroyed Humvee's tumble. Local axes: pitch across the chassis, roll along its length.
pub fn humvee_tumble(seed: f64, id: u32, deaths: u32) -> HumveeTumble {
    let hash = destruction_hash(to_int32(seed) ^ 0x51ed270b, id, deaths);
    let direction = if hash & 4 != 0 { 1.0 } else { -1.0 };
    let profiles = [
        HumveeTumble {
            height: 0.7,
            pitch: 0.0,
            roll: 5.5,
            yaw: 0.0,
            damping: 1.1,
        },
        HumveeTumble {
            height: 1.5,
            pitch: 0.6,
            roll: 6.0,
            yaw: 0.5,
            damping: 0.65,
        },
        HumveeTumble {
            height: 2.6,
            pitch: 4.7,
            roll: 0.7,
            yaw: 0.4,
            damping: 0.5,
        },
        HumveeTumble {
            height: 2.0,
            pitch: 2.8,
            roll: 3.8,
            yaw: 1.4,
            damping: 0.65,
        },
    ];
    let profile = profiles[hash as usize % profiles.len()];
    HumveeTumble {
        pitch: profile.pitch * direction,
        roll: profile.roll * direction,
        yaw: profile.yaw * direction,
        ..profile
    }
}
