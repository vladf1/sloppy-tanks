//! The runtime files of `public/` embedded in the binary (`build.rs`), by their path
//! under `public/` (`textures/ground/village-ground.webp`).

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The embedded file at `path`.
pub fn file(path: &str) -> Option<&'static [u8]> {
    FILES
        .binary_search_by(|(key, _)| (*key).cmp(path))
        .ok()
        .map(|index| FILES[index].1)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sloppy_core::models::{
        CoverShape, TreeDetail, WreckPart, build_scenery, cover_damage_stage, cover_model,
        create_arena_floor, create_spawn_pads, flags_model, node_textures, pickup_board,
        pickup_cube, pickup_pallet, pickup_parcel, tank_model, wreck_model,
    };
    use sloppy_core::scene::{Node, TextureSource};
    use sloppy_core::sim::arena::CoverDef;
    use sloppy_core::sim::data::ARENA;
    use sloppy_core::sim::extra_levels::extra_level;
    use sloppy_core::sim::house_layout::{HOUSE_HEAP, HouseDesign, heap_seed};
    use sloppy_core::sim::map_options::MAP_OPTIONS;
    use sloppy_core::sim::maps::{ArenaMap, selected_map};
    use sloppy_core::sim::tower_layout::TOWER_BASE;
    use sloppy_core::sim::{CoverKind, PickupKind, Team, VehicleKind};

    /// Every map Battle Setup offers, the extra levels' own arenas included.
    fn arenas() -> impl Iterator<Item = &'static ArenaMap> {
        MAP_OPTIONS.iter().map(|option| {
            let level = extra_level(option.id).and_then(|level| level.custom_map.flatten());
            selected_map(option.id, level)
        })
    }

    fn shape(cover: &CoverDef) -> CoverShape {
        CoverShape {
            kind: cover.kind,
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
            h: cover.h,
            color: cover.color,
            debris_seed: cover.debris_seed,
            timber_hits: Vec::new(),
            timber_join: cover.timber_join,
        }
    }

    /// The looks a cover shows from full health down to none.
    fn damage_stages(cover: &CoverDef) -> BTreeSet<u32> {
        if cover.hp.is_infinite() {
            return BTreeSet::from([0]);
        }
        (0..=100)
            .map(|percent| cover_damage_stage(cover.kind, f64::from(percent), 100.0))
            .collect()
    }

    /// The rubble a collapse leaves standing where the cover was, as the simulation
    /// lays it (`Simulation::damage_cover`): a cottage's heap, whose seed carries the
    /// cottage's design, or a tower's two foundations.
    fn rubble(cover: &CoverDef) -> Vec<CoverShape> {
        if cover.hp.is_infinite() {
            return Vec::new();
        }
        let pile = |x, z, w, d, h, seed| CoverShape {
            kind: CoverKind::Rubble,
            x,
            z,
            w,
            d,
            h,
            color: cover.color,
            debris_seed: Some(seed),
            timber_hits: Vec::new(),
            timber_join: None,
        };
        // The match's random draw only scatters the pieces, never what they are made of.
        let scatter = 0.5;
        match cover.kind {
            CoverKind::House => {
                let design = HouseDesign::of(cover.x, cover.z, cover.w, cover.d, cover.h);
                let (offset, size) = design.heap_box();
                let (x, z) = (cover.x + offset.x, cover.z + offset.y);
                let seed = heap_seed(scatter, &design);
                vec![pile(x, z, size.x, size.y, HOUSE_HEAP.height, seed)]
            }
            CoverKind::Tower => [-1.0, 1.0]
                .map(|side| {
                    let x = cover.x + side * TOWER_BASE.offset;
                    let (w, d, h) = (TOWER_BASE.width, TOWER_BASE.depth, TOWER_BASE.rubble_height);
                    pile(x, cover.z, w, d, h, scatter)
                })
                .to_vec(),
            _ => Vec::new(),
        }
    }

    /// Natively the renderer looks a `TextureSource::File` path up in this table as it
    /// stands (`sloppy_render::gpu::set_asset_files`), so a texture `build.rs` leaves
    /// out would only show as a blank surface in the running app. The walk covers each
    /// arena's scenery, floor and cover (every damage look and the rubble it leaves),
    /// every vehicle and its wreck, the supplies and the flags.
    #[test]
    fn every_texture_the_arenas_name_is_embedded() {
        let mut files = BTreeSet::new();
        let mut add = |root: &Node| {
            for source in node_textures(root) {
                if let TextureSource::File(path) = source {
                    files.insert(path);
                }
            }
        };
        for arena in arenas() {
            match arena.theme {
                Some(theme) => add(build_scenery(theme).root()),
                None => add(&create_spawn_pads(arena.scale.unwrap_or(1.0))),
            }
            for ground in [arena.floor, arena.outer_floor].into_iter().flatten() {
                add(&create_arena_floor(ground, ARENA * 2.0, 0.0));
            }
            for cover in (arena.layout)() {
                for stage in damage_stages(&cover) {
                    add(&cover_model(&shape(&cover), TreeDetail::Full, stage));
                }
                for pile in rubble(&cover) {
                    add(&cover_model(&pile, TreeDetail::Full, 0));
                }
            }
        }
        for kind in VehicleKind::ALL {
            for team in [Team::Blue, Team::Red] {
                add(&tank_model(kind, team));
                for part in [
                    WreckPart::Intact,
                    WreckPart::Hull,
                    WreckPart::Turret,
                    WreckPart::TurretBarrel,
                    WreckPart::Barrel,
                ] {
                    add(&wreck_model(kind, team, part));
                }
            }
        }
        for kind in PickupKind::ALL {
            add(&pickup_cube(kind));
            add(&pickup_parcel(kind));
        }
        add(&pickup_pallet());
        add(&pickup_board());
        add(&flags_model());

        assert!(!files.is_empty(), "the models name file textures");
        for path in files {
            assert!(super::file(path).is_some(), "{path} is not embedded");
        }
        assert!(super::file("textures/missing.webp").is_none());
    }
}
