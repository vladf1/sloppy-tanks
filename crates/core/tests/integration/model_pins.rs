//! Pins every value the model builders produce: the three themes' scenery, the
//! vehicles and their wrecks, and each map's covers at every damage stage, batched
//! as presentation draws them. A hash covers each node's name, transform and
//! visibility, and each drawable's material, flags, instances and mesh (positions,
//! normals, UVs, colours, indices and custom attributes, bit for bit), so a
//! construction speed-up that moves one vertex, reorders one triangle or drops one
//! node fails here. Update a pin only for an intentional change to a model.

use sloppy_core::models::MapTheme;
use sloppy_core::sim::map_options::MapId;

use crate::model_pin_hash::{covers_pin, scenery_pin, vehicles_pin};

/// A pin for the build profile: native debug and release builds of the same
/// sources differ in the last bits of the village and quarry scenery and the
/// village's covers (already at 4ba8be26, before any of these speed-ups).
fn profile_pin(debug: u64, release: u64) -> u64 {
    if cfg!(debug_assertions) {
        debug
    } else {
        release
    }
}

fn assert_pin(what: &str, actual: u64, expected: u64) {
    assert_eq!(
        actual, expected,
        "{what} changed: pin {actual:#018x}, expected {expected:#018x}"
    );
}

#[test]
fn village_scenery_is_pinned() {
    assert_pin(
        "village scenery",
        scenery_pin(MapTheme::Village),
        profile_pin(0x0c52_66a4_1d6d_b740, 0x7901_2edf_0bc1_2087),
    );
}

#[test]
fn harbor_scenery_is_pinned() {
    assert_pin(
        "harbor scenery",
        scenery_pin(MapTheme::Harbor),
        0x2579_221d_91e9_5548,
    );
}

#[test]
fn quarry_scenery_is_pinned() {
    assert_pin(
        "quarry scenery",
        scenery_pin(MapTheme::Quarry),
        profile_pin(0x6f4e_b490_6e1a_2a5c, 0xd801_da82_bd8f_1a73),
    );
}

#[test]
fn vehicles_and_wrecks_are_pinned() {
    assert_pin("vehicles and wrecks", vehicles_pin(), 0x5eb7_8e3b_29af_69ca);
}

#[test]
fn village_covers_are_pinned() {
    assert_pin(
        "village covers",
        covers_pin(MapId::Village),
        profile_pin(0xdf29_d15a_f4a0_ae39, 0x9511_c809_8f19_b1e4),
    );
}

#[test]
fn harbor_covers_are_pinned() {
    assert_pin(
        "harbor covers",
        covers_pin(MapId::Harbor),
        0xb197_b799_92b0_3f77,
    );
}

#[test]
fn quarry_covers_are_pinned() {
    assert_pin(
        "quarry covers",
        covers_pin(MapId::Quarry),
        0x5e0f_0aa7_0303_a172,
    );
}
