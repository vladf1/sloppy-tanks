//! The walkable tops of each theme's spawn pads, so ground decals (tread marks,
//! blast rings) and mines lying on a pad draw on its deck instead of under it.
//!
//! The pads are presentation scenery: tanks drive over them at ground height and
//! the simulation never sees them. This is the renderer's own footprint of those
//! models, one flat deck per visible tier, so a lookup only chooses a drawn
//! height; it never moves a mine, a mark or a blast in the simulation. The tiers
//! mirror `create_spawn_pads` (village and extra levels), the harbor pads in
//! `HarborScenery` and `quarry_spawn_pad_pieces`; the tests measure the real pad
//! meshes to keep them in step.

use std::f64::consts::{FRAC_PI_4, PI, TAU};

use sloppy_core::models::{SpawnPadShape, quarry_spawn_pad_pieces};
use sloppy_core::sim::arena::spawn_positions;
use sloppy_core::sim::{RenderState, Team, Vec2};

/// Ground decals on a pad sit this far above its deck: several depth-buffer steps
/// at the farthest overhead zoom (like the player ring's clearance), before their
/// polygon offset.
pub const PAD_DECAL_LIFT: f64 = 0.02;

/// Where a deck covers the ground, relative to the pad's spawn point.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Footprint {
    /// A cylinder cap with `sides` corners, the first on +Z (`CylinderGeometry`).
    Polygon { radius: f64, sides: u32 },
    /// A flat ring of paint.
    Annulus { inner: f64, outer: f64 },
    /// A box turned `yaw` about Y, `width` along its local X and `depth` along Z.
    Box { width: f64, depth: f64, yaw: f64 },
    /// A convex quad, corners in either winding.
    Quad([Vec2; 4]),
}

/// One flat tier of a pad model: its footprint centred at `offset` from the
/// spawn point and the height of its top surface.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Deck {
    offset: Vec2,
    footprint: Footprint,
    top: f64,
}

const fn deck(dx: f64, dz: f64, footprint: Footprint, top: f64) -> Deck {
    Deck {
        offset: Vec2 { x: dx, z: dz },
        footprint,
        top,
    }
}

impl Deck {
    /// The horizontal distance from the centre that bounds the footprint.
    fn reach(&self) -> f64 {
        let extent = match self.footprint {
            Footprint::Polygon { radius, .. } => radius,
            Footprint::Annulus { outer, .. } => outer,
            Footprint::Box { width, depth, .. } => width.hypot(depth) / 2.0,
            Footprint::Quad(corners) => corners.iter().map(|c| c.x.hypot(c.z)).fold(0.0, f64::max),
        };
        self.offset.x.hypot(self.offset.z) + extent
    }

    /// Whether the footprint, or any point within `margin` of it, covers the
    /// point `(dx, dz)` from its centre.
    fn covers(&self, dx: f64, dz: f64, margin: f64) -> bool {
        match self.footprint {
            Footprint::Polygon { radius, sides } => {
                let r = dx.hypot(dz);
                let sector = TAU / f64::from(sides);
                // Angle from the nearest side's midpoint, which lies half a sector
                // past each corner. The circle trims the grown corners.
                let angle = dx.atan2(dz).rem_euclid(sector) - sector / 2.0;
                r * angle.cos() <= radius * (PI / f64::from(sides)).cos() + margin
                    && r <= radius + margin
            }
            Footprint::Annulus { inner, outer } => {
                let r = dx.hypot(dz);
                r >= inner - margin && r <= outer + margin
            }
            Footprint::Box { width, depth, yaw } => {
                let (sin, cos) = yaw.sin_cos();
                let x = (dx * cos - dz * sin).abs() - width / 2.0;
                let z = (dx * sin + dz * cos).abs() - depth / 2.0;
                x.max(0.0).hypot(z.max(0.0)) <= margin
            }
            Footprint::Quad(corners) => {
                let edge = |i: usize| (corners[i], corners[(i + 1) % 4]);
                // The winding's sign puts the inside on the positive side of every edge.
                let winding = (0..4)
                    .map(|i| {
                        let (a, b) = edge(i);
                        a.x * b.z - b.x * a.z
                    })
                    .sum::<f64>()
                    .signum();
                let inside = (0..4).all(|i| {
                    let (a, b) = edge(i);
                    winding * ((b.x - a.x) * (dz - a.z) - (b.z - a.z) * (dx - a.x)) >= 0.0
                });
                inside
                    || (0..4).any(|i| {
                        let (a, b) = edge(i);
                        let (ex, ez) = (b.x - a.x, b.z - a.z);
                        let t = (((dx - a.x) * ex + (dz - a.z) * ez) / (ex * ex + ez * ez))
                            .clamp(0.0, 1.0);
                        (dx - a.x - t * ex).hypot(dz - a.z - t * ez) <= margin
                    })
            }
        }
    }
}

