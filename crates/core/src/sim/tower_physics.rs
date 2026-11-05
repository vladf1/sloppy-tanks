//! A watchtower's structure in play: its standing parts as colliders on the tower's
//! fixed body, shells and blasts breaking the parts they reach, and what no longer
//! stands coming down as rigid bodies.
//!
//! A broken part is gone at once (a `Shatter` event throws its splinters). When what is
//! left can no longer stand ([`TowerStructure::falls`]) it comes down as one body: a
//! frame that lost a leg tips over toward the gap about the legs still in their shoes,
//! the deck, cabin and roof with it, and nothing hangs where it stood. A body that lands
//! hard breaks into its sections (a leg, a face's bracing, a deck half with its
//! railing, a wall, the roof, the ladder) while the debris budget has room, and every
//! loose section settles, lies a while and sinks away like other debris. The footings
//! and their steel shoes stay as low cover; a rebuilt tower clears what is left of the
//! last one.
//!
//! No random draw: every variation comes from the tower's parts and where it was hit.

use glam::{DVec2, DVec3};
use rapier3d::prelude::{
    ActiveEvents, ColliderBuilder, ColliderHandle, Pose, RigidBodyBuilder, RigidBodyHandle,
    Rotation, SharedShape,
};

use super::data::{STEP, group};
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{DebrisMaterial, debris_material, track_debris_contacts};
use super::house_physics::house_debris_damping;
use super::house_structure::PartSet;
use super::math::{Point3, Vec2};
use super::navigation::Footprint;
use super::physics::{ContactForce, from_vector, interaction_groups, vector};
use super::simulation::Simulation;
use super::tower_structure::{
    DECKS, FACES, LOWER_LEGS, PartShape, RAILINGS, TowerFall, TowerPartKind, tower_structure,
};
use super::types::{Cover, CoverKind, Fragment, SimEvent, SimEventType, Team};

/// A falling body's colliders sit this far (m) inside its parts' faces, clear of the
/// standing parts they were built against.
const SHRINK: f64 = 0.03;
/// Standing parts' colliders reach this far (m) past their faces into their
/// neighbours', so a shell along the seam between two parts meets one.
const SEAM: f64 = 0.01;
/// A frame tipping over starts turning at this rate (rad/s) about the edge of its legs
/// it tips over; its own weight does the rest.
const TIP_RATE: f64 = 0.35;
/// What broke this near (m, across the ground) the tower's middle names no side for a
/// frame to tip toward; it tips away from the blast instead.
const TOWARD_CENTRE: f64 = 0.5;
/// A part torn loose by a blast is thrown away from it this fast (m/s), and one that
/// loses its hold drops at this speed.
const BLAST_THROW: f64 = 2.5;
const SLUMP_SPEED: f64 = 0.6;
/// A blast shatters the parts within this far (m) of it; parts farther off that it
/// breaks come loose whole and fall, so splinters fly only where it went off.
const BLAST_CORE: f64 = 0.5;
/// A falling body breaks into its sections when its velocity changes by this much
/// (m/s) in one tick: a hard landing.
const LANDING_SPEED: f64 = 2.5;
/// The dust a tower's top raises where it crashes down: its reach (m) and height (m).
const CRASH_SIZE: f64 = 7.0;
const CRASH_HEIGHT: f64 = 1.5;
/// The share of the falling frame's spin each section keeps when it breaks up.
const LANDING_SPIN: f32 = 0.5;
/// Seconds a loose piece lies before it sinks away (plus the cleanup fade), and its
/// hard deadline: by about 20 s only the footings and their stubs are left.
const PART_LIFE: f64 = 16.0;
const PART_DEADLINE: f64 = 24.0;
/// A landed piece slower than this (m/s and rad/s) for [`REST_HOLD`] seconds has come to
/// rest: it can no longer turn, so it stays as it lies until it sinks (it can still be
/// shoved along or drop if what it lies on goes).
const REST_SPEED: f32 = 0.3;
const REST_SPIN: f32 = 0.6;
const REST_HOLD: f64 = 0.2;
/// A landed piece is damped to rest this long (s) after the landing, and rests for good
/// by this long after it, still or not: the rocking is over within about a second.
const LANDED_SETTLE: f64 = 0.8;
const LANDED_REST: f64 = 1.3;
/// Seconds earlier a resting piece clears for every metre its middle lies up the pile.
const CLEAR_TOP_FIRST: f64 = 1.2;
/// A falling frame counts as at rest no sooner than this (s) after it came loose.
const REST_AFTER: f64 = 1.0;
/// Nobody: the owner a falling tower's damage to the cover it lands on is credited to.
const NOBODY: u32 = u32::MAX;
/// A loose part's damping while it falls, as a cottage's.
const FALLING_DAMPING: (f32, f32) = (0.1, 0.3);
/// The damage each unit of impulse a falling body strikes a standing part with does,
/// and the contact force per unit of a falling body's mass that counts as striking
/// rather than resting.
const CRUSH_DAMAGE: f64 = 6.0;
const CRUSH_FORCE_PER_MASS: f64 = 40.0;
/// The most splinter events one break sends.
const SHATTER_EVENTS: usize = 6;
/// The paints of the tower's timber, its roof, its concrete and its steel.
const POST_PAINT: u32 = 0x887454;
const BRACE_PAINT: u32 = 0x96734c;
const DECK_PAINT: u32 = 0x887d59;
const ROOF_PAINT: u32 = 0x2b6a4c;
const CONCRETE_PAINT: u32 = 0x9a968c;
const STEEL_PAINT: u32 = 0x5d6266;

