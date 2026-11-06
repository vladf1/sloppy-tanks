//! What the page builds a cottage's looks from, shared between the builds that need
//! it at once instead of each making its own.
//!
//! A cottage's layout ([`house_structure`]) takes a weak device a few milliseconds to
//! lay out, and its finished pieces several more; a rebuild of its look and every body
//! of it that falls in a frame each made both again. The layout is shared while any
//! build holds it (a stepwise rebuild holds it over its frames), and the pieces are
//! built once a frame for all the cottage's bodies, then dropped: the bodies' models
//! hold what they draw, so no CPU copy of a mesh outlives the frame.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use sloppy_core::models::{CoverShape, HouseDebris, structure_debris};
use sloppy_core::scene::Node;
use sloppy_core::sim::CoverKind;
use sloppy_core::sim::house_layout::HouseDesign;
use sloppy_core::sim::house_structure::house_structure;
use sloppy_core::sim::structure::{PartSet, Structure};

/// A fallen body's source: the structure it fell from (cover `cover`, shaped `shape`)
/// and its parts, enough to draw it again ([`CottageLayouts::body`]).
#[derive(Clone, Debug)]
pub struct DebrisSource {
    pub cover: u32,
    pub shape: CoverShape,
    pub parts: PartSet,
}

#[derive(Default)]
pub struct CottageLayouts {
    /// Each cottage's layout by cover id, while a build holds it.
    layouts: HashMap<u32, Weak<Structure>>,
    /// This frame's cottages' pieces for their fallen bodies, by cover id.
    debris: Vec<(u32, HouseDebris)>,
}

impl CottageLayouts {
    /// The layout of cottage `cover`, shaped `shape`: laid out again only when no
    /// build holds it.
    pub fn layout(&mut self, cover: u32, shape: &CoverShape) -> Arc<Structure> {
        if let Some(layout) = self.layouts.get(&cover).and_then(Weak::upgrade) {
            return layout;
        }
        let design = HouseDesign::of(shape.x, shape.z, shape.w, shape.d, shape.h);
        let layout = Arc::new(house_structure(&design));
        self.layouts.insert(cover, Arc::downgrade(&layout));
        layout
    }

    /// The model of a structure's fallen body: a cottage's cut from its pieces, built
    /// once this frame for all its bodies.
    pub fn body(&mut self, source: &DebrisSource) -> Node {
        let shape = &source.shape;
        if shape.kind != CoverKind::House {
            return structure_debris(shape, &source.parts);
        }
        let index = match self.debris.iter().position(|(id, _)| *id == source.cover) {
            Some(index) => index,
            None => {
                let layout = self.layout(source.cover, shape);
                let pieces = HouseDebris::new(layout, shape.x, shape.z);
                self.debris.push((source.cover, pieces));
                self.debris.len() - 1
            }
        };
        self.debris[index].1.body(&source.parts)
    }

    /// Drop this frame's pieces.
    pub fn end_frame(&mut self) {
        self.debris.clear();
    }

    /// Forget the round's cottages.
    pub fn clear(&mut self) {
        self.layouts.clear();
        self.debris.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::house_debris;
    use sloppy_core::sim::render_state::RenderCover;

    #[test]
    fn a_layout_is_shared_while_held_and_bodies_match_their_own_builds() {
        let cover = RenderCover {
            id: 3,
            kind: CoverKind::House,
            x: 12.0,
            z: -8.0,
            w: 7.0,
            d: 9.0,
            h: 6.0,
            color: 0xa0_80_60,
            ..RenderCover::default()
        };
        let shape = CoverShape::from(&cover);
        let mut layouts = CottageLayouts::default();
        let held = layouts.layout(cover.id, &shape);
        assert!(Arc::ptr_eq(&held, &layouts.layout(cover.id, &shape)));
        let parts = held.parts.len();
        for set in [
            PartSet::from_indices([0]),
            PartSet::from_indices((0..parts).filter(|i| i % 5 == 0)),
        ] {
            let source = DebrisSource {
                cover: cover.id,
                shape: shape.clone(),
                parts: set,
            };
            let shared = layouts.body(&source);
            let own = house_debris(cover.x, cover.z, cover.w, cover.d, cover.h, &set);
            assert_eq!(format!("{shared:?}"), format!("{own:?}"));
        }
        assert_eq!(layouts.debris.len(), 1, "one set of pieces for the frame");
        layouts.end_frame();
        assert!(layouts.debris.is_empty());
        drop(held);
        assert!(
            layouts.layouts[&cover.id].upgrade().is_none(),
            "no layout is kept once no build holds it"
        );
    }
}