/// The village and extra-level plinth (`create_spawn_pads`): an octagonal base,
/// two stepped decks, the team rim and lights, the centre badge and grille, and
/// the two painted chevrons on the ground in front of it (added per team).
fn plinth_decks() -> Vec<Deck> {
    use Footprint::*;
    let mut decks = vec![
        deck(
            0.0,
            0.0,
            Polygon {
                radius: 2.75,
                sides: 8,
            },
            0.08 + 0.05,
        ),
        deck(
            0.0,
            0.0,
            Polygon {
                radius: 2.52,
                sides: 8,
            },
            0.135 + 0.0225,
        ),
        deck(
            0.0,
            0.0,
            Polygon {
                radius: 2.37,
                sides: 32,
            },
            0.17 + 0.0175,
        ),
        // The team rim is eight painted arcs; one ring covers them and their gaps.
        deck(
            0.0,
            0.0,
            Annulus {
                inner: 2.05,
                outer: 2.3,
            },
            0.198,
        ),
        deck(
            0.0,
            0.0,
            Polygon {
                radius: 1.98,
                sides: 8,
            },
            0.193 + 0.0125,
        ),
        deck(
            0.0,
            0.0,
            Box {
                width: 0.7,
                depth: 0.7,
                yaw: FRAC_PI_4,
            },
            0.22 + 0.0125,
        ),
    ];
    for i in 0..8 {
        let angle = f64::from(i) * FRAC_PI_4;
        let light = Polygon {
            radius: 0.075,
            sides: 8,
        };
        decks.push(deck(
            angle.cos() * 2.58,
            angle.sin() * 2.58,
            light,
            0.175 + 0.0125,
        ));
    }
    for dz in [-1.25, 1.25] {
        for i in 0..5 {
            let slot = Box {
                width: 0.18,
                depth: 0.4,
                yaw: 0.0,
            };
            decks.push(deck(-0.52 + f64::from(i) * 0.26, dz, slot, 0.218 + 0.01));
        }
    }
    decks
}

/// The plinth's painted ground chevrons (flat paint at 0.09 m), 3.1 and 3.8 m
/// toward the arena from the spawn point; each is two arms meeting at the tip.
fn plinth_chevrons(team: Team, decks: &mut Vec<Deck>) {
    const TOP: f64 = 0.09;
    // One arm of the arrow path in its own frame, tip toward +X: outer corner,
    // tip, inner tip, inner corner. The other arm mirrors it across X.
    const ARM: [(f64, f64); 4] = [(-0.28, 0.55), (-0.48, 0.37), (-0.1, 0.0), (0.28, 0.0)];
    let inward = if team == Team::Blue { 1.0 } else { -1.0 };
    for offset in [3.1, 3.8] {
        for mirror in [1.0, -1.0] {
            let corners = ARM.map(|(x, z)| Vec2 {
                x: inward * (offset + x),
                z: inward * mirror * z,
            });
            decks.push(deck(0.0, 0.0, Footprint::Quad(corners), TOP));
        }
    }
}