/// Where a falling tower body has got to: its velocity last tick (to tell a hard
/// landing), whether it has landed (a landed body never breaks again), and since when
/// it has lain still and whether it has come to rest for good.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TowerLanding {
    pub velocity: Point3,
    pub landed: bool,
    pub still_since: Option<f64>,
    pub rested: bool,
}

/// A watchtower's structure as it stands: the parts broken or fallen from it, the
/// damage its standing parts have taken and their colliders.
#[derive(Clone, Debug)]
pub struct TowerState {
    pub gone: PartSet,
    pub damage: Vec<f64>,
    pub colliders: Vec<Option<ColliderHandle>>,
    /// Its deck is down: it counts as destroyed, and only what stands of it on the
    /// ground blocks routes.
    pub levelled: bool,
}

impl Default for TowerState {
    fn default() -> Self {
        let count = tower_structure().parts.len();
        Self {
            gone: PartSet::EMPTY,
            damage: vec![0.0; count],
            colliders: vec![None; count],
            levelled: false,
        }
    }
}

impl TowerState {
    pub fn standing(&self, part: usize) -> bool {
        !self.gone.contains(part)
    }

    /// The parts that have taken damage, standing or not: a client's own-shot
    /// prediction reads the standing ones, and the model scars a struck leg and leaves a
    /// tall stub of a shot-out one.
    pub fn cracked(&self) -> PartSet {
        PartSet::from_indices((0..self.damage.len()).filter(|&index| self.damage[index] > 0.0))
    }

    /// The share of the tower's breakable parts still standing.
    pub fn standing_share(&self) -> f64 {
        let parts = &tower_structure().parts;
        let breakable = parts
            .iter()
            .filter(|part| part.kind != TowerPartKind::Footing)
            .count();
        let standing = (0..parts.len())
            .filter(|&index| parts[index].kind != TowerPartKind::Footing && self.standing(index))
            .count();
        standing as f64 / breakable as f64
    }
}

/// A part's collider relative to its tower's ground centre, shrunk by `inset` metres,
/// marked with the part's index.
pub fn tower_part_collider(index: usize, inset: f64) -> ColliderBuilder {
    let part = tower_structure().member(index);
    let mut shapes: Vec<(Pose, SharedShape)> =
        part.shapes.iter().map(|shape| shape_of(shape, inset)).collect();
    let builder = if shapes.len() == 1 {
        let (pose, shape) = shapes.remove(0);
        ColliderBuilder::new(shape).position(pose)
    } else {
        ColliderBuilder::compound(shapes)
    };
    builder.user_data(index as u128)
}

fn shape_of(shape: &PartShape, inset: f64) -> (Pose, SharedShape) {
    match shape {
        PartShape::Box {
            center,
            half,
            rotation,
        } => {
            let half = (*half - DVec3::splat(inset)).max(DVec3::splat(0.01));
            let rotation = Rotation::from_xyzw(
                rotation.x as f32,
                rotation.y as f32,
                rotation.z as f32,
                rotation.w as f32,
            );
            (
                Pose::from_parts(vector(center.x, center.y, center.z), rotation),
                SharedShape::cuboid(half.x as f32, half.y as f32, half.z as f32),
            )
        }
        PartShape::Hull(points) => {
            let center = points.iter().copied().sum::<DVec3>() / points.len() as f64;
            let inset: Vec<_> = points
                .iter()
                .map(|&p| {
                    let p = p + (center - p).normalize_or_zero() * inset;
                    vector(p.x, p.y, p.z)
                })
                .collect();
            let hull = SharedShape::convex_hull(&inset).expect("the roof's points span a hull");
            (Pose::IDENTITY, hull)
        }
    }
}

/// The colliders of a body of a tower's loose `parts`, relative to the tower's ground
/// centre where they fell from: the host's, and a client's for its predicted hull.
pub fn tower_debris_colliders(parts: &PartSet) -> Vec<ColliderBuilder> {
    let layout = tower_structure();
    parts
        .iter()
        .map(|index| {
            let part = layout.member(index);
            let surface = debris_material(material(part.kind));
            // Timber this size lands dead: no bounce lifts a section off the ground.
            tower_part_collider(index, SHRINK)
                .collision_groups(interaction_groups(group::HOUSE_SHARD))
                .mass(part.mass as f32)
                .friction(surface.friction as f32)
                .restitution(0.0)
        })
        .collect()
}

