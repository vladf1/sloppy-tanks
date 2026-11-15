//! The single-storey office and entry before each of Senda's homes, as covers of their
//! own: the room projecting from the half of the front away from the garage and the
//! porch bay beside it, which a tank cannot drive through and which break on their own,
//! leaving the house standing (the two-storey box is the house cover, one structure body
//! with no parts to spare). Each is drawn from its house's model (`Entry__<Surface>`,
//! `models::imported_house`), mirrored and stretched as that house is, and placed from the
//! box the model's script wrote into its plan ([`EntryBox`]).

use glam::{DVec2, DVec3};

use super::arena::CoverDef;
use super::house_layout::{EntryBox, HouseDesign, house_model};
use super::kinds::CoverKind;
use super::senda_layout::SendaHouse;

/// An entry's colour as a cover: its look's mark (its paint and tile are its house's
/// style's) and the stucco its rubble falls in, which no other cover wears.
pub const ENTRY_COLOR: u32 = 0xd3bf9d;
/// How much (hp) an entry takes to bring down: more than a loggia, a closed room.
const ENTRY_HEALTH: f64 = 200.0;

/// Whether a concrete cover is an entry, by its colour.
pub fn is_entry(color: u32) -> bool {
    color == ENTRY_COLOR
}

/// Where a house's entry stands: its box's ground centre in the world, its width across
/// the house's front, its depth out from it and its height (m). The box is its model's,
/// fitted to the lot as the house's model is; its frame is the house's street frame, the
/// turn the house's plan gives a plain cover (`arena::cover_turn`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntryPlace {
    pub at: DVec2,
    pub w: f64,
    pub d: f64,
    pub h: f64,
}

/// The entry before `house`, when its design's model has one.
pub fn entry_place(house: &SendaHouse) -> Option<EntryPlace> {
    let place = design_entry(&house.design())?;
    Some(EntryPlace {
        at: place.at + DVec2::new(house.x, house.z),
        ..place
    })
}

/// The entry before a house of `design`, its centre measured from the house's.
pub fn design_entry(design: &HouseDesign) -> Option<EntryPlace> {
    let (model, fit) = house_model(design)?;
    let EntryBox { x, z, w, d, h } = model.authored().entry?;
    let centre = design.to_world(fit.point(DVec3::new(x, 0.0, z)));
    let (along_x, along_z) = (w * fit.scale_x, d * fit.scale_z);
    // A house whose door is in its eave wall is built a quarter turn round: its street
    // front runs along the build frame's z.
    let (across, out) = if design.door_wall == 0 {
        (along_x, along_z)
    } else {
        (along_z, along_x)
    };
    let mm = |v: f64| (v * 1000.0).round() / 1000.0;
    Some(EntryPlace {
        at: DVec2::new(centre.x, centre.z),
        w: mm(across),
        d: mm(out),
        h: mm(fit.point(DVec3::new(0.0, h, 0.0)).y),
    })
}

/// Every house's entry as a destructible concrete cover, turned square to its house's
/// front by the house's plan, which it carries for its style, its garage's side and its
/// turn.
pub fn entry_covers(houses: &[SendaHouse]) -> Vec<CoverDef> {
    houses
        .iter()
        .filter_map(|house| {
            let place = entry_place(house)?;
            Some(CoverDef {
                house_plan: Some(house.plan),
                ..CoverDef::new(
                    CoverKind::Concrete,
                    (place.at.x * 100.0).round() / 100.0,
                    (place.at.y * 100.0).round() / 100.0,
                    place.w,
                    place.d,
                    place.h,
                    ENTRY_HEALTH,
                    ENTRY_COLOR,
                )
            })
        })
        .collect()
}
