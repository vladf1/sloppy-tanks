//! Tanks run down thin cover. A young tree's trunk or a street lamp's pole does not stop
//! a hull: a tank driving into one knocks it over and drives on, a little slower, the
//! lighter the tank the more it loses. Thicker trees, walls and everything else stop a
//! hull as before.
//!
//! The pass runs after the tanks are driven and before the physics step, on where each
//! hull is about to be: the solver stops a hull as it reaches cover, so a pass after the
//! step would see a tank already halted. It draws nothing from the gameplay random
//! stream itself; what it knocks down breaks through the cover's own damage path.

use super::data::{STEP, vehicle};
use super::math::{Point3, Vec2};
use super::physics::{from_vector, vector};
use super::scenery_pieces::topple_post;
use super::simulation::Simulation;
use super::structure::PartSet;
use super::tank_dimensions::tank_hull;
use super::tree_proportions::tree_proportions;
use super::tree_structure::{TREE_BUTT, TREE_STUMP};
use super::types::{Cover, CoverKind, SimEvent, SimEventType};

/// A tree goes down to any hull when its trunk is at most this thick (m, its radius at
/// the butt): a parkway's or a median's young tree, not a village's or an old oak.
pub const RAM_TRUNK_RADIUS: f64 = 0.25;
/// A destructible post that stands still goes down to any hull when it is at most this
/// wide (m): a street lamp's pole.
pub const RAM_POST_WIDTH: f64 = 0.5;
/// How far ahead (s) of a hull its path is checked. The solver slows a hull a tick before
/// it touches cover (its contact prediction), so a hull is met a few ticks out.
const RAM_LOOKAHEAD: f64 = 3.0 * STEP;
/// Nearer than this (m) a hull counts as touching what it drives at.
const RAM_MARGIN: f64 = 0.05;
/// The least speed (m/s) that knocks thin cover down. A tick of drive from rest is over
/// three times this, so a tank standing against a sapling pushes it over as it sets off;
/// one drifting to a stop beside it does not.
const RAM_MIN_SPEED: f64 = 0.5;
/// The share of its speed a hull loses to what it runs down is this over its mass
/// (`VehicleStats::mass`), at most [`RAM_MOST_DRAG`]: 28% for the lightest hull, 8% for
/// the heaviest.
const RAM_DRAG: f64 = 0.2;
const RAM_MOST_DRAG: f64 = 0.5;
/// The height (m) a hull's bow strikes at.
const RAM_HEIGHT: f64 = 0.4;
/// The size of the knock a hull makes against what it runs down, as a shell's hit on
/// cover is sized, and how far up a tree (a share of its height) its crown shakes.
const RAM_KNOCK: f64 = 0.6;
const RAM_CROWN: f64 = 0.7;
/// How far to the side of the hull's travel a tree goes over, as a share of forward: it
/// falls ahead and off the side it was struck on, clear of the tracks.
const RAM_SIDEWAYS: f64 = 0.8;

/// The thin cover of a round, by index, with the radius (m) a hull meets.
#[derive(Default)]
pub(crate) struct ThinCover {
    /// The number of covers the list was made from; none before the first pass and after
    /// a reset.
    covers: Option<usize>,
    list: Vec<(usize, f64)>,
}

impl ThinCover {
    pub fn forget(&mut self) {
        self.covers = None;
    }
}

/// The radius (m) a hull meets `cover` at, if a hull runs it down.
fn thin_radius(cover: &Cover) -> Option<f64> {
    match cover.kind {
        CoverKind::Tree if cover.structure.is_some() => {
            let tree = tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h);
            (tree.radius <= RAM_TRUNK_RADIUS).then_some(tree.stump_radius)
        }
        // A barrel is no post: it rolls, and bursts when it breaks.
        CoverKind::Drum => None,
        _ if cover.structure.is_none()
            && cover.destructible
            && cover.motion.is_none()
            && cover.w.max(cover.d) <= RAM_POST_WIDTH =>
        {
            Some(cover.w.max(cover.d) / 2.0)
        }
        _ => None,
    }
}

/// Whether `cover` still stands in a hull's way.
fn stands(cover: &Cover) -> bool {
    match &cover.structure {
        Some(state) => state.standing(TREE_STUMP),
        None => cover.alive,
    }
}

/// Knock down the thin cover each moving hull is about to drive into.
pub fn run_down_thin_cover(simulation: &mut Simulation) {
    let mut thin = std::mem::take(&mut simulation.scratch.thin_cover);
    if thin.covers != Some(simulation.covers.len()) {
        thin.covers = Some(simulation.covers.len());
        thin.list.clear();
        thin.list.extend(
            simulation
                .covers
                .iter()
                .enumerate()
                .filter_map(|(index, cover)| Some((index, thin_radius(cover)?))),
        );
    }
    if !thin.list.is_empty() {
        for tank in 0..simulation.tanks.len() {
            run_down_ahead_of(simulation, tank, &thin.list);
        }
    }
    simulation.scratch.thin_cover = thin;
}

