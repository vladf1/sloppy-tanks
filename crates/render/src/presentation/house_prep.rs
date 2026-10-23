//! A damaged cottage's collapse pieces made ahead of its fall.
//!
//! A cottage's pieces take a slow device about a second to cut and upload, far too
//! long for the frame it falls on. Its shell hits stop changing once a shot fells it
//! (the felling shot leaves no mark), so the pieces cut for its current hits are the
//! ones it falls into. Presentation cuts them, and uploads their models, a few
//! milliseconds a frame for the most damaged standing cottage; a new hit starts that
//! cottage over, and one that falls before its pieces are ready cuts the rest at
//! once. One cottage at a time, since its pieces hold megabytes of geometry.
//! Presentation only: the simulation breaks the cottage the same way regardless.

use std::collections::HashMap;

use sloppy_core::models::{HousePieces, PieceStep, cover_damage_stage};
use sloppy_core::scene::Node;
use sloppy_core::sim::CoverKind;
use sloppy_core::sim::house_layout::HousePiece;
use sloppy_core::sim::render_state::{RenderCover, RenderFragment};
use sloppy_core::sim::timber_layout::TimberHit;

/// Milliseconds of each frame the preparation may take, and the most steps it takes
/// in one (the page clock reads nothing in native tests). A step takes about 0.3 ms
/// on a fast desktop and up to 1 ms, so a cottage is ready in well under a second
/// there and in a few seconds on a slow device.
pub const PREPARE_BUDGET_MS: f64 = 2.0;
pub const PREPARE_STEPS: usize = 24;
/// Seconds a cottage's look must hold before its pieces are started: the frame it is
/// hit already rebuilds its standing model, and a cottage under fire would start
/// over with each shell.
const PREPARE_DELAY: f64 = 0.3;
/// The damage stage from which a cottage's pieces are made ahead: wrecked, a few
/// shells from falling. Lighter damage would keep a cottage's worth of cutting
/// going in the background for cottages that may never fall.
const PREPARE_STAGE: u32 = 2;

/// A collapse piece made ahead: its node and, once uploaded, its model `M`.
pub struct Prepared<M> {
    pub piece: HousePiece,
    pub node: Node,
    pub model: Option<M>,
}

/// What a cottage's pieces are cut from: which cottage, its box and its shell hits.
#[derive(Clone, Debug, PartialEq)]
struct HouseKey {
    id: u32,
    place: [f64; 5],
    hits: Vec<TimberHit>,
}

impl HouseKey {
    fn of(cover: &RenderCover) -> Self {
        Self {
            id: cover.id,
            place: place(cover),
            hits: cover.timber_hits.clone(),
        }
    }

    /// Whether `cover` still breaks into the pieces cut for this key.
    fn fits(&self, cover: &RenderCover) -> bool {
        self.id == cover.id && self.place == place(cover) && self.hits == cover.timber_hits
    }
}

fn place(cover: &RenderCover) -> [f64; 5] {
    [cover.x, cover.z, cover.w, cover.d, cover.h]
}

struct Target<M> {
    key: HouseKey,
    /// When its look last changed (presentation seconds).
    since: f64,
    pieces: HousePieces,
    cut: bool,
    prepared: Vec<Prepared<M>>,
}

pub struct HousePreparation<M> {
    target: Option<Target<M>>,
}

impl<M> Default for HousePreparation<M> {
    fn default() -> Self {
        Self { target: None }
    }
}