/// Harbor pads (`HarborScenery`): a twelve-sided slab and a flat team ring.
fn harbor_decks() -> Vec<Deck> {
    use Footprint::*;
    vec![
        deck(
            0.0,
            0.0,
            Polygon {
                radius: 2.65,
                sides: 12,
            },
            0.06 + 0.04,
        ),
        deck(
            0.0,
            0.0,
            Annulus {
                inner: 2.1,
                outer: 2.3,
            },
            0.11,
        ),
    ]
}

/// Quarry pads (`quarry_spawn_pad_pieces`): gravel discs, hazard dashes and the
/// team chevrons. The beacon post and cap stand up from the pad, not on it.
fn quarry_decks(team: Team) -> Vec<Deck> {
    quarry_spawn_pad_pieces(team)
        .into_iter()
        .filter_map(|piece| {
            let footprint = match piece.shape {
                SpawnPadShape::Disc => Footprint::Polygon {
                    radius: piece.w,
                    sides: 20,
                },
                SpawnPadShape::Dash | SpawnPadShape::Chevron => Footprint::Box {
                    width: piece.w,
                    depth: piece.d,
                    yaw: piece.rot_y,
                },
                SpawnPadShape::Post | SpawnPadShape::Cap => return None,
            };
            Some(deck(piece.dx, piece.dz, footprint, piece.y + piece.h / 2.0))
        })
        .collect()
}

/// One placed pad: its spawn point, its decks and the radius that bounds them.
#[derive(Clone, Debug, PartialEq)]
struct Pad {
    center: Vec2,
    reach: f64,
    decks: Vec<Deck>,
}

/// The spawn-pad decks of the current map, rebuilt when the theme or scale changes.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPadDecks {
    theme: String,
    scale: f64,
    pads: Vec<Pad>,
}

impl Default for SpawnPadDecks {
    fn default() -> Self {
        Self::new("village", 1.0)
    }
}

impl SpawnPadDecks {
    /// Pads of the theme's scenery: village and extra levels share the plinth
    /// (extra levels place it at their scale), the harbor and quarry have their own.
    pub fn new(theme: &str, scale: f64) -> Self {
        let mut pads = Vec::new();
        for team in [Team::Blue, Team::Red] {
            let decks = match theme {
                "harbor" => harbor_decks(),
                "quarry" => quarry_decks(team),
                _ => {
                    let mut decks = plinth_decks();
                    plinth_chevrons(team, &mut decks);
                    decks
                }
            };
            // Themed scenery places its pads for the standard arena.
            let placement = if matches!(theme, "village" | "harbor" | "quarry") {
                1.0
            } else {
                scale
            };
            let reach = decks.iter().map(Deck::reach).fold(0.0, f64::max);
            for center in spawn_positions(team, placement) {
                pads.push(Pad {
                    center,
                    reach,
                    decks: decks.clone(),
                });
            }
        }
        Self {
            theme: theme.to_owned(),
            scale,
            pads,
        }
    }

    /// Follow the rendered map; cheap when nothing changed.
    pub fn sync(&mut self, state: &RenderState) {
        if self.theme != state.map_theme || self.scale != state.map_scale {
            *self = Self::new(&state.map_theme, state.map_scale);
        }
    }

    /// The top of the highest pad deck within `radius` of `(x, z)`: what a mine
    /// of that radius, or a ring spreading that far, has to clear.
    pub fn top_within(&self, x: f64, z: f64, radius: f64) -> Option<f64> {
        let mut top: Option<f64> = None;
        for pad in &self.pads {
            let (dx, dz) = (x - pad.center.x, z - pad.center.z);
            if dx.abs() > pad.reach + radius || dz.abs() > pad.reach + radius {
                continue;
            }
            for deck in &pad.decks {
                if deck.covers(dx - deck.offset.x, dz - deck.offset.z, radius) {
                    top = Some(top.map_or(deck.top, |t| t.max(deck.top)));
                }
            }
        }
        top
    }