/// The colliders of a tower's standing parts (those not in `gone`), in part order, on
/// a fixed body at its ground centre.
pub fn standing_tower_colliders(gone: &PartSet) -> Vec<ColliderBuilder> {
    (0..tower_structure().parts.len())
        .filter(|&index| !gone.contains(index))
        .map(standing_collider)
        .collect()
}

fn standing_collider(index: usize) -> ColliderBuilder {
    tower_part_collider(index, -SEAM)
        .collision_groups(interaction_groups(group::COVER))
        .friction(0.4)
}

fn material(kind: TowerPartKind) -> DebrisMaterial {
    match kind {
        TowerPartKind::Ladder => DebrisMaterial::Metal,
        TowerPartKind::Footing => DebrisMaterial::Concrete,
        _ => DebrisMaterial::Wood,
    }
}

/// The paint a part's splinters and dust take.
fn part_paint(kind: TowerPartKind, cabin: u32) -> u32 {
    match kind {
        TowerPartKind::Footing => CONCRETE_PAINT,
        TowerPartKind::LowerLeg | TowerPartKind::UpperLeg => POST_PAINT,
        TowerPartKind::Brace | TowerPartKind::Girt | TowerPartKind::Railing => BRACE_PAINT,
        TowerPartKind::Deck => DECK_PAINT,
        TowerPartKind::Wall => cabin,
        TowerPartKind::Roof => ROOF_PAINT,
        TowerPartKind::Ladder => STEEL_PAINT,
    }
}

/// The world boxes of a tower that block tanks on the ground: its whole box while its
/// deck stands over the legs, then its footings and whatever else of it still stands on
/// the ground.
pub fn tower_ground_footprints(cover: &Cover, out: &mut Vec<Footprint>) {
    let Some(state) = &cover.tower else {
        return;
    };
    if !state.levelled {
        out.push(Footprint::from(cover));
        return;
    }
    for (index, part) in tower_structure().parts.iter().enumerate() {
        if state.standing(index) && part.blocks_ground() {
            out.push(Footprint {
                x: cover.x + (part.min.x + part.max.x) / 2.0,
                z: cover.z + (part.min.z + part.max.z) / 2.0,
                w: part.max.x - part.min.x,
                d: part.max.z - part.min.z,
            });
        }
    }
}

/// Where to shoot watchtower `cover` from `from` to bring it down: its nearest leg still
/// standing in its shoe, or `None` for other cover or a tower with none.
pub fn tower_breach_point(cover: &Cover, from: Vec2) -> Option<Vec2> {
    let state = cover.tower.as_ref()?;
    LOWER_LEGS
        .iter()
        .filter(|&&leg| state.standing(leg))
        .map(|&leg| {
            let center = tower_structure().parts[leg].center;
            Vec2::new(cover.x + center.x, cover.z + center.z)
        })
        .min_by(|a, b| {
            let gap = |p: &Vec2| (p.x - from.x).hypot(p.z - from.z);
            gap(a).total_cmp(&gap(b))
        })
}

/// Whether `fragment` is a body of a tower's fallen parts.
pub fn tower_debris(fragment: &Fragment) -> bool {
    fragment.tower_parts.is_some()
}

/// The sections a landing body breaks into, in order of their lowest part: each leg
/// (both its lifts), each face's bracing, each deck half with its railing, each wall,
/// the roof and the ladder.
pub fn tower_sections(parts: &PartSet) -> Vec<PartSet> {
    let section = |index: usize| -> usize {
        let part = &tower_structure().parts[index];
        if let Some(corner) = part.corner {
            return corner;
        }
        if let Some(face) = FACES.iter().position(|members| members.contains(&index)) {
            return 4 + face;
        }
        if let Some(side) = RAILINGS.iter().position(|&railing| railing == index) {
            return 8 + side;
        }
        if let Some(side) = DECKS.iter().position(|&deck| deck == index) {
            return 8 + side;
        }
        10 + index
    };
    let mut sections: Vec<(usize, PartSet)> = Vec::new();
    for index in parts.iter() {
        let key = section(index);
        match sections.iter_mut().find(|(other, _)| *other == key) {
            Some((_, set)) => set.insert(index),
            None => sections.push((key, PartSet::from_indices([index]))),
        }
    }
    sections.into_iter().map(|(_, set)| set).collect()
}

impl Simulation {
    /// The part of a watchtower behind `collider` (from its user data), if it is one.
    pub fn tower_part(&self, collider: ColliderHandle) -> Option<(usize, usize)> {
        let &cover = self.cover_by_collider.get(&collider)?;
        self.covers[cover].tower.as_ref()?;
        let part = self.world.colliders.get(collider)?.user_data as usize;
        Some((cover, part))
    }