impl<M> HousePreparation<M> {
    /// Aim at the most damaged standing cottage among `covers` (none while every one
    /// is whole) at presentation time `now`. Models uploaded for another cottage, or
    /// for this one before its latest hit, are handed back to free.
    pub fn aim(&mut self, covers: &[RenderCover], now: f64) -> Vec<M> {
        let candidate = covers
            .iter()
            .filter(|cover| {
                cover.kind == CoverKind::House
                    && cover.alive
                    && cover_damage_stage(cover.kind, cover.hp, cover.max_hp) >= PREPARE_STAGE
            })
            .min_by(|a, b| {
                (a.hp / a.max_hp)
                    .total_cmp(&(b.hp / b.max_hp))
                    .then(a.id.cmp(&b.id))
            });
        if let (Some(target), Some(cover)) = (&self.target, candidate)
            && target.key.fits(cover)
        {
            return Vec::new();
        }
        let freed = self.clear();
        self.target = candidate.map(|cover| Target {
            key: HouseKey::of(cover),
            since: now,
            pieces: HousePieces::new(
                cover.x,
                cover.z,
                cover.w,
                cover.d,
                cover.h,
                &cover.timber_hits,
            ),
            cut: false,
            prepared: Vec::new(),
        });
        freed
    }

    /// Do the next bit of work at time `now`: a step of cutting, then uploading each
    /// piece as it is finished (`upload` makes its model). False while there is
    /// nothing to do yet, or any more.
    pub fn step(&mut self, now: f64, upload: impl FnOnce(&Node) -> M) -> bool {
        let Some(target) = &mut self.target else {
            return false;
        };
        if target.cut || now - target.since < PREPARE_DELAY {
            return false;
        }
        match target.pieces.step() {
            PieceStep::Cut => {}
            PieceStep::Piece(piece, node) => {
                let model = Some(upload(&node));
                target.prepared.push(Prepared { piece, node, model });
            }
            PieceStep::Done => target.cut = true,
        }
        true
    }

    /// The pieces `cover` fell into, if they were being made for it as it fell:
    /// every piece, those not made yet cut now (without models). The preparation
    /// starts over.
    pub fn take(&mut self, cover: &RenderCover) -> Option<Vec<Prepared<M>>> {
        if !self.target.as_ref()?.key.fits(cover) {
            return None;
        }
        let Target {
            pieces, prepared, ..
        } = self.target.take()?;
        let mut prepared = prepared;
        prepared.extend(pieces.map(|(piece, node)| Prepared {
            piece,
            node,
            model: None,
        }));
        Some(prepared)
    }