    /// The height of a ground decal at `(x, z)`: `ground` on open ground, or
    /// just above the highest deck within `radius` on a pad.
    pub fn decal_height(&self, x: f64, z: f64, radius: f64, ground: f64) -> f64 {
        self.top_within(x, z, radius)
            .map_or(ground, |top| ground.max(top + PAD_DECAL_LIFT))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use glam::{DMat4, DVec3};
    use sloppy_core::geometry::{box_geometry, cylinder_geometry, widen};
    use sloppy_core::models::{HarborScenery, create_spawn_pads};
    use sloppy_core::scene::{Material, Node};
    use std::sync::Arc;

    /// Pad decks are ankle-high; anything above is a post, a cap or a vessel.
    const DECK_CEILING: f64 = 0.3;

    pub(crate) type Triangle = [DVec3; 3];

    /// The triangles of `node` below the deck ceiling that reach into the square of
    /// half-size `extent` around `center`.
    pub(crate) fn triangles_near(node: &Node, center: Vec2, extent: f64) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        node.traverse(DMat4::IDENTITY, &mut |part, world| {
            let Some(drawable) = &part.drawable else {
                return;
            };
            let matrices: Vec<DMat4> = match &drawable.instances {
                None => vec![world],
                Some(instances) => instances.iter().map(|i| world * i.matrix).collect(),
            };
            let mesh = &drawable.mesh;
            let count = mesh.indices.as_ref().map_or(mesh.positions.len(), Vec::len);
            let index = |i: usize| mesh.indices.as_ref().map_or(i, |ix| ix[i] as usize);
            for matrix in matrices {
                for t in (0..count).step_by(3) {
                    let triangle = [0, 1, 2]
                        .map(|k| matrix.transform_point3(widen(mesh.positions[index(t + k)])));
                    let low = triangle.iter().all(|p| p.y < DECK_CEILING);
                    let near = |f: fn(&DVec3) -> f64, c: f64| {
                        let (min, max) = triangle
                            .iter()
                            .map(f)
                            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
                        max >= c - extent && min <= c + extent
                    };
                    if low && near(|p| p.x, center.x) && near(|p| p.z, center.z) {
                        triangles.push(triangle);
                    }
                }
            }
        });
        triangles
    }

    /// The highest triangle over `(x, z)`: the surface a decal there must clear.
    pub(crate) fn surface_top(triangles: &[Triangle], x: f64, z: f64) -> Option<f64> {
        triangles
            .iter()
            .filter_map(|&[a, b, c]| height_over(a, b, c, x, z))
            .fold(None, |top: Option<f64>, y| {
                Some(top.map_or(y, |t| t.max(y)))
            })
    }

    /// Height of a non-vertical triangle above `(x, z)`, if it covers that point.
    fn height_over(a: DVec3, b: DVec3, c: DVec3, x: f64, z: f64) -> Option<f64> {
        let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if det.abs() < 1e-12 {
            return None;
        }
        let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
        let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
        let w = 1.0 - u - v;
        (u >= 0.0 && v >= 0.0 && w >= 0.0).then_some(u * a.y + v * b.y + w * c.y)
    }

    fn quarry_pads() -> Node {
        let mut root = Node::group("quarry pads");
        let material = Arc::new(Material::default());
        for team in [Team::Blue, Team::Red] {
            for spawn in spawn_positions(team, 1.0) {
                for piece in quarry_spawn_pad_pieces(team) {
                    let mesh = match piece.shape {
                        SpawnPadShape::Disc => cylinder_geometry(piece.w, piece.w, piece.h, 20),
                        SpawnPadShape::Post => cylinder_geometry(piece.w, piece.w, piece.h, 8),
                        _ => box_geometry(piece.w, piece.h, piece.d),
                    };
                    let mut node = Node::mesh(Arc::new(mesh), material.clone());
                    node.position = DVec3::new(spawn.x + piece.dx, piece.y, spawn.z + piece.dz);
                    node.rotation = glam::DQuat::from_rotation_y(piece.rot_y);
                    root.children.push(node);
                }
            }
        }
        root
    }