    /// Hit watchtower `cover`'s `part` for `amount` at `point`, from `along` (the way
    /// the shell flew); `force` breaks it whatever its strength (an armour-piercing
    /// round through it). A footing never breaks. Returns whether the part broke.
    #[allow(clippy::too_many_arguments)]
    pub fn strike_tower(
        &mut self,
        cover: usize,
        part: usize,
        amount: f64,
        force: bool,
        point: Point3,
        along: Vec2,
        by: Option<(u32, Team)>,
    ) -> bool {
        let Some(state) = &mut self.covers[cover].tower else {
            return false;
        };
        let strength = tower_structure().parts[part].strength;
        if !state.standing(part) || strength.is_infinite() {
            return false;
        }
        state.damage[part] += amount;
        if !force && state.damage[part] < strength {
            return false;
        }
        let from = Point3::new(point.x - along.x, point.y, point.z - along.z);
        self.break_tower_parts(cover, PartSet::from_indices([part]), PartSet::EMPTY, Some(from), by);
        true
    }

    /// A burst at `point` damaging every watchtower part within `radius`, falling off
    /// to nothing at its edge.
    pub fn tower_blast(&mut self, point: Point3, radius: f64, damage: f64, by: Option<(u32, Team)>) {
        for cover in 0..self.covers.len() {
            self.tower_blast_on(cover, point, radius, damage, by);
        }
    }

    /// [`Self::tower_blast`] on one tower.
    pub fn tower_blast_on(
        &mut self,
        cover: usize,
        point: Point3,
        radius: f64,
        damage: f64,
        by: Option<(u32, Team)>,
    ) {
        let c = &self.covers[cover];
        let Some(state) = &c.tower else {
            return;
        };
        let reach = Vec2::new(
            ((point.x - c.x).abs() - c.w / 2.0).max(0.0),
            ((point.z - c.z).abs() - c.d / 2.0).max(0.0),
        );
        if reach.x.hypot(reach.z) >= radius {
            return;
        }
        let local = DVec3::new(point.x - c.x, point.y, point.z - c.z);
        let mut hits: Vec<(usize, f64, f64)> = Vec::new();
        for (index, part) in tower_structure().parts.iter().enumerate() {
            if !state.standing(index) || part.strength.is_infinite() || part.distance(local) >= radius {
                continue;
            }
            // How far the burst is from the part itself: a diagonal's box reaches far
            // past the member.
            let gap = state.colliders[index]
                .and_then(|handle| self.world.colliders.get(handle))
                .map_or(part.distance(local), |collider| {
                    f64::from(collider.shape().distance_to_point(
                        collider.position(),
                        vector(point.x, point.y, point.z),
                        true,
                    ))
                });
            if gap < radius {
                hits.push((index, damage * (1.0 - gap / radius), gap));
            }
        }
        let state = self.covers[cover].tower.as_mut().expect("checked above");
        let (mut broken, mut loose) = (PartSet::EMPTY, PartSet::EMPTY);
        for (index, amount, gap) in hits {
            state.damage[index] += amount;
            if state.damage[index] >= tower_structure().parts[index].strength {
                if gap < BLAST_CORE {
                    broken.insert(index);
                } else {
                    loose.insert(index);
                }
            }
        }
        if !broken.is_empty() || !loose.is_empty() {
            self.break_tower_parts(cover, broken, loose, Some(point), by);
        }
    }

    /// Every standing part of watchtower `cover` takes `amount`. The parts on the
    /// ground that it breaks shatter; what it breaks above them comes loose whole and
    /// falls.
    pub fn damage_every_tower_part(&mut self, cover: usize, amount: f64, by: Option<(u32, Team)>) {
        let Some(state) = &mut self.covers[cover].tower else {
            return;
        };
        let (mut broken, mut loose) = (PartSet::EMPTY, PartSet::EMPTY);
        for (index, part) in tower_structure().parts.iter().enumerate() {
            if !state.standing(index) || part.strength.is_infinite() {
                continue;
            }
            state.damage[index] += amount;
            if state.damage[index] >= part.strength {
                if part.min.y < 0.5 {
                    broken.insert(index);
                } else {
                    loose.insert(index);
                }
            }
        }
        if !broken.is_empty() || !loose.is_empty() {
            self.break_tower_parts(cover, broken, loose, None, by);
        }
    }

    /// Stand watchtower `cover` as a scene shows it, with exactly the parts not in
    /// `gone` (a model of a room refilled from what a client sees): their colliders come
    /// and go, no part falls and nothing is scored.
    pub fn set_tower_gone(&mut self, cover: usize, gone: PartSet) {
        let body = self.covers[cover].body;
        let Some(state) = &mut self.covers[cover].tower else {
            return;
        };
        if state.gone == gone || !self.world.bodies.contains(body) {
            return;
        }
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for index in 0..state.colliders.len() {
            match (gone.contains(index), state.colliders[index]) {
                (true, Some(handle)) => {
                    removed.push(handle);
                    state.colliders[index] = None;
                }
                (false, None) => added.push(index),
                _ => {}
            }
        }
        state.gone = gone;
        state.levelled = DECKS.iter().all(|&deck| gone.contains(deck));
        for handle in removed {
            self.cover_by_collider.remove(&handle);
            self.world.remove_collider(handle);
        }
        for index in added {
            let handle = self.world.insert_collider(standing_collider(index), Some(body));
            self.cover_by_collider.insert(handle, cover);
            if let Some(state) = &mut self.covers[cover].tower {
                state.colliders[index] = Some(handle);
            }
        }
        let region = Footprint::from(&self.covers[cover]);
        self.nav.rebuild(&self.covers, Some(region));
    }

