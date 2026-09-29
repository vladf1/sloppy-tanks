//! Harbor Havoc: rotationally balanced lanes; outer deployment strips and shared pickups
//! stay clear.

use super::arena::CoverDef;
use super::data::ARENA;
use super::types::CoverKind;

pub fn harbor_layout() -> Vec<CoverDef> {
    let mut covers = Vec::new();
    let cover = CoverDef::new;
    let infinite = f64::INFINITY;
    for side in [-1.0, 1.0] {
        covers.push(cover(CoverKind::Boundary, side * (ARENA + 0.5), 0.0, 1.0, ARENA * 2.0 + 2.0, 1.2, infinite, 0x879698));
        covers.push(cover(CoverKind::Boundary, 0.0, side * (ARENA + 0.5), ARENA * 2.0 + 2.0, 1.0, 1.2, infinite, 0x879698));
        for z in [-32.0, -12.0, 12.0, 32.0] {
            let color = if z < 0.0 { 0xd37c38 } else { 0x31958d };
            covers.push(cover(CoverKind::Container, side * 30.0, z, 6.0, 14.0, 3.6, infinite, color));
        }
        for z in [-8.0, 8.0] {
            covers.push(cover(CoverKind::Container, side * 13.0, z, 12.0, 5.0, 3.6, infinite, 0x6689ad));
        }
        // Two individually breakable crates plug each shortcut between container rows.
        for z in [-22.0, 22.0] {
            for x in [28.4, 31.6] {
                covers.push(cover(CoverKind::Cargo, side * x, z, 3.1, 3.0, 2.6, 90.0, 0xb88b53));
            }
            covers.push(cover(CoverKind::Drum, side * 24.0, z, 1.2, 1.2, 1.7, 30.0, 0xff5b24));
        }
        for x in [10.0, 34.0] {
            covers.push(cover(CoverKind::Concrete, side * x, side * 55.0, 8.0, 1.1, 1.5, infinite, 0xb5b5a5));
        }
        for x in [-4.0, 4.0] {
            covers.push(cover(CoverKind::Cargo, x, side * 20.0, 3.0, 3.0, 2.6, 90.0, 0xb88b53));
        }
        covers.push(cover(CoverKind::Concrete, side * 43.0, 0.0, 1.2, 10.0, 1.7, infinite, 0xb5b5a5));
    }
    covers
}