/// Knock down the thin cover hull `index` is about to drive into, and slow it for each.
fn run_down_ahead_of(simulation: &mut Simulation, index: usize, thin: &[(usize, f64)]) {
    let tank = &simulation.tanks[index];
    if !tank.alive {
        return;
    }
    let body = &simulation.world.bodies[tank.body];
    let velocity = from_vector(body.linvel()).planar();
    let speed = velocity.x.hypot(velocity.z);
    if speed < RAM_MIN_SPEED {
        return;
    }
    let position = from_vector(body.translation()).planar();
    let hull = tank_hull(tank.kind);
    let (sin, cos) = tank.heading.sin_cos();
    // The hull's frame: z along its bow, x to its right.
    let local = |x: f64, z: f64| Vec2::new(x * cos - z * sin, x * sin + z * cos);
    let half = Vec2::new(hull.size.x / 2.0, hull.size.z / 2.0);
    let reach = half.x.hypot(half.z) + hull.center.x.hypot(hull.center.z) + speed * RAM_LOOKAHEAD;
    let travel = Vec2::new(velocity.x / speed, velocity.z / speed);
    let (id, team, mass) = (tank.id, tank.team, vehicle(tank.kind).mass);
    let mut struck = 0;
    for &(cover, radius) in thin {
        let c = &simulation.covers[cover];
        let to = Vec2::new(c.x - position.x, c.z - position.z);
        if to.x.abs() > reach + radius || to.z.abs() > reach + radius || !stands(c) {
            continue;
        }
        // Only what the hull drives at: one it slides along or backs away from stays.
        if to.x * travel.x + to.z * travel.z <= 0.0 {
            continue;
        }
        // Inside the hull's box, grown by the cover's radius, now or where it will be.
        let meets = |ahead: f64| {
            let at = local(to.x - velocity.x * ahead, to.z - velocity.z * ahead);
            (at.x - hull.center.x).abs() <= half.x + radius + RAM_MARGIN
                && (at.z - hull.center.z).abs() <= half.z + radius + RAM_MARGIN
        };
        if !meets(0.0) && !meets(RAM_LOOKAHEAD) {
            continue;
        }
        // It goes over ahead of the hull and off the side it stands on.
        let side = local(to.x, to.z).x - hull.center.x;
        let right = Vec2::new(cos, -sin);
        let lean = if side < 0.0 { -RAM_SIDEWAYS } else { RAM_SIDEWAYS };
        let fall = Vec2::new(travel.x + right.x * lean, travel.z + right.z * lean);
        let from = Point3::new(c.x - fall.x, RAM_HEIGHT, c.z - fall.z);
        let (kind, color, cover_id, at, top) = (c.kind, c.color, c.id, Vec2::new(c.x, c.z), c.h);
        if c.structure.is_some() {
            let stood = c.alive;
            simulation.run_down_structure(
                cover,
                PartSet::from_indices([TREE_BUTT]),
                PartSet::from_indices([TREE_STUMP]),
                from,
                Some((id, team)),
            );
            if stood {
                // Its crown shakes as it goes over.
                let mut rustle = SimEvent::at(SimEventType::Rustle, at.x, at.z);
                rustle.id = Some(cover_id);
                rustle.height = Some(top * RAM_CROWN);
                rustle.cover_kind = Some(kind);
                simulation.events.push(rustle);
            }
        } else {
            simulation.take_cover_out(cover, id, team);
            topple_post(simulation, cover, (fall.x, fall.z));
        }
        // The knock of the hull against it, and chips of it: no burst.
        let mut knock = SimEvent::at(SimEventType::Impact, at.x, at.z);
        knock.id = Some(cover_id);
        knock.height = Some(RAM_HEIGHT);
        knock.size = Some(RAM_KNOCK);
        knock.color = Some(color);
        knock.cover_kind = Some(kind);
        simulation.events.push(knock);
        struck += 1;
    }
    if struck > 0 {
        let kept = (1.0 - (RAM_DRAG / mass).min(RAM_MOST_DRAG)).powi(struck);
        let body = &mut simulation.world.bodies[simulation.tanks[index].body];
        let velocity = body.linvel();
        body.set_linvel(
            vector(
                f64::from(velocity.x) * kept,
                f64::from(velocity.y),
                f64::from(velocity.z) * kept,
            ),
            true,
        );
    }
}