    /// Break `broken` out of watchtower `cover` (each throws its splinters away from
    /// `from`) and tear `loose` free whole, then bring down whatever can no longer
    /// stand.
    pub fn break_tower_parts(
        &mut self,
        cover: usize,
        broken: PartSet,
        loose: PartSet,
        from: Option<Point3>,
        by: Option<(u32, Team)>,
    ) {
        let c = &self.covers[cover];
        let (x, z, id, cabin) = (c.x, c.z, c.id, c.color);
        let Some(state) = &c.tower else {
            return;
        };
        let layout = tower_structure();
        let breakable = |index: usize| state.standing(index) && layout.parts[index].strength.is_finite();
        let broken = PartSet::from_indices(broken.iter().filter(|&index| breakable(index)));
        let loose = PartSet::from_indices(
            loose
                .iter()
                .filter(|&index| breakable(index) && !broken.contains(index)),
        );
        if broken.is_empty() && loose.is_empty() {
            return;
        }
        // Splinters from the heaviest broken parts.
        let mut thrown: Vec<usize> = broken.iter().collect();
        thrown.sort_by(|&a, &b| {
            layout.parts[b]
                .mass
                .total_cmp(&layout.parts[a].mass)
                .then(a.cmp(&b))
        });
        for &index in thrown.iter().take(SHATTER_EVENTS) {
            let part = &layout.parts[index];
            let center = part.center;
            let mut event = SimEvent::at(SimEventType::Shatter, x + center.x, z + center.z);
            event.height = Some(center.y);
            event.size = Some((part.max - part.min).max_element().min(2.5));
            event.color = Some(part_paint(part.kind, cabin));
            event.cover_kind = Some(CoverKind::Tower);
            event.id = Some(id);
            event.from = from;
            self.events.push(event);
        }
        self.remove_tower_parts(cover, &broken);
        // A frame whose weight lies on the line of its legs tips toward what broke, or
        // away from the blast that broke all round it.
        let parts = broken.union(&loose);
        let mut toward =
            parts.iter().map(|index| layout.parts[index].center).sum::<DVec3>() / parts.len() as f64;
        if let Some(from) = from
            && DVec2::new(toward.x, toward.z).length() < TOWARD_CENTRE
        {
            toward = DVec3::new(x - from.x, 0.0, z - from.z);
        }
        let gone = self.covers[cover]
            .tower
            .as_ref()
            .map_or(PartSet::EMPTY, |state| state.gone);
        let mut left = broken;
        for fall in layout.falls(&gone, &loose, toward) {
            self.remove_tower_parts(cover, &fall.parts);
            left = left.union(&fall.parts);
            let blast = fall
                .parts
                .iter()
                .any(|index| loose.contains(index))
                .then_some(from)
                .flatten();
            self.drop_tower_fall(cover, &fall, blast);
        }
        self.after_tower_break(cover, &left, by);
    }

    /// Take `parts` out of the tower's standing structure and the world.
    fn remove_tower_parts(&mut self, cover: usize, parts: &PartSet) {
        let Some(state) = &mut self.covers[cover].tower else {
            return;
        };
        let mut removed = Vec::new();
        for index in parts.iter() {
            state.gone.insert(index);
            if let Some(handle) = state.colliders[index].take() {
                removed.push(handle);
            }
        }
        for handle in removed {
            self.cover_by_collider.remove(&handle);
            self.world.remove_collider(handle);
        }
    }

    /// The health, score and routes after parts left watchtower `cover`: it counts as
    /// destroyed once its deck is down.
    fn after_tower_break(&mut self, cover: usize, left: &PartSet, by: Option<(u32, Team)>) {
        let c = &mut self.covers[cover];
        let Some(state) = &mut c.tower else {
            return;
        };
        c.hp = c.max_hp * state.standing_share();
        let levelled = !state.levelled && DECKS.iter().all(|&deck| state.gone.contains(deck));
        if levelled {
            // It counts as destroyed as its deck starts down; its dust and noise come
            // when the deck lands ([`settle_tower_debris`]).
            state.levelled = true;
            c.alive = false;
            self.destroyed += 1;
            if let Some((owner, team)) = by
                && self
                    .tanks
                    .iter()
                    .any(|tank| self.records(tank) && tank.id == owner && tank.team == team)
            {
                self.combat_record.cover_destroyed += 1;
            }
        }
        let parts = &tower_structure().parts;
        let blocking = left.iter().any(|index| parts[index].blocks_ground());
        if levelled || (self.covers[cover].tower.as_ref().is_some_and(|state| state.levelled) && blocking) {
            let region = Footprint::from(&self.covers[cover]);
            self.nav.rebuild(&self.covers, Some(region));
        }
    }