    /// Every sample on and around one pad per team: a decal lifted there clears
    /// the real deck without floating far above it, and lifts only where a deck is.
    fn assert_decks_match(theme: &str, pads: &Node, scale: f64) {
        const STEP: f64 = 0.05;
        const EXTENT: f64 = 4.6;
        // Where a flat-sided footprint stands in for a chevron or a gapped rim.
        const EDGE: f64 = 0.1;
        const MAX_FLOAT: f64 = 0.045;
        let decks = SpawnPadDecks::new(theme, scale);
        for team in [Team::Blue, Team::Red] {
            let center = spawn_positions(team, scale)[1];
            let triangles = triangles_near(pads, center, EXTENT);
            let steps = (EXTENT * 2.0 / STEP) as usize;
            // Offset off the exact tier edges the grid would otherwise hit.
            let at = |i: usize, j: usize| {
                (
                    center.x - EXTENT + i as f64 * STEP + 0.0013,
                    center.z - EXTENT + j as f64 * STEP + 0.0017,
                )
            };
            // The deck under each sample; floors and ground paint under the 7.5 cm tread
            // marks are not pad decks.
            let decks_under: Vec<Vec<Option<f64>>> = (0..=steps)
                .map(|i| {
                    (0..=steps)
                        .map(|j| {
                            let (x, z) = at(i, j);
                            surface_top(&triangles, x, z).filter(|&y| y > 0.05)
                        })
                        .collect()
                })
                .collect();
            let reach = (EDGE / STEP) as usize;
            let mut lifted = 0;
            for i in 0..=steps {
                for j in 0..=steps {
                    let (x, z) = at(i, j);
                    let height = decks.decal_height(x, z, 0.0, 0.0);
                    if let Some(deck) = decks_under[i][j] {
                        lifted += 1;
                        assert!(
                            height > deck + 0.004 && height < deck + MAX_FLOAT,
                            "{theme} decal {height} over deck {deck} at {x} {z}"
                        );
                    } else if height > 0.0 {
                        // A lift off the model must hug a deck edge.
                        let near = (i.saturating_sub(reach)..=(i + reach).min(steps)).any(|a| {
                            (j.saturating_sub(reach)..=(j + reach).min(steps))
                                .any(|b| decks_under[a][b].is_some())
                        });
                        assert!(
                            near,
                            "{theme} decal lifted to {height} off the pad at {x} {z}"
                        );
                    }
                }
            }
            assert!(lifted > 1000, "{theme} sampled only {lifted} deck points");
        }
    }

    /// Each theme's real pad models (the harbor's inside its whole scenery).
    pub(crate) fn pad_models() -> [(&'static str, Node); 3] {
        [
            ("village", create_spawn_pads(1.0)),
            ("harbor", HarborScenery::new().root),
            ("quarry", quarry_pads()),
        ]
    }

    #[test]
    fn decks_follow_every_themes_spawn_pad_models() {
        for (theme, pads) in pad_models() {
            assert_decks_match(theme, &pads, 1.0);
        }
        assert_decks_match("superstress", &create_spawn_pads(0.65), 0.65);
    }

    #[test]
    fn open_ground_keeps_its_decal_height() {
        for theme in ["village", "harbor", "quarry", "stress-test"] {
            let decks = SpawnPadDecks::new(theme, 1.0);
            let top = |x, z| decks.top_within(x, z, 0.0);
            for (x, z) in [(0.0, 0.0), (-40.0, -23.0), (53.0, 11.5), (-53.0, 30.0)] {
                assert_eq!(top(x, z), None, "{theme} {x} {z}");
                assert_eq!(decks.decal_height(x, z, 0.0, 0.075), 0.075);
            }
            assert!(top(-53.0, -23.0).is_some(), "{theme} pad centre");
            assert!(top(53.0, 46.0).is_some(), "{theme} red pad");
        }
    }

    #[test]
    fn a_radius_reaches_the_pad_from_beside_it() {
        let decks = SpawnPadDecks::new("village", 1.0);
        // Five metres in front of a blue pad: past its chevrons.
        assert_eq!(decks.top_within(-48.0, 0.0, 0.0), None);
        assert_eq!(decks.top_within(-48.0, 0.0, 1.2), Some(0.09));
        assert!(decks.top_within(-48.0, 0.0, 3.5).unwrap() > 0.2);
    }
}
