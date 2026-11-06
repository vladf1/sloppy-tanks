//! Pins every value the model builders produce: the three themes' scenery, the
//! vehicles and their wrecks, and each map's covers as they stand, batched as
//! presentation draws them. A hash covers each node's name, transform and
//! visibility, and each drawable's material, flags, instances and mesh (positions,
//! normals, UVs, colours, indices and custom attributes, bit for bit), so a
//! construction speed-up that moves one vertex, reorders one triangle or drops one
//! node fails here. Update a pin only for an intentional change to a model.

use sloppy_core::models::MapTheme;
use sloppy_core::sim::map_options::MapId;

use crate::model_pin_hash::{covers_pin, scenery_pin, vehicles_pin};

/// Where a model's pin was recorded: an operating system and architecture
/// (`std::env::consts`) and whether it was a debug build.
type Platform = (&'static str, &'static str, bool);

const MAC_DEBUG: Platform = ("macos", "aarch64", true);
const MAC_RELEASE: Platform = ("macos", "aarch64", false);
const LINUX_DEBUG: Platform = ("linux", "x86_64", true);

/// The pin recorded for this platform and build, if any. Scenery that turns angles
/// takes `f64::sin` and its kin from the system's math library natively, whose last
/// bits differ between macOS and Linux, and between debug and release builds of the
/// same sources (already at 4ba8be26, before any of these speed-ups). CI pins Linux
/// debug; a platform with no pin reports its hash rather than failing.
fn platform_pin(pins: &[(Platform, u64)]) -> Option<u64> {
    let here = (
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(debug_assertions),
    );
    pins.iter()
        .find(|(platform, _)| *platform == here)
        .map(|(_, pin)| *pin)
}

fn assert_pin(what: &str, actual: u64, expected: impl Into<Option<u64>>) {
    let Some(expected) = expected.into() else {
        eprintln!("{what}: no pin for this platform; it hashes to {actual:#018x}");
        return;
    };
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
        platform_pin(&[
            (MAC_DEBUG, 0x0334_8941_bbef_7cab),
            (MAC_RELEASE, 0x2b03_15a7_54c9_8b60),
            (LINUX_DEBUG, 0xd685_6b69_7366_4198),
        ]),
    );
}

#[test]
fn harbor_scenery_is_pinned() {
    assert_pin(
        "harbor scenery",
        scenery_pin(MapTheme::Harbor),
        platform_pin(&[
            (MAC_DEBUG, 0x2579_221d_91e9_5548),
            (MAC_RELEASE, 0x2579_221d_91e9_5548),
            (LINUX_DEBUG, 0x74b5_1f76_a06d_eec8),
        ]),
    );
}

#[test]
fn quarry_scenery_is_pinned() {
    assert_pin(
        "quarry scenery",
        scenery_pin(MapTheme::Quarry),
        platform_pin(&[
            (MAC_DEBUG, 0x6f4e_b490_6e1a_2a5c),
            (MAC_RELEASE, 0xd801_da82_bd8f_1a73),
            (LINUX_DEBUG, 0xc802_df47_24a5_af6b),
        ]),
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
        platform_pin(&[
            (MAC_DEBUG, 0xacd6_d30a_7238_94b4),
            (MAC_RELEASE, 0x9b4a_ece5_ec80_379f),
            (LINUX_DEBUG, 0x3e2f_44b6_ebd8_1d83),
        ]),
    );
}

#[test]
fn harbor_covers_are_pinned() {
    assert_pin(
        "harbor covers",
        covers_pin(MapId::Harbor),
        0x1ff0_772c_04b3_9139,
    );
}

#[test]
fn quarry_covers_are_pinned() {
    assert_pin(
        "quarry covers",
        covers_pin(MapId::Quarry),
        0x73c0_51c3_3360_a09e,
    );
}