    /// `fall` of watchtower `cover`'s parts coming down as one body: a frame tipping
    /// over turns about the edge of its legs it tips over, a part torn loose by a blast
    /// is thrown from it, anything else drops.
    fn drop_tower_fall(&mut self, cover: usize, fall: &TowerFall, blast: Option<Point3>) {
        let c = &self.covers[cover];
        let (x, z, id, color) = (c.x, c.z, c.id, c.color);
        let body = self.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, 0.0, z))
                .linear_damping(FALLING_DAMPING.0)
                .angular_damping(FALLING_DAMPING.1)
                .can_sleep(true),
        );
        let colliders = self.insert_tower_debris(&fall.parts, body);
        let rigid_body = &mut self.world.bodies[body];
        let center = from_vector(rigid_body.center_of_mass());
        let center = DVec3::new(center.x - x, center.y, center.z - z);
        let (linear, angular) = match fall.tip {
            Some((pivot, way)) => {
                let spin = DVec3::Y.cross(DVec3::new(way.x, 0.0, way.y)) * TIP_RATE;
                (spin.cross(center - pivot), spin)
            }
            None => {
                let mut linear = DVec3::NEG_Y * SLUMP_SPEED;
                if let Some(blast) = blast {
                    let away = DVec2::new(x + center.x - blast.x, z + center.z - blast.z)
                        .normalize_or_zero()
                        * BLAST_THROW;
                    linear += DVec3::new(away.x, 0.0, away.y);
                }
                (linear, DVec3::ZERO)
            }
        };
        rigid_body.set_linvel(vector(linear.x, linear.y, linear.z), true);
        rigid_body.set_angvel(vector(angular.x, angular.y, angular.z), true);
        self.add_tower_fragment(id, color, body, &colliders, fall.parts);
    }

    /// A falling body's colliders, one for each of `parts`, each reporting what it
    /// lands on hard ([`crush_tower_parts`]).
    fn insert_tower_debris(&mut self, parts: &PartSet, body: RigidBodyHandle) -> Vec<ColliderHandle> {
        let colliders: Vec<ColliderHandle> = tower_debris_colliders(parts)
            .into_iter()
            .map(|collider| self.world.insert_collider(collider, Some(body)))
            .collect();
        let threshold = (f64::from(self.world.bodies[body].mass()) * CRUSH_FORCE_PER_MASS) as f32;
        for &handle in &colliders {
            let collider = &mut self.world.colliders[handle];
            collider.set_active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
            collider.set_contact_force_event_threshold(threshold);
        }
        colliders
    }

    fn add_tower_fragment(
        &mut self,
        tower: u32,
        color: u32,
        body: RigidBodyHandle,
        colliders: &[ColliderHandle],
        parts: PartSet,
    ) {
        self.reserve_fragments(1);
        let fragment_id = self.allocate_id();
        if let Some(&first) = colliders.first() {
            track_debris_contacts(self, body, first, fragment_id, DebrisMaterial::Wood);
        }
        let mut fragment = Fragment::new(
            fragment_id,
            body,
            PART_LIFE + DEBRIS_CLEANUP_SECONDS,
            1.0,
            color,
        );
        fragment.material = Some(DebrisMaterial::Wood);
        fragment.source_kind = Some(CoverKind::Tower);
        fragment.tower_id = Some(tower);
        fragment.tower_parts = Some(parts);
        fragment.created_at = Some(self.elapsed);
        fragment.expires_at = Some(self.elapsed + PART_DEADLINE);
        fragment.tower_landing = Some(TowerLanding {
            velocity: from_vector(self.world.bodies[body].linvel()),
            ..TowerLanding::default()
        });
        self.fragments.push(fragment);
    }

    /// Break a landed body into `sections`, each a body of its own moving as it was.
    fn break_tower_fragment(&mut self, index: usize, sections: Vec<PartSet>) {
        let fragment = self.fragments.remove(index);
        let old = &self.world.bodies[fragment.body];
        let (position, rotation) = (old.translation(), *old.rotation());
        let (linvel, angvel) = (old.linvel(), old.angvel());
        let center = old.center_of_mass();
        self.remove_body(fragment.body);
        for section in sections {
            let body = self.world.insert_body(
                RigidBodyBuilder::dynamic()
                    .translation(position)
                    .rotation(rotation.to_scaled_axis())
                    .linear_damping(FALLING_DAMPING.0)
                    .angular_damping(FALLING_DAMPING.1)
                    .can_sleep(true),
            );
            let colliders = self.insert_tower_debris(&section, body);
            let rigid_body = &mut self.world.bodies[body];
            let r = rigid_body.center_of_mass() - center;
            // The landing's knock does not fling the far end of the frame up: each
            // section goes on as it moved there, no faster upward than not at all.
            let mut velocity = linvel + angvel.cross(r);
            velocity.y = velocity.y.min(0.0);
            rigid_body.set_linvel(velocity, true);
            rigid_body.set_angvel(angvel * LANDING_SPIN, true);
            self.reserve_fragments(1);
            let fragment_id = self.allocate_id();
            if let Some(&first) = colliders.first() {
                track_debris_contacts(self, body, first, fragment_id, DebrisMaterial::Wood);
            }
            // A section is a body of its own from its landing: it falls flat over the
            // next moments before it is damped to rest, rather than freezing on its edge.
            let mut piece = fragment.clone();
            piece.id = fragment_id;
            piece.body = body;
            piece.created_at = Some(self.elapsed);
            piece.tower_parts = Some(section);
            piece.tower_landing = Some(TowerLanding {
                velocity: from_vector(self.world.bodies[body].linvel()),
                landed: true,
                ..TowerLanding::default()
            });
            self.fragments.push(piece);
        }
    }
}