    /// Forget the cottage being prepared, handing back its models to free.
    pub fn clear(&mut self) -> Vec<M> {
        self.target
            .take()
            .map(|target| {
                target
                    .prepared
                    .into_iter()
                    .filter_map(|prepared| prepared.model)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// How many of the target's pieces are ready (for diagnostics).
    pub fn ready(&self) -> usize {
        self.target
            .as_ref()
            .map_or(0, |target| target.prepared.len())
    }
}

/// Drop the piece sets of cottages none of whose pieces is on the field any more,
/// handing each unshown piece's model to `release`. A collapse takes its cottage's
/// whole set when its first piece appears, but presentation can see only part of it
/// (a fragment budget spent between two frames, a room joined mid-collapse), and the
/// pieces it never shows would otherwise keep their geometry and models all round.
pub fn release_spent_houses<M>(
    sets: &mut HashMap<u32, Vec<Prepared<M>>>,
    fragments: &[RenderFragment],
    mut release: impl FnMut(M),
) {
    if sets.is_empty() {
        return;
    }
    sets.retain(|house, pieces| {
        let shown = fragments
            .iter()
            .any(|fragment| fragment.house_piece.is_some() && fragment.house_id == Some(*house));
        if !shown {
            pieces
                .drain(..)
                .filter_map(|prepared| prepared.model)
                .for_each(&mut release);
        }
        shown
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::house_pieces;

    #[test]
    fn piece_sets_go_with_their_collapse() {
        use sloppy_core::sim::house_layout::HousePart;
        let wall = HousePiece {
            part: HousePart::FrontWall,
            shard: 0,
        };
        let set = |models: &[usize]| -> Vec<Prepared<usize>> {
            models
                .iter()
                .map(|&model| Prepared {
                    piece: wall,
                    node: Node::default(),
                    model: Some(model),
                })
                .collect()
        };
        let mut sets = HashMap::from([(1, set(&[10, 11])), (2, set(&[20]))]);
        let piece_of = |house: u32| RenderFragment {
            house_piece: Some(wall),
            house_id: Some(house),
            ..RenderFragment::default()
        };
        // Cottage 2's last shown piece is gone; cottage 1 still has one on the field.
        let mut released = Vec::new();
        release_spent_houses(&mut sets, &[piece_of(1)], |model| released.push(model));
        assert_eq!(released, [20]);
        assert!(sets.contains_key(&1) && !sets.contains_key(&2));
        // Other debris does not keep a cottage's set.
        let other = RenderFragment {
            house_id: Some(1),
            ..RenderFragment::default()
        };
        release_spent_houses(&mut sets, &[other], |model| released.push(model));
        released.sort_unstable();
        assert_eq!(released, [10, 11, 20]);
        assert!(sets.is_empty());
    }

    fn cottage(id: u32, hp: f64) -> RenderCover {
        RenderCover {
            id,
            kind: CoverKind::House,
            x: 10.0,
            z: -4.0,
            w: 7.0,
            h: 5.5,
            d: 6.0,
            hp,
            max_hp: 600.0,
            alive: true,
            destructible: true,
            ..RenderCover::default()
        }
    }

    #[test]
    fn the_most_damaged_cottage_falls_into_the_pieces_made_for_it() {
        let mut preparation: HousePreparation<usize> = HousePreparation::default();
        // A cottage merely damaged is left alone.
        assert!(preparation.aim(&[cottage(4, 300.0)], 0.0).is_empty());
        assert!(!preparation.step(1.0, |_| 0));
        let mut covers = vec![cottage(1, 260.0), cottage(2, 200.0), cottage(3, 600.0)];
        covers[1].timber_hits.push(TimberHit {
            x: 0.5,
            y: 1.2,
            z: 3.0,
            size: 1.0,
        });
        assert!(preparation.aim(&covers, 0.0).is_empty());
        // Nothing is cut until the cottage's look has held a moment.
        assert!(!preparation.step(0.1, |_| 0));
        let mut uploads = 0;
        for _ in 0..12 {
            preparation.step(1.0, |_| {
                uploads += 1;
                uploads
            });
        }
        // A cottage nobody prepared falls into freshly cut pieces.
        assert!(preparation.take(&covers[0]).is_none());
        // A new hit starts over and frees what was uploaded for the old look.
        covers[1].timber_hits.push(TimberHit {
            x: -1.0,
            y: 1.0,
            z: 3.0,
            size: 1.5,
        });
        assert_eq!(preparation.aim(&covers, 1.0).len(), uploads);
        for _ in 0..40 {
            preparation.step(2.0, |_| 0);
        }
        assert!(preparation.ready() > 0);
        covers[1].alive = false;
        let pieces = preparation
            .take(&covers[1])
            .expect("prepared for this cottage");
        let c = &covers[1];
        let fresh = house_pieces(c.x, c.z, c.w, c.d, c.h, &c.timber_hits);
        assert_eq!(pieces.len(), fresh.len());
        fn vertices(node: &Node, out: &mut Vec<[f32; 3]>) {
            if let Some(drawable) = &node.drawable {
                out.extend(&drawable.mesh.positions);
            }
            for child in &node.children {
                vertices(child, out);
            }
        }
        for (prepared, (piece, node)) in pieces.iter().zip(&fresh) {
            assert_eq!(prepared.piece, *piece);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            vertices(&prepared.node, &mut a);
            vertices(node, &mut b);
            assert_eq!(a, b, "{piece:?}");
        }
        assert!(pieces[0].model.is_some() && pieces.last().unwrap().model.is_none());
        assert_eq!(preparation.ready(), 0);
    }
}
