//! What the page builds a cottage's looks from, shared between the builds that need
//! it at once instead of each making its own.
//!
//! A cottage's layout ([`shared_house_structure`]) takes a weak device a few milliseconds to
//! lay out, and its finished pieces several more; a rebuild of its look and every body
//! of it that falls in a frame each made both again. The layout is kept for the round
//! once a cottage is first hit: every later hit rebuilds its look from it, and laying it
//! out again each time a rebuild had let it go cost the village's rounds two out of
//! three of their layouts (half a millisecond each natively). It holds no mesh, only the
//! parts' shapes, and is the simulation's own copy wherever that still stands.
//!
//! A cottage's pieces (a millisecond natively) are kept a few seconds after the last
//! body cut from them, for the [`PIECES_KEPT`] latest cottages: a collapse drops its
//! bodies over a second or two, and its pile draws them again whenever a merged chunk
//! breaks up as they settle and fade, in bursts seconds apart; building the pieces
//! afresh each such frame was most of a collapsing cottage's cost. They are the
//! source the bodies are cut and batched from, never what the GPU draws.

use crate::fixed_hash::HashMap;
use std::sync::Arc;

use sloppy_core::models::{CoverShape, HouseDebris, structure_debris};
use sloppy_core::scene::Node;
use sloppy_core::sim::CoverKind;
use sloppy_core::sim::house_layout::HouseDesign;
use sloppy_core::sim::house_structure::shared_house_structure;
use sloppy_core::sim::structure::{PartSet, Structure};

/// A fallen body's source: the structure it fell from (cover `cover`, shaped `shape`)
/// and its parts, enough to draw it again ([`CottageLayouts::body`]).
#[derive(Clone, Debug)]
pub struct DebrisSource {
    pub cover: u32,
    pub shape: CoverShape,
    pub parts: PartSet,
}

/// Seconds a cottage's pieces are kept after the last body cut from them.
const PIECES_KEPT_SECONDS: f64 = 3.0;
/// Cottages whose pieces are kept beyond the frame that used them (about 0.6 MB each
/// natively).
const PIECES_KEPT: usize = 1;

/// A cottage's pieces for its fallen bodies, and when a body was last cut from them
/// (presentation seconds).
struct KeptPieces {
    cover: u32,
    pieces: HouseDebris,
    used: f64,
}

#[derive(Default)]
pub struct CottageLayouts {
    /// Each hit cottage's layout by cover id, for the round.
    layouts: HashMap<u32, Arc<Structure>>,
    /// Recently collapsing cottages' pieces, least recently used first.
    debris: Vec<KeptPieces>,
}

impl CottageLayouts {
    /// The layout of cottage `cover`, shaped `shape`, laid out on its first use in the
    /// round.
    pub fn layout(&mut self, cover: u32, shape: &CoverShape) -> Arc<Structure> {
        self.layouts
            .entry(cover)
            .or_insert_with(|| {
                let design = HouseDesign::of(shape.x, shape.z, shape.w, shape.d, shape.h);
                shared_house_structure(&design)
            })
            .clone()
    }

    /// The model of a structure's fallen body: a cottage's cut from its pieces, built
    /// once for all its bodies falling or merging within a few seconds of each other
    /// (`now`, presentation seconds).
    pub fn body(&mut self, source: &DebrisSource, now: f64) -> Node {
        let shape = &source.shape;
        if shape.kind != CoverKind::House {
            return structure_debris(shape, &source.parts);
        }
        let kept = match self
            .debris
            .iter()
            .position(|kept| kept.cover == source.cover)
        {
            Some(index) => {
                let mut kept = self.debris.remove(index);
                kept.used = now;
                kept
            }
            None => {
                let layout = self.layout(source.cover, shape);
                KeptPieces {
                    cover: source.cover,
                    pieces: HouseDebris::new(layout, shape.x, shape.z),
                    used: now,
                }
            }
        };
        let body = kept.pieces.body(&source.parts);
        self.debris.push(kept);
        body
    }

    /// Drop the pieces no body was cut from for [`PIECES_KEPT_SECONDS`], and the least
    /// recently used beyond [`PIECES_KEPT`].
    pub fn end_frame(&mut self, now: f64) {
        self.debris
            .retain(|kept| now - kept.used < PIECES_KEPT_SECONDS);
        let excess = self.debris.len().saturating_sub(PIECES_KEPT);
        self.debris.drain(..excess);
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
    fn a_layout_is_kept_for_the_round_and_bodies_match_their_own_builds() {
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
            let shared = layouts.body(&source, 0.0);
            let own = house_debris(cover.x, cover.z, cover.w, cover.d, cover.h, &set);
            assert_eq!(format!("{shared:?}"), format!("{own:?}"));
        }
        assert_eq!(layouts.debris.len(), 1, "one set of pieces for its bodies");
        layouts.end_frame(PIECES_KEPT_SECONDS - 0.5);
        assert_eq!(
            layouts.debris.len(),
            1,
            "kept while its bodies fall and settle"
        );
        layouts.end_frame(PIECES_KEPT_SECONDS);
        assert!(
            layouts.debris.is_empty(),
            "dropped once none was cut for a while"
        );
        let other = DebrisSource {
            cover: 4,
            shape: CoverShape::from(&RenderCover {
                id: 4,
                x: -20.0,
                ..cover.clone()
            }),
            parts: PartSet::from_indices([0]),
        };
        for source in [
            &other,
            &DebrisSource {
                cover: cover.id,
                shape: shape.clone(),
                parts: PartSet::from_indices([1]),
            },
        ] {
            layouts.body(source, 1.0);
        }
        layouts.end_frame(1.0);
        assert_eq!(
            layouts.debris.len(),
            PIECES_KEPT,
            "the latest cottages' pieces only"
        );
        assert_eq!(layouts.debris.last().map(|kept| kept.cover), Some(cover.id));
        let first = Arc::downgrade(&held);
        drop(held);
        assert!(
            Arc::ptr_eq(&layouts.layout(cover.id, &shape), &first.upgrade().unwrap()),
            "the round keeps the layout once no build holds it"
        );
        layouts.clear();
        assert!(first.upgrade().is_none(), "a new round lets it go");
    }
}