/// Each tick: a falling tower body that lands hard on the ground breaks where it struck
/// ([`TowerStructure::landing`], or into its coarser sections when the debris budget is
/// short), its top crashing down with dust and noise however full the budget is; a
/// body caught on cover stays whole until it comes down off it or breaks through. A
/// landed piece that has lain still a moment comes to rest for good: it no longer turns.
pub fn settle_tower_debris(simulation: &mut Simulation) {
    let layout = tower_structure();
    let mut index = 0;
    while index < simulation.fragments.len() {
        let fragment = &simulation.fragments[index];
        let (Some(parts), Some(landing)) = (fragment.tower_parts, fragment.tower_landing) else {
            index += 1;
            continue;
        };
        let handle = fragment.body;
        let body = &simulation.world.bodies[handle];
        let now = from_vector(body.linvel());
        let before = landing.velocity;
        let change = (now.x - before.x)
            .hypot(now.y - before.y)
            .hypot(now.z - before.z);
        let age = simulation.elapsed - fragment.created_at.unwrap_or(simulation.elapsed);
        // A frame caught on cover that has come to rest there breaks up where it lies
        // as if it had landed: it does not lie whole and jammed.
        let jammed = !landing.landed
            && parts.len() > 1
            && landing
                .still_since
                .is_some_and(|since| simulation.elapsed - since >= REST_HOLD);
        if !landing.landed
            && fragment.life > DEBRIS_CLEANUP_SECONDS
            && (jammed || (change > LANDING_SPEED && on_ground(simulation, handle)))
        {
            if DECKS.iter().any(|&deck| parts.contains(deck)) {
                let at = from_vector(body.center_of_mass());
                let mut crash = SimEvent::at(SimEventType::Destroy, at.x, at.z);
                crash.cover_kind = Some(CoverKind::Tower);
                crash.height = Some(CRASH_HEIGHT);
                crash.id = fragment.tower_id;
                crash.size = Some(CRASH_SIZE);
                crash.color = Some(fragment.color);
                simulation.events.push(crash);
            }
            let room = |count: usize| {
                count > 1 && simulation.fragments.len() + count - 1 <= simulation.max_fragments
            };
            let pieces = layout.landing(&parts);
            let split = if room(pieces.len()) {
                Some(pieces)
            } else {
                Some(tower_sections(&parts)).filter(|sections| room(sections.len()))
            };
            if let Some(split) = split {
                simulation.break_tower_fragment(index, split);
                continue;
            }
            simulation.fragments[index]
                .tower_landing
                .get_or_insert_default()
                .landed = true;
        }
        let elapsed = simulation.elapsed;
        let fragment = &mut simulation.fragments[index];
        let body = &mut simulation.world.bodies[handle];
        let landing = fragment.tower_landing.get_or_insert_default();
        landing.velocity = now;
        let can_rest = landing.landed || parts.len() == 1;
        if !landing.rested && (can_rest || age > REST_AFTER) {
            let still = body.linvel().length() < REST_SPEED && body.angvel().length() < REST_SPIN;
            match (still, landing.still_since) {
                (false, _) => landing.still_since = None,
                (true, None) => landing.still_since = Some(elapsed),
                _ => {}
            }
            let held = landing
                .still_since
                .is_some_and(|since| elapsed - since >= REST_HOLD);
            // A landed piece has done its rocking within a moment of the landing: it
            // rests then, still or not, and no longer turns.
            if can_rest && (held || (landing.landed && age > LANDED_REST)) {
                // It neither turns nor rises any more: shoved, it slides. The pile clears
                // from the top, so nothing it lies on goes from under it first.
                landing.rested = true;
                body.lock_rotations(true, false);
                body.set_enabled_translations(true, false, true, false);
                body.set_angvel(rapier3d::prelude::Vector::ZERO, false);
                let mut velocity = body.linvel();
                velocity.y = 0.0;
                body.set_linvel(velocity, false);
                // The pile clears from the top down, so nothing fades through what
                // lies over it.
                let height = f64::from(body.center_of_mass().y).max(0.0);
                fragment.life = (fragment.life - height * CLEAR_TOP_FIRST)
                    .max(DEBRIS_CLEANUP_SECONDS + CLEAR_TOP_FIRST);
            }
        }
        let settled = landing.rested || (landing.landed && age > LANDED_SETTLE);
        let (linear, angular) = house_debris_damping(if settled { f64::INFINITY } else { age });
        if body.linear_damping() != linear {
            body.set_linear_damping(linear);
            body.set_angular_damping(angular);
        }
        index += 1;
    }
}

/// Whether any of `body`'s colliders touches the ground.
fn on_ground(simulation: &Simulation, body: RigidBodyHandle) -> bool {
    simulation.world.bodies[body].colliders().iter().any(|&collider| {
        simulation.world.contact_pairs_with(collider).any(|pair| {
            let other = if pair.collider1 == collider {
                pair.collider2
            } else {
                pair.collider1
            };
            pair.has_any_active_contact()
                && simulation.world.colliders.get(other).is_some_and(|other| {
                    other.collision_groups().memberships.bits() & GROUND_MEMBERSHIP != 0
                })
        })
    })
}

/// The ground's collision membership ([`group::GROUND`]).
const GROUND_MEMBERSHIP: u32 = group::GROUND >> 16;

/// Falling bodies (a tower's or a cottage's) landing hard on a tower's standing parts
/// break them: a standing part takes [`CRUSH_DAMAGE`] for every unit of impulse they
/// struck it with this tick, past their weight. Parts are taken in tower, then part
/// order: no random draw.
pub fn crush_tower_parts(simulation: &mut Simulation, contacts: &[ContactForce]) {
    let mut struck: Vec<((usize, usize), f64, Point3)> = Vec::new();
    let mut crushed: Vec<(usize, f64, Point3, Vec2)> = Vec::new();
    for contact in contacts {
        for (falling, standing) in [
            (contact.collider1, contact.collider2),
            (contact.collider2, contact.collider1),
        ] {
            // A tower's falling body landing on timber or a crate breaks it as it would a
            // standing part: the tower comes down through it.
            if let Some(&cover) = simulation.cover_by_collider.get(&standing)
                && matches!(simulation.covers[cover].kind, CoverKind::Timber | CoverKind::Cargo)
                && simulation.covers[cover].alive
                && let Some(body) = simulation
                    .world
                    .colliders
                    .get(falling)
                    .and_then(|collider| collider.parent())
                && simulation
                    .fragments
                    .iter()
                    .any(|fragment| fragment.body == body && fragment.tower_parts.is_some())
            {
                let impulse = f64::from(contact.total_force_magnitude) * STEP;
                let rigid_body = &simulation.world.bodies[body];
                let at = from_vector(rigid_body.center_of_mass());
                let moving = from_vector(rigid_body.linvel());
                let along = Vec2::new(moving.x, moving.z);
                match crushed.iter_mut().find(|(key, ..)| *key == cover) {
                    Some((_, total, ..)) => *total += impulse,
                    None => crushed.push((cover, impulse, at, along)),
                }
                continue;
            }
            let Some(part) = simulation.tower_part(standing) else {
                continue;
            };
            let Some(body) = simulation
                .world
                .colliders
                .get(falling)
                .and_then(|collider| collider.parent())
            else {
                continue;
            };
            if !simulation.fragments.iter().any(|fragment| {
                fragment.body == body
                    && (fragment.tower_parts.is_some() || fragment.house_parts.is_some())
            }) {
                continue;
            }
            let impulse = f64::from(contact.total_force_magnitude) * STEP;
            let from = from_vector(simulation.world.bodies[body].center_of_mass());
            match struck.iter_mut().find(|(key, _, _)| *key == part) {
                Some((_, total, _)) => *total += impulse,
                None => struck.push((part, impulse, from)),
            }
        }
    }
    struck.sort_by_key(|((cover, part), _, _)| (*cover, *part));
    for ((cover, part), impulse, from) in struck {
        let c = &simulation.covers[cover];
        let center = tower_structure().parts[part].center;
        let center = Point3::new(c.x + center.x, center.y, c.z + center.z);
        let along = Vec2::new(center.x - from.x, center.z - from.z);
        simulation.strike_tower(cover, part, impulse * CRUSH_DAMAGE, false, center, along, None);
    }
    crushed.sort_by_key(|(cover, ..)| *cover);
    for (cover, impulse, at, along) in crushed {
        let length = along.x.hypot(along.z);
        if length > 1e-6 {
            simulation.covers[cover].kick = Some(Vec2::new(along.x / length, along.z / length));
        }
        let point = Point3::new(at.x, 1.0, at.z);
        simulation.damage_cover(
            cover,
            impulse * CRUSH_DAMAGE,
            NOBODY,
            Team::Blue,
            None,
            Some(point),
        );
    }
}

/// The world position of watchtower `cover`'s part `index`'s middle.
pub fn tower_part_center(cover: &Cover, index: usize) -> Option<Point3> {
    cover.tower.as_ref()?;
    let center = tower_structure().parts.get(index)?.center;
    Some(Point3::new(cover.x + center.x, center.y, cover.z + center.z))
}

/// Watchtower `cover`'s part `index` as a world box (min, max).
pub fn tower_part_box(cover: &Cover, index: usize) -> Option<(Point3, Point3)> {
    cover.tower.as_ref()?;
    let part = tower_structure().parts.get(index)?;
    Some((
        Point3::new(cover.x + part.min.x, part.min.y, cover.z + part.min.z),
        Point3::new(cover.x + part.max.x, part.max.y, cover.z + part.max.z),
    ))
}
