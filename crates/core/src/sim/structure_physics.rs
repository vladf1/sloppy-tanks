//! A structure in play ([`super::structure`]): its standing parts as colliders on its
//! cover's fixed body, shells and blasts breaking the parts they reach, and what no
//! longer stands coming down as rigid bodies of its parts.
//!
//! A broken part is gone at once (a `Shatter` event throws its splinters). What can no
//! longer stand comes down in the groups still fastened together, each one compound body
//! ([`Structure::falls`]): a group that tipped over turns about the edge it tipped over,
//! one that lost its hold slumps out from the structure's middle, and one a blast tore
//! loose is thrown away from it. A body that lands hard breaks into the pieces its
//! structure breaks into ([`Structure::landing`], coarser when the debris budget is
//! short); a mortared body bursts into bricks. Each landed piece is laid down off a thin
//! edge, slides off the remains it would lie up on, and once it has lain still a moment
//! comes to rest for good, gripping the pile; the pile clears from the top. A fallen piece
//! a shell or a blast reaches breaks where it is struck. Falling bodies break the
//! standing parts they land on hard. What still stands stays standing; navigation sees the
//! parts that block the ground as they go.
//!
//! No random draw: every variation comes from the structure's parts and where it was hit.

use std::sync::{Arc, OnceLock};

use glam::{DVec2, DVec3};
use rapier3d::prelude::{
    ActiveEvents, ColliderBuilder, ColliderHandle, Pose, RigidBodyBuilder, RigidBodyHandle,
    Rotation, SharedShape, Vector,
};

use super::combat_rules::{BlastFalloff, StructureBlast};
use super::data::{STEP, group};
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{blast_loose_debris, debris_material, track_debris_contacts};
use super::math::{Point3, Vec2};
use super::navigation::Footprint;
use super::physics::{ContactForce, from_vector, interaction_groups, query_filter, vector};
use super::simulation::Simulation;
use super::structure::{Breaking, Contact, Lie, PartSet, PartShape, Structure};
use super::types::{Cover, CoverKind, Fragment, SimEvent, SimEventType, Team};

/// A shell fused on striking a structure flies on this far (m) before it bursts, so an
/// HE round through a wall goes off in the room (or the hold) behind it.
pub const FUSE_DISTANCE: f64 = 1.0;
/// An HE shell's burst: its reach (m) over the parts round it; the damage falls off to
/// nothing at the edge.
pub const HE_BLAST_RADIUS: f64 = 1.4;
/// Blasts from outside (rockets, mines, drums): the share of their reach over tanks that
/// they reach over a structure's parts.
pub const STRUCTURE_BLAST_REACH: f64 = 0.5;
/// How high (m) a blast on the ground meets a structure.
pub const BLAST_HEIGHT: f64 = 1.0;
/// A part shielded from a blast by another standing part takes this share of it.
const SHIELDED: f64 = 0.35;
/// A blast shatters the parts within this far (m) of it; parts farther off that it
/// breaks come loose whole and fall, so splinters fly only where it went off.
pub const BLAST_CORE: f64 = 0.5;
/// A falling body's colliders sit this far (m) inside its parts' faces, clear of the
/// standing parts they were built against.
const SHRINK: f64 = 0.03;
/// How big (m) a burst of a fallen stack's bricks reads as, and the first knock (m/s
/// change in a tick) that bursts it: mortared brick does not land standing.
const BRICKS_SIZE: f64 = 2.5;
const BRICKS_LANDING: f64 = 1.0;
/// A part giving way slumps out from the structure's middle at this speed (m/s), and a
/// wall alone tips out over its foot at this rate (rad/s), so nothing hangs level in the
/// air where it stood.
const SLUMP_SPEED: f64 = 0.6;
const SLUMP_TIP: f64 = 0.6;
/// A group tipping over starts turning at this rate (rad/s) about the edge it tips over;
/// its own weight does the rest.
const TIP_RATE: f64 = 0.35;
/// What broke this near (m, across the ground) a structure's middle names no side for a
/// group to tip toward; it tips away from the blast instead.
const TOWARD_CENTRE: f64 = 0.5;
/// How fast (m/s) a blast throws the parts it tears loose away from it.
const BLAST_THROW: f64 = 2.5;
/// A loose piece lying (slower than this, m/s: sliding or still) with its middle this
/// high (m) up on its structure's standing parts, this long (s) after it came loose, is
/// wedged there.
const WEDGE_SPEED: f64 = 1.5;
const WEDGE_HEIGHT: f64 = 1.0;
/// A loose piece whose lowest point is this low (m) stands on the ground, not up on the
/// remains.
const WEDGE_FOOT: f64 = 0.3;
const WEDGE_AFTER: f64 = 0.4;
/// How fast (m/s) a wedged piece is shoved off what it lies on, and how long (s) it is
/// shoved before it drops through instead.
const WEDGE_PUSH: f64 = 1.2;
const WEDGE_GIVE_UP: f64 = 1.0;
/// The damage each unit of impulse a falling body strikes a standing part with does.
const CRUSH_DAMAGE: f64 = 6.0;
/// A falling body strikes with at most its momentum times this (a bounce back).
const CRUSH_BOUNCE: f64 = 1.5;
/// A body moving slower than this (m/s) where it meets a standing part presses on it and
/// strikes nothing: a fallen floor squeezed against a chimney stack, creeping, once wore
/// it down tick after tick, and a strip of wall tipping off its stubs ground them away as
/// it turned on their edge.
const CRUSH_SPEED: f64 = 1.0;
/// Contact force per unit of a falling body's mass that counts as striking what it lands
/// on rather than resting its weight there.
const CRUSH_FORCE_PER_MASS: f64 = 40.0;
/// A loose body broken by a shell or a blast near it throws its pieces this fast (m/s)
/// out from the burst and up, for a standard shell's damage, less for a big piece.
const LOOSE_THROW_OUT: f64 = 1.6;
const LOOSE_THROW_UP: f64 = 2.4;
/// A blast breaks the loose parts within this share of its reach (at least
/// [`BLAST_CORE`], at most [`LOOSE_CORE_MAX`] m) and throws the others in reach.
const LOOSE_CORE_SHARE: f64 = 0.35;
/// How much further than its reach on what stands a blast throws loose pieces.
const LOOSE_REACH: f64 = 1.8;
const LOOSE_CORE_MAX: f64 = 1.0;
/// A blast's damage that throws loose parts at [`LOOSE_THROW_OUT`]; more throws them
/// harder, up to [`LOOSE_THROW_MOST`] times.
const LOOSE_THROW_DAMAGE: f64 = 25.0;
const LOOSE_THROW_MOST: f64 = 1.8;
/// The dust a structure's load raises where it crashes down: its reach (m) and height.
const CRASH_SIZE: f64 = 7.0;
const CRASH_HEIGHT: f64 = 1.5;
/// The share of the falling body's spin each piece keeps when it breaks up on landing.
const LANDING_SPIN: f32 = 0.5;
/// The pieces of a body breaking where it lies still tip at this rate (rad/s) about
/// their break line, alternately, so the break shows as a crease, not a seam.
const CREASE_SPIN: f64 = 1.2;
/// A falling body slower than this (m/s and rad/s) for [`REST_HOLD`] seconds, no sooner
/// than [`REST_AFTER`] after it came loose, is jammed where it lies: it breaks up there
/// as if it had landed.
const REST_SPEED: f32 = 0.3;
const REST_SPIN: f32 = 0.6;
const REST_HOLD: f64 = 0.2;
const REST_AFTER: f64 = 1.0;
/// A landed piece that over [`REST_WINDOW`] seconds has moved less than [`REST_DRIFT`]
/// (m) and turned less than [`REST_TURN`] (rad) has come to rest: it can no longer turn,
/// so it stays as it lies until it clears. Judged by where it got to rather than by its
/// speed at each tick, which on some machines never stays low for long on a piece propped
/// on the pile or sliding slowly down it.
const REST_WINDOW: f64 = 0.3;
const REST_DRIFT: f32 = 0.06;
const REST_TURN: f32 = 0.06;
/// A landed piece still sliding or rocking this long (s) after its landing (shoved along
/// by the pile, levered up by a piece landing on it) drags as if wedged in the pile, with
/// this linear and angular damping, and rests once it is slower than this (m/s and
/// rad/s): slowed rather than stopped dead, and a piece still falling or tipping over is
/// faster, so it is never frozen part way down.
const CREEP_AFTER: f64 = 1.6;
const CREEP_DAMPING: (f32, f32) = (6.0, 6.0);
const CREEP_SPEED: f32 = 0.5;
const CREEP_SPIN: f32 = 0.8;
/// A landed piece is damped toward rest this long (s) after the landing, so its rocking
/// is over within about a second.
const LANDED_SETTLE: f64 = 0.8;
/// A resting piece's linear and angular damping: struck, it is barely knocked over.
const RESTED_DAMPING: (f32, f32) = (2.0, 12.0);
/// How long (s) before a resting piece starts to fade it takes what it holds up along
/// ([`clear_together`]): time for that to reach up a pile several pieces deep.
const CLEAR_AHEAD: f64 = 0.25;
/// A contact whose normal out of a clearing piece points further down than this (its
/// vertical share) is with a piece it lies on, which it does not take along.
const BENEATH: f32 = 0.5;
/// A resting piece's grip on what it lies on.
const RESTED_FRICTION: f32 = 2.5;
/// Seconds earlier a resting piece clears for every metre its middle lies up the pile.
const CLEAR_TOP_FIRST: f64 = 1.2;
/// A loose part's linear and angular damping while it falls, and after this long (s)
/// the damping toward rest that settles a pile rather than letting it creep.
const FALLING_DAMPING: (f32, f32) = (0.1, 0.3);
const SETTLE_AFTER: f64 = 2.0;
const SETTLED_DAMPING: (f32, f32) = (2.0, 3.0);
/// A loose piece come to rest this long (s) after it came loose, slower than this (m/s),
/// on a thin edge (its face's normal's rise under [`LEANING`], about 45 degrees; a stick
/// on its end, its length's rise over [`ON_END`]) and not propped on the remains, is laid
/// down: turned over the way it leans at [`LAY_RATE`] (rad/s) about its foot until its
/// face's rise reaches [`LAID`] (or its length's falls under [`LAID_STICK`]).
const LAY_AFTER: f64 = 1.0;
const LAY_SLOWER: f32 = 0.5;
const LEANING: f32 = 0.7;
const ON_END: f32 = 0.7;
const LAY_RATE: f32 = 2.5;
const LAID: f32 = 0.93;
const LAID_STICK: f32 = 0.35;
/// A piece leaning on what stands is laid down off it until its face's rise reaches this.
const LAID_OFF: f32 = 0.985;
/// The fastest (m/s) a loose piece moves up and across, and turns (rad/s): a blast's
/// throw at its strongest and a little more. Falling is left to gravity. The solver
/// pushing apart parts that came loose overlapping would fling them over the roofs.
const MAX_RISE: f32 = 4.5;
const MAX_ACROSS: f32 = 6.0;
const MAX_TURN: f32 = 6.0;
/// The most splinter events one break sends: a blast breaking many parts throws them
/// from its heaviest few.
const SHATTER_EVENTS: usize = 6;
/// Nobody: the owner a falling body's damage to what it lands on is credited to.
const NOBODY: (u32, Team) = (u32::MAX, Team::Blue);

/// Hold a fallen piece at rest for good: it no longer turns, is damped hard and grips
/// what it lies on (the host's, and a client's predicted world's as the host has it).
pub fn rest_body(world: &mut rapier3d::prelude::PhysicsWorld, body: RigidBodyHandle) {
    let rigid_body = &mut world.bodies[body];
    rigid_body.lock_rotations(true, false);
    rigid_body.set_linear_damping(RESTED_DAMPING.0);
    rigid_body.set_angular_damping(RESTED_DAMPING.1);
    for &collider in world.bodies[body].colliders() {
        world.colliders[collider].set_friction(RESTED_FRICTION);
    }
}

/// Set every collider of `body` to collide as `groups` (a group from [`group`]).
fn set_body_groups(world: &mut rapier3d::prelude::PhysicsWorld, body: RigidBodyHandle, groups: u32) {
    for &collider in world.bodies[body].colliders() {
        set_debris_groups(&mut world.colliders[collider], groups);
    }
}

/// Set a fallen part's collider's groups, unless it is a conifer's bough, which only
/// ever meets the ground ([`group::BOUGH_DEBRIS`]).
fn set_debris_groups(collider: &mut rapier3d::prelude::Collider, groups: u32) {
    if collider.collision_groups() != interaction_groups(group::BOUGH_DEBRIS) {
        collider.set_collision_groups(interaction_groups(groups));
    }
}

/// Whether a structure's fallen body passes through what stands of the structures
/// ([`group::STRUCTURE_DEBRIS_THROUGH`]): a timber knocked out, a piece dropping off the
/// remains it was wedged on.
pub fn passes_through(world: &rapier3d::prelude::PhysicsWorld, body: RigidBodyHandle) -> bool {
    world.bodies[body].colliders().iter().any(|&collider| {
        world.colliders[collider].collision_groups()
            == interaction_groups(group::STRUCTURE_DEBRIS_THROUGH)
    })
}

/// Let a structure's fallen body in a client's predicted world pass through what stands,
/// or meet it again, as the host's does ([`passes_through`]).
pub fn set_passing_through(
    world: &mut rapier3d::prelude::PhysicsWorld,
    body: RigidBodyHandle,
    through: bool,
) {
    let groups = if through {
        group::STRUCTURE_DEBRIS_THROUGH
    } else {
        group::STRUCTURE_DEBRIS
    };
    set_body_groups(world, body, groups);
}

/// Let a rested piece turn again, thrown out of its rest by a blast (the host's, and a
/// client's predicted world's as the host has it).
pub fn unrest_body(world: &mut rapier3d::prelude::PhysicsWorld, body: RigidBodyHandle) {
    world.bodies[body].lock_rotations(false, true);
}

/// How the host damps a structure's fallen piece this tick, replicated so a client's
/// predicted copy moves as the host's does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Damping {
    /// Falling, by its age ([`falling_damping`]).
    #[default]
    Falling,
    /// Sliding off the remains or being laid down: falling's damping.
    Sliding,
    /// Landed a moment ago.
    Settled,
    /// Barely moving well after its landing.
    Creeping,
    /// At rest for good.
    Rested,
}

impl Damping {
    /// Its linear and angular damping for a piece `age` seconds after it came loose.
    pub fn values(self, age: f64) -> (f32, f32) {
        match self {
            Damping::Falling => falling_damping(age),
            Damping::Sliding => FALLING_DAMPING,
            Damping::Settled => SETTLED_DAMPING,
            Damping::Creeping => CREEP_DAMPING,
            Damping::Rested => RESTED_DAMPING,
        }
    }
}

/// The linear and angular damping of a falling body `age` seconds after it came loose.
pub fn falling_damping(age: f64) -> (f32, f32) {
    if age > SETTLE_AFTER {
        SETTLED_DAMPING
    } else {
        FALLING_DAMPING
    }
}

/// A structure as it stands: its layout, the parts broken or fallen from it, the damage
/// its parts have taken and its standing parts' colliders.
#[derive(Clone, Debug)]
pub struct StructureState {
    pub layout: Arc<Structure>,
    pub gone: PartSet,
    pub damage: Vec<f64>,
    /// The parts that have taken damage, kept as damage lands: every frame's scene reads
    /// them for every structure, mostly intact.
    hurt: PartSet,
    pub colliders: Vec<Option<ColliderHandle>>,
    /// Counted as levelled ([`super::structure::Levelling`]).
    pub levelled: bool,
}

impl StructureState {
    pub fn new(layout: Arc<Structure>) -> Self {
        let count = layout.parts.len();
        Self {
            layout,
            gone: PartSet::EMPTY,
            damage: vec![0.0; count],
            hurt: PartSet::EMPTY,
            colliders: vec![None; count],
            levelled: false,
        }
    }

    pub fn standing(&self, part: usize) -> bool {
        !self.gone.contains(part)
    }

    /// The parts that have taken damage, standing or not: a client's own-shot prediction
    /// reads the standing ones, and a watchtower's model scars a struck leg and leaves a
    /// tall stub of a shot-out one.
    pub fn cracked(&self) -> PartSet {
        self.hurt
    }

    /// Part `index` takes `amount` of damage.
    pub fn take(&mut self, index: usize, amount: f64) {
        self.damage[index] += amount;
        if self.damage[index] > 0.0 {
            self.hurt.insert(index);
        }
    }

    /// The share of the structure's breakable parts still standing; for one whose health
    /// follows a part ([`Structure::health_part`]), that part's strength left.
    pub fn standing_share(&self) -> f64 {
        if let Some(index) = self.layout.health_part {
            let strength = self.layout.parts[index].strength;
            return if self.standing(index) {
                (1.0 - self.damage[index] / strength).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        let parts = &self.layout.parts;
        let standing = (0..parts.len())
            .filter(|&index| !parts[index].footing && self.standing(index))
            .count();
        standing as f64 / self.layout.breakable().max(1) as f64
    }

    /// Whether what is gone levels it.
    fn levels(&self) -> bool {
        let rule = self.layout.levelling;
        (!rule.keys.is_empty() && rule.keys.iter().all(|index| self.gone.contains(index)))
            || self.standing_share() < rule.share
    }
}

/// How a fallen body of a structure's parts is landing and settling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StructureDebris {
    /// The structure's cover, by index.
    pub cover: usize,
    pub parts: PartSet,
    /// Its velocity last tick, to tell a hard landing.
    pub velocity: Point3,
    /// It has landed (or broke off a body that did): it never breaks up again, only
    /// where something strikes it.
    pub landed: bool,
    /// The match time (s) it landed at.
    pub landed_at: f64,
    pub still_since: Option<f64>,
    /// When its rest window opened, and where its middle was and how it was turned then.
    pub window: Option<(f64, glam::Vec3, glam::Quat)>,
    pub rested: bool,
    /// How it is damped this tick.
    pub damping: Damping,
    /// A timber knocked out whole passes through what stands until then.
    pub clear_at: Option<f64>,
    /// It broke a part of a structure it came down on since the last tick: its landing,
    /// however gently it came to a stop there.
    pub struck: bool,
    /// When it came to rest up on its structure's remains, while it is shoved off them.
    pub wedged: Option<f64>,
    /// Being laid down off the thin edge it came to rest on.
    pub laying: bool,
}

/// A part's collider relative to its structure's ground centre (world axes), its shapes
/// shrunk by `inset` metres, marked with the part's index.
pub fn part_collider(layout: &Structure, index: usize, inset: f64) -> ColliderBuilder {
    let (pose, shape) = layout.shapes.shape(layout, index, inset);
    ColliderBuilder::new(shape).position(pose).user_data(index as u128)
}

/// A structure's parts' collider shapes, each built the first time a collider asks for it
/// and shared by every collider made of it after: a part's shape (a hull's quickhull
/// above all) never changes, so a falling body, each piece it lands in and a client's
/// predicted copy all take theirs from here. One store for each inset the colliders use:
/// a fallen part's [`SHRINK`] and a standing part's seam.
#[derive(Default)]
pub struct ShapeCache {
    debris: OnceLock<PartShapes>,
    standing: OnceLock<PartShapes>,
}

/// A slot for each part's and landing piece's shape and pose, filled when first asked.
type PartShapes = Box<[OnceLock<(Pose, SharedShape)>]>;

impl ShapeCache {
    /// Part (or landing piece) `index`'s shape and pose at `inset`.
    fn shape(&self, layout: &Structure, index: usize, inset: f64) -> (Pose, SharedShape) {
        let store = if inset == SHRINK {
            Some(&self.debris)
        } else if inset == -layout.debris.seam {
            Some(&self.standing)
        } else {
            None
        };
        let slot = store.and_then(|store| {
            store
                .get_or_init(|| {
                    (0..layout.parts.len() + layout.pieces.len())
                        .map(|_| OnceLock::new())
                        .collect()
                })
                .get(index)
        });
        match slot {
            Some(slot) => slot.get_or_init(|| part_shape(layout, index, inset)).clone(),
            None => part_shape(layout, index, inset),
        }
    }
}

/// A copy of a structure starts with no shapes: it may be changed.
impl Clone for ShapeCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// The shapes follow from the parts, so they never tell two structures apart.
impl PartialEq for ShapeCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for ShapeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShapeCache")
    }
}

/// [`part_collider`]'s shape and its pose, built.
fn part_shape(layout: &Structure, index: usize, inset: f64) -> (Pose, SharedShape) {
    let part = layout.part(index);
    let turn = Rotation::from_rotation_y(layout.yaw as f32);
    let mut shapes: Vec<(Pose, SharedShape)> = part
        .shapes
        .iter()
        .map(|shape| match shape {
            PartShape::Box {
                center,
                half,
                rotation,
            } => {
                let half = (*half - DVec3::splat(inset)).max(DVec3::splat(0.01));
                if *rotation == glam::DQuat::IDENTITY {
                    // Square to the build frame: square to the world, a quarter turn
                    // swapping its sides, so it stays an axis-aligned box.
                    let (a, b) = (layout.to_world(*center - half), layout.to_world(*center + half));
                    let (min, max) = (a.min(b), a.max(b));
                    let (half, center) = ((max - min) / 2.0, (min + max) / 2.0);
                    (
                        Pose::from_translation(vector(center.x, center.y, center.z)),
                        SharedShape::cuboid(half.x as f32, half.y as f32, half.z as f32),
                    )
                } else {
                    let center = layout.to_world(*center);
                    let rotation = Rotation::from_xyzw(
                        rotation.x as f32,
                        rotation.y as f32,
                        rotation.z as f32,
                        rotation.w as f32,
                    );
                    (
                        Pose::from_parts(vector(center.x, center.y, center.z), turn * rotation),
                        SharedShape::cuboid(half.x as f32, half.y as f32, half.z as f32),
                    )
                }
            }
            PartShape::Hull(points) => {
                let center = points.iter().copied().sum::<DVec3>() / points.len() as f64;
                let inset: Vec<_> = points
                    .iter()
                    .map(|&p| {
                        let p = layout.to_world(p + (center - p).normalize_or_zero() * inset);
                        vector(p.x, p.y, p.z)
                    })
                    .collect();
                let shape = SharedShape::convex_hull(&inset).unwrap_or_else(|| {
                    let half = ((part.max - part.min) / 2.0).max(DVec3::splat(0.02));
                    SharedShape::cuboid(half.x as f32, half.y as f32, half.z as f32)
                });
                (Pose::IDENTITY, shape)
            }
        })
        .collect();
    if shapes.len() == 1 {
        shapes.remove(0)
    } else {
        (Pose::IDENTITY, SharedShape::compound(shapes))
    }
}

/// The colliders of a body of a structure's loose `parts`, relative to its ground centre
/// where they fell from: the host's, and a client's for its predicted world. Timber this
/// size lands dead: no bounce lifts a piece off the ground.
pub fn debris_colliders(layout: &Structure, parts: &PartSet) -> Vec<ColliderBuilder> {
    parts.iter().map(|index| debris_collider(layout, index)).collect()
}

/// [`debris_colliders`]' collider for part `index`.
fn debris_collider(layout: &Structure, index: usize) -> ColliderBuilder {
    let part = layout.part(index);
    let surface = debris_material(part.material.surface);
    let groups = if part.contact == Contact::Nothing {
        group::BOUGH_DEBRIS
    } else {
        group::STRUCTURE_DEBRIS
    };
    part_collider(layout, index, SHRINK)
        .collision_groups(interaction_groups(groups))
        .mass(part.mass.max(0.01) as f32)
        .friction(surface.friction as f32)
        .restitution(0.0)
}

/// The colliders of a structure's standing parts (those not in `gone`), in part order,
/// on a fixed body at its ground centre.
pub fn standing_colliders(layout: &Structure, gone: &PartSet) -> Vec<ColliderBuilder> {
    if layout.whole && gone.is_empty() {
        return vec![whole_collider(layout)];
    }
    (0..layout.parts.len())
        .filter(|&index| !gone.contains(index))
        .map(|index| standing_collider(layout, index))
        .collect()
}

/// The user data of an intact structure's one collider ([`Structure::whole`]).
pub const WHOLE: u128 = u128::MAX;

/// An intact structure's box as one collider.
fn whole_collider(layout: &Structure) -> ColliderBuilder {
    let (min, max) = layout.bounds();
    let inset = DVec3::splat(layout.debris.seam);
    let (a, b) = (layout.to_world(min + inset), layout.to_world(max - inset));
    let (min, max) = (a.min(b), a.max(b));
    let (half, center) = ((max - min) / 2.0, (min + max) / 2.0);
    ColliderBuilder::cuboid(half.x as f32, half.y as f32, half.z as f32)
        .translation(vector(center.x, center.y, center.z))
        .user_data(WHOLE)
        .collision_groups(interaction_groups(group::COVER))
        .friction(0.4)
}

fn standing_collider(layout: &Structure, index: usize) -> ColliderBuilder {
    let groups = match layout.parts[index].contact {
        Contact::All => group::COVER,
        Contact::Tanks => group::STUMP_CONTACT,
        Contact::Shells => group::SHELL_COVER,
        Contact::Nothing => 0,
    };
    part_collider(layout, index, -layout.debris.seam)
        .collision_groups(interaction_groups(groups))
        .friction(0.4)
}

/// The world boxes of a structure's standing parts that block tanks on the ground, or
/// its whole box while none of them is gone (a sealed cottage's rooms are no way in).
pub fn ground_footprints(cover: &Cover, out: &mut Vec<Footprint>) {
    let Some(state) = &cover.structure else {
        return;
    };
    let layout = &state.layout;
    let open = (0..layout.parts.len())
        .any(|index| state.gone.contains(index) && layout.parts[index].blocks_ground);
    if !open {
        if state.gone.len() < layout.parts.len() {
            out.push(Footprint::from(cover));
        }
        return;
    }
    for (index, part) in layout.parts.iter().enumerate() {
        if state.gone.contains(index) || !part.blocks_ground {
            continue;
        }
        let (min, max) = layout.world_box(index);
        out.push(Footprint {
            x: cover.x + (min.x + max.x) / 2.0,
            z: cover.z + (min.z + max.z) / 2.0,
            w: max.x - min.x,
            d: max.z - min.z,
        });
    }
}

/// Where to shoot structure `cover` from `from` to open it: the nearest standing part a
/// level shell still breaks that blocks the way (a timber wall's post or beam at shell
/// height), and for a structure carrying a load only the anchors it stands in (a
/// watchtower falls when a leg goes); `None` once none is left.
pub fn breach_point(cover: &Cover, from: Vec2) -> Option<Vec2> {
    let state = cover.structure.as_ref()?;
    let layout = &state.layout;
    if !layout.breachable {
        return None;
    }
    let loaded = layout.parts.iter().any(|part| part.load);
    (0..layout.parts.len())
        .filter(|&index| {
            let part = &layout.parts[index];
            state.standing(index)
                && part.strength.is_finite()
                && if loaded {
                    part.anchor
                } else {
                    part.blocks_ground && part.min.y <= BLAST_HEIGHT && part.max.y >= BLAST_HEIGHT
                }
        })
        // A point of one of its shapes at shell height: a crate wall's frame is battens
        // round an open middle, which a shell at its box's middle passes through.
        .filter_map(|index| {
            let part = &layout.parts[index];
            let at = if loaded { part.mass_center } else { part.point_at_height(BLAST_HEIGHT)? };
            let center = layout.to_world(at);
            Some(Vec2::new(cover.x + center.x, cover.z + center.z))
        })
        .min_by(|a, b| {
            let gap = |p: &Vec2| (p.x - from.x).hypot(p.z - from.z);
            gap(a).total_cmp(&gap(b))
        })
}

/// The paint part `index` of `cover`'s structure splinters in.
fn part_paint(cover: &Cover, layout: &Structure, index: usize) -> u32 {
    layout.part(index).paint.unwrap_or(cover.color)
}

impl Simulation {
    /// The part of a structure behind `collider` (from its user data), if it is one.
    pub fn structure_part(&self, collider: ColliderHandle, at: Point3) -> Option<(usize, usize)> {
        let &cover = self.cover_by_collider.get(&collider)?;
        let c = &self.covers[cover];
        let state = c.structure.as_ref()?;
        let data = self.world.colliders.get(collider)?.user_data;
        let part = if data == WHOLE {
            let local = state.layout.to_local(DVec3::new(at.x - c.x, at.y, at.z - c.z));
            state.layout.part_at(local, &state.gone)?
        } else {
            data as usize
        };
        Some((cover, state.layout.parts[part].struck_as.map_or(part, usize::from)))
    }

    /// The structure a standing collider belongs to.
    pub fn structure_cover(&self, collider: ColliderHandle) -> Option<usize> {
        let &cover = self.cover_by_collider.get(&collider)?;
        self.covers[cover].structure.as_ref().map(|_| cover)
    }

    /// Give intact structure `cover`, standing as one collider, a collider for each part
    /// not in `going`, before those go.
    fn split_whole(&mut self, cover: usize, going: &PartSet) {
        let body = self.covers[cover].body;
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        let Some(handle) = state.colliders.first().copied().flatten() else {
            return;
        };
        if self.world.colliders.get(handle).is_none_or(|collider| collider.user_data != WHOLE) {
            return;
        }
        state.colliders.iter_mut().for_each(|collider| *collider = None);
        let layout = state.layout.clone();
        // The parts go in before the whole comes out: a part taking the whole's freed
        // slot would share its broad-phase leaf, which the next step drops with the
        // whole.
        let whole = handle;
        for index in (0..layout.parts.len()).filter(|&index| !going.contains(index)) {
            let handle = self
                .world
                .insert_collider(standing_collider(&layout, index), Some(body));
            // Scene queries see a collider once the broad phase has it, which a step
            // does; the shells still flying this tick (one from the far side, an armour-
            // piercing round flying on) must meet the parts left standing, not the gap
            // the whole collider left. Only the new boxes go in, not a whole pass.
            let world = &mut self.world;
            let aabb = world.colliders[handle]
                .compute_broad_phase_aabb(&world.integration_parameters, &world.bodies);
            world
                .broad_phase
                .set_aabb(&world.integration_parameters, handle, aabb);
            self.cover_by_collider.insert(handle, cover);
            if let Some(state) = &mut self.covers[cover].structure {
                state.colliders[index] = Some(handle);
            }
        }
        self.cover_by_collider.remove(&whole);
        self.world.remove_collider(whole);
    }

    /// Hit structure `cover`'s `part` for `amount` at `point`, from `along` (the way the
    /// shell flew); `force` breaks it whatever its strength (an armour-piercing round
    /// through it). A part that never breaks takes nothing. Returns whether it broke.
    #[allow(clippy::too_many_arguments)]
    pub fn strike_structure(
        &mut self,
        cover: usize,
        part: usize,
        amount: f64,
        force: bool,
        point: Point3,
        along: Vec2,
        by: Option<(u32, Team)>,
    ) -> bool {
        let Some(state) = &mut self.covers[cover].structure else {
            return false;
        };
        let strength = state.layout.parts[part].strength;
        if !state.standing(part) || strength.is_infinite() {
            return false;
        }
        state.take(part, amount);
        self.follow_health(cover);
        let Some(state) = &self.covers[cover].structure else {
            return false;
        };
        if !force && state.damage[part] < strength {
            return false;
        }
        // A board tears its neighbours of the same stuff, nailed to the same frame, near
        // the hit along with it.
        let layout = state.layout.clone();
        let material = layout.parts[part].material;
        let (x, z) = (self.covers[cover].x, self.covers[cover].z);
        let Some(state) = &self.covers[cover].structure else {
            return false;
        };
        let local = layout.to_local(DVec3::new(point.x - x, point.y, point.z - z));
        let fastened = |index: usize| {
            let by = |index: usize| layout.supports[index].iter().map(|support| support.by);
            by(index).eq(by(part))
        };
        let torn = PartSet::from_indices((0..layout.parts.len()).filter(|&index| {
            index == part
                || (material.tear > 0.0
                    && state.standing(index)
                    && layout.parts[index].material == material
                    && fastened(index)
                    && layout.parts[index].shape_distance(local) <= material.tear)
        }));
        let from = Point3::new(point.x - along.x, point.y, point.z - along.z);
        self.break_structure(cover, torn, PartSet::EMPTY, Some(from), by);
        true
    }

    /// A burst at `point` damaging every structure's parts within `radius`, falling off
    /// to nothing at its edge and shielded by standing parts between, and breaking and
    /// throwing the loose debris round it.
    pub fn structure_blast(&mut self, point: Point3, radius: f64, damage: f64, by: Option<(u32, Team)>) {
        self.structure_blast_sparing(point, radius, damage, by, None);
    }

    /// [`Self::structure_blast`], leaving the structure `spared` (if any) alone.
    pub fn structure_blast_sparing(
        &mut self,
        point: Point3,
        radius: f64,
        damage: f64,
        by: Option<(u32, Team)>,
        spared: Option<usize>,
    ) {
        blast_loose_debris(self, point.planar(), radius, damage);
        blast_structure_debris(self, point, radius, damage);
        for cover in (0..self.covers.len()).filter(|&cover| Some(cover) != spared) {
            self.structure_blast_on(cover, point, radius, damage, by);
        }
    }

    /// [`Self::structure_blast`] on one structure, without the debris.
    pub fn structure_blast_on(
        &mut self,
        cover: usize,
        point: Point3,
        radius: f64,
        damage: f64,
        by: Option<(u32, Team)>,
    ) {
        let blast = StructureBlast {
            pressure: damage,
            falloff: BlastFalloff::Even,
            reach: radius,
            shatter: BLAST_CORE,
            throw: loose_throw(damage),
        };
        self.structure_blast_with(cover, point, blast, by);
    }

    /// A blast at `point` on structure `cover`'s standing parts, as `blast` says: its
    /// pressure falling off to nothing at its reach, shielded by standing parts between.
    pub fn structure_blast_with(
        &mut self,
        cover: usize,
        point: Point3,
        blast: StructureBlast,
        by: Option<(u32, Team)>,
    ) {
        let (radius, damage) = (blast.reach, blast.pressure);
        let c = &self.covers[cover];
        let Some(state) = &c.structure else {
            return;
        };
        // The structure's box in reach at all.
        let reach = Vec2::new(
            ((point.x - c.x).abs() - c.w / 2.0).max(0.0),
            ((point.z - c.z).abs() - c.d / 2.0).max(0.0),
        );
        if reach.x.hypot(reach.z) >= radius {
            return;
        }
        let layout = state.layout.clone();
        let local = layout.to_local(DVec3::new(point.x - c.x, point.y, point.z - c.z));
        let mut hits = std::mem::take(&mut self.scratch.structure_hits);
        hits.clear();
        for (index, part) in layout.parts.iter().enumerate() {
            if !state.standing(index) || part.strength.is_infinite() || part.distance(local) >= radius
            {
                continue;
            }
            // How far the burst is from the part itself: a sloping sheet's or a
            // diagonal's box reaches far past it.
            let gap = state.colliders[index]
                .and_then(|handle| self.world.colliders.get(handle))
                .filter(|collider| collider.user_data != WHOLE)
                .map_or(part.shape_distance(local), |collider| {
                    f64::from(collider.shape().distance_to_point(
                        collider.position(),
                        vector(point.x, point.y, point.z),
                        true,
                    ))
                });
            if gap >= radius {
                continue;
            }
            let mut share = blast.falloff.share(gap, radius);
            if gap > 0.05 && self.shielded(cover, index, point, local, gap) {
                share *= SHIELDED;
            }
            hits.push((index, damage * share, gap));
        }
        let state = self.covers[cover].structure.as_mut().expect("checked above");
        let (mut broken, mut loose) = (PartSet::EMPTY, PartSet::EMPTY);
        for &(index, amount, gap) in &hits {
            state.take(index, amount);
            if state.damage[index] >= layout.parts[index].strength {
                if gap < blast.shatter {
                    broken.insert(index);
                } else {
                    loose.insert(index);
                }
            }
        }
        self.scratch.structure_hits = hits;
        self.follow_health(cover);
        if !broken.is_empty() || !loose.is_empty() {
            self.break_structure(cover, broken, loose, Some(point), by);
        }
    }

    /// Structure `cover`'s hit points after its parts took damage, when they follow a
    /// part's ([`Structure::health_part`]): a tree's sinking as its butt is hurt.
    fn follow_health(&mut self, cover: usize) {
        let c = &mut self.covers[cover];
        if let Some(state) = &c.structure
            && state.layout.health_part.is_some()
        {
            c.hp = c.max_hp * state.standing_share();
        }
    }

    /// Every standing part of structure `cover` takes `amount`. The parts on the ground
    /// that it breaks shatter; what it breaks above them comes loose whole and falls.
    pub fn damage_every_part(&mut self, cover: usize, amount: f64, by: Option<(u32, Team)>) {
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        let (mut broken, mut loose) = (PartSet::EMPTY, PartSet::EMPTY);
        let layout = state.layout.clone();
        for index in 0..layout.parts.len() {
            let part = &layout.parts[index];
            if !state.standing(index) || part.strength.is_infinite() {
                continue;
            }
            state.take(index, amount);
            if state.damage[index] >= part.strength {
                if part.grounded || part.min.y < WEDGE_FOOT {
                    broken.insert(index);
                } else {
                    loose.insert(index);
                }
            }
        }
        self.follow_health(cover);
        if !broken.is_empty() || !loose.is_empty() {
            self.break_structure(cover, broken, loose, None, by);
        }
    }

    /// Whether another standing part lies between a blast at `point` (`local` in the
    /// structure's build frame) and the nearest point of `part`, `gap` away.
    fn shielded(&self, cover: usize, part: usize, point: Point3, local: DVec3, gap: f64) -> bool {
        let c = &self.covers[cover];
        let Some(state) = &c.structure else {
            return false;
        };
        let layout = &state.layout;
        let target = layout.to_world(local.clamp(layout.parts[part].min, layout.parts[part].max));
        let to = DVec3::new(c.x + target.x, target.y, c.z + target.z)
            - DVec3::new(point.x, point.y, point.z);
        let length = to.length();
        if length < 1e-6 {
            return false;
        }
        let direction = to / length;
        let ray = rapier3d::prelude::Ray::new(
            vector(point.x, point.y, point.z),
            vector(direction.x, direction.y, direction.z),
        );
        self.world
            .cast_ray(
                &ray,
                (gap - 0.05).max(0.0) as f32,
                false,
                query_filter(group::COVER_QUERY),
            )
            .is_some_and(|(collider, hit)| {
                let at = ray.point_at(hit);
                self.structure_part(collider, from_vector(at)) != Some((cover, part))
            })
    }

    /// Stand structure `cover` as a scene shows it, with exactly the parts not in `gone`
    /// (a model of a room refilled from what a client sees): their colliders come and go,
    /// no part falls and nothing is scored.
    pub fn set_structure_gone(&mut self, cover: usize, gone: PartSet) {
        let body = self.covers[cover].body;
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        if state.gone == gone || !self.world.bodies.contains(body) {
            return;
        }
        self.split_whole(cover, &gone);
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        let layout = state.layout.clone();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        let parts = layout.parts.len();
        for (index, collider) in state.colliders.iter_mut().enumerate().take(parts) {
            match (gone.contains(index), *collider) {
                (true, Some(handle)) => {
                    removed.push(handle);
                    *collider = None;
                }
                (false, None) => added.push(index),
                _ => {}
            }
        }
        state.gone = gone;
        state.levelled = state.levels();
        for handle in removed {
            self.cover_by_collider.remove(&handle);
            self.world.remove_collider(handle);
        }
        for index in added {
            let handle = self
                .world
                .insert_collider(standing_collider(&layout, index), Some(body));
            self.cover_by_collider.insert(handle, cover);
            if let Some(state) = &mut self.covers[cover].structure {
                state.colliders[index] = Some(handle);
            }
        }
        let region = Footprint::from(&self.covers[cover]);
        self.nav.rebuild(&self.covers, Some(region));
    }

    /// Break `broken` out of structure `cover` (each throws its splinters away from
    /// `from`) and tear `loose` free whole, then bring down whatever can no longer stand.
    pub fn break_structure(
        &mut self,
        cover: usize,
        broken_parts: PartSet,
        loose: PartSet,
        from: Option<Point3>,
        by: Option<(u32, Team)>,
    ) {
        let c = &self.covers[cover];
        let (x, z) = (c.x, c.z);
        let Some(state) = &c.structure else {
            return;
        };
        let layout = state.layout.clone();
        let breakable =
            |index: usize| state.standing(index) && layout.parts[index].strength.is_finite();
        // Brick does not splinter away where it stands, nor a heavy timber: a broken
        // chimney stack comes loose and bursts into bricks where it lands, a wall's beam
        // is knocked out of its slots whole and thrown.
        let brick = |index: usize| {
            matches!(
                layout.parts[index].material.breaking,
                Breaking::Bricks | Breaking::Dislodges
            )
        };
        let mut broken = PartSet::from_indices(
            broken_parts
                .iter()
                .filter(|&index| breakable(index) && !brick(index)),
        );
        let asked = broken_parts;
        let loose = PartSet::from_indices(
            loose
                .iter()
                .chain(asked.iter().filter(|&index| brick(index)))
                .filter(|&index| breakable(index) && !broken.contains(index)),
        );
        if broken.is_empty() && loose.is_empty() {
            return;
        }
        // A group whose weight lies on the line of what it stands on tips toward what
        // broke, or away from the blast that broke all round it.
        let toward = |broken: &PartSet| {
            let parts = broken.union(&loose);
            let toward = parts.iter().map(|index| layout.parts[index].middle()).sum::<DVec3>()
                / parts.len() as f64;
            match from {
                Some(from) if DVec2::new(toward.x, toward.z).length() < TOWARD_CENTRE => {
                    layout.to_local(DVec3::new(x - from.x, 0.0, z - from.z))
                }
                _ => toward,
            }
        };
        // Glass the break leaves without its seat does not drop out whole: it goes, and
        // its sash breaks up as it comes out. When none does, what gives way is what
        // comes down below: the same parts are out.
        let mut given_way = None;
        if layout
            .parts
            .iter()
            .any(|part| part.material.breaking == Breaking::Glass)
        {
            let after = state.gone.union(&broken).union(&loose);
            let way = layout.giving_way(&after, toward(&broken));
            let glass = PartSet::from_indices(way.falling.iter().filter(|&index| {
                layout.parts[index].material.breaking == Breaking::Glass
            }));
            if glass.is_empty() {
                given_way = Some(way);
            } else {
                broken = broken.union(&glass);
            }
        }
        // Splinters from the heaviest broken parts.
        let thrown = first_few(broken.iter(), |&a, &b| {
            layout.parts[b]
                .mass
                .total_cmp(&layout.parts[a].mass)
                .then(a.cmp(&b))
        });
        let c = &self.covers[cover];
        for &index in thrown.iter().flatten() {
            let part = &layout.parts[index];
            let center = layout.to_world(part.middle());
            let mut event = SimEvent::at(SimEventType::Shatter, x + center.x, z + center.z);
            event.height = Some(center.y);
            event.size = Some((part.max - part.min).max_element());
            event.color = Some(part_paint(c, &layout, index));
            event.cover_kind = Some(c.kind);
            event.id = Some(c.id);
            event.from = from;
            self.events.push(event);
        }
        // Splinters and dust off the struck face of a timber knocked out whole.
        for index in asked.iter().filter(|&index| {
            layout.parts[index].material.breaking == Breaking::Dislodges && loose.contains(index)
        }) {
            let center = layout.to_world(layout.parts[index].middle());
            let mut event = SimEvent::at(SimEventType::Shatter, x + center.x, z + center.z);
            event.height = Some(center.y);
            event.size = Some(DISLODGED_DUST);
            event.color = Some(part_paint(c, &layout, index));
            event.cover_kind = Some(c.kind);
            event.id = Some(c.id);
            event.from = from;
            self.events.push(event);
        }
        self.remove_parts(cover, &broken);
        let gone = self.covers[cover]
            .structure
            .as_ref()
            .map_or(PartSet::EMPTY, |state| state.gone);
        let way = given_way.unwrap_or_else(|| layout.giving_way(&gone.union(&loose), toward(&broken)));
        let mut left = broken;
        for fall in layout.falls_after(&gone, &loose, way) {
            self.remove_parts(cover, &fall.parts);
            left = left.union(&fall.parts);
            let blast = fall
                .parts
                .iter()
                .any(|index| loose.contains(index))
                .then_some(from)
                .flatten();
            self.drop_fall(cover, &layout, fall.parts, fall.tip, blast);
        }
        self.after_break(cover, &layout, &left, by);
    }

    /// Take `parts` out of the structure's standing parts and the world.
    fn remove_parts(&mut self, cover: usize, parts: &PartSet) {
        self.split_whole(cover, parts);
        let Some(state) = &mut self.covers[cover].structure else {
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

    /// The health, score and routes after the parts in `left` left structure `cover`.
    fn after_break(&mut self, cover: usize, layout: &Structure, left: &PartSet, by: Option<(u32, Team)>) {
        let c = &mut self.covers[cover];
        let Some(state) = &mut c.structure else {
            return;
        };
        c.hp = c.max_hp * state.standing_share();
        let levelled = !state.levelled && state.levels();
        if levelled {
            state.levelled = true;
            c.alive = false;
            let (x, z, h, id, color, kind) = (c.x, c.z, c.h, c.id, c.color, c.kind);
            self.destroyed += 1;
            if let Some((owner, team)) = by
                && self
                    .tanks
                    .iter()
                    .any(|tank| self.records(tank) && tank.id == owner && tank.team == team)
            {
                self.combat_record.cover_destroyed += 1;
            }
            if layout.levelling.announce {
                let mut destroy = SimEvent::at(SimEventType::Destroy, x, z);
                destroy.cover_kind = Some(kind);
                destroy.height = Some(h);
                destroy.id = Some(id);
                destroy.size = Some(layout.levelling.dust);
                destroy.color = Some(color);
                self.events.push(destroy);
            }
        }
        if levelled || left.iter().any(|index| layout.parts[index].blocks_ground) {
            let region = Footprint::from(&self.covers[cover]);
            self.nav.rebuild(&self.covers, Some(region));
        }
    }

    /// `parts` of structure `cover` coming down as one body: a group tipping over turns
    /// about the edge it tips over (`tip`, build frame), a part torn loose by a blast is
    /// thrown from it, anything else slumps out from the structure's middle (a wall
    /// alone tipping out over its foot).
    fn drop_fall(
        &mut self,
        cover: usize,
        layout: &Structure,
        parts: PartSet,
        tip: Option<(DVec3, DVec2)>,
        blast: Option<Point3>,
    ) {
        let (x, z) = (self.covers[cover].x, self.covers[cover].z);
        let body = self.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, 0.0, z))
                .linear_damping(FALLING_DAMPING.0)
                .angular_damping(FALLING_DAMPING.1)
                .can_sleep(true),
        );
        let colliders = self.insert_debris(layout, &parts, body);
        let dislodged = blast.is_some()
            && parts
                .iter()
                .all(|index| layout.parts[index].material.breaking == Breaking::Dislodges);
        let center = parts
            .iter()
            .map(|index| layout.to_world(layout.parts[index].middle()))
            .sum::<DVec3>()
            / parts.len() as f64;
        let (linear, angular) = match tip {
            Some((pivot, way)) => {
                let pivot = layout.to_world(pivot);
                let way = layout.to_world(DVec3::new(way.x, 0.0, way.y));
                let spin = DVec3::Y.cross(way) * TIP_RATE;
                let middle = from_vector(self.world.bodies[body].center_of_mass());
                let middle = DVec3::new(middle.x - x, middle.y, middle.z - z);
                (spin.cross(middle - pivot), spin)
            }
            // A timber knocked out of its slots flies out away from the hit, end over end.
            None if dislodged => {
                let blast = blast.expect("knocked out by a hit");
                let away = DVec3::new(x + center.x - blast.x, 0.0, z + center.z - blast.z).normalize_or_zero();
                let velocity = away * DISLODGE_THROW + DVec3::Y * DISLODGE_LIFT;
                let spin = DVec3::Y.cross(away) * DISLODGE_SPIN;
                (velocity, spin)
            }
            None => {
                let out = DVec3::new(center.x, 0.0, center.z).normalize_or_zero();
                let mut velocity = DVec3::new(out.x * SLUMP_SPEED, -SLUMP_SPEED, out.z * SLUMP_SPEED);
                if let Some(blast) = blast {
                    let away = DVec3::new(x + center.x - blast.x, 0.0, z + center.z - blast.z);
                    velocity += away.normalize_or_zero() * BLAST_THROW;
                }
                // Only a wall alone tips: tipping a wall with roof or floor on it out
                // about its middle swung the roof's inner edge up past the ridge.
                let wall = parts.iter().all(|index| layout.parts[index].upright);
                let spin = if wall {
                    DVec3::Y.cross(out) * SLUMP_TIP
                } else {
                    DVec3::ZERO
                };
                (velocity, spin)
            }
        };
        let rigid_body = &mut self.world.bodies[body];
        rigid_body.set_linvel(vector(linear.x, linear.y, linear.z), true);
        rigid_body.set_angvel(vector(angular.x, angular.y, angular.z), true);
        self.add_debris(cover, layout, body, &colliders, parts);
        // It slides out between what still stands round it (the standing members'
        // colliders reach past their faces and would jam it), meeting them again once
        // clear of the wall.
        if dislodged {
            for &handle in &colliders {
                set_debris_groups(
                    &mut self.world.colliders[handle],
                    group::STRUCTURE_DEBRIS_THROUGH,
                );
            }
            if let Some(debris) = self.fragments.last_mut().and_then(|fragment| fragment.structure.as_mut()) {
                debris.clear_at = Some(self.elapsed + DISLODGE_CLEAR);
            }
        }
    }

    /// A falling body's colliders, one for each of `parts`, each reporting what it lands
    /// on hard ([`crush_structure_parts`]).
    fn insert_debris(&mut self, layout: &Structure, parts: &PartSet, body: RigidBodyHandle) -> Vec<ColliderHandle> {
        let colliders: Vec<ColliderHandle> = parts
            .iter()
            .map(|index| self.world.insert_collider(debris_collider(layout, index), Some(body)))
            .collect();
        let threshold = (f64::from(self.world.bodies[body].mass()) * CRUSH_FORCE_PER_MASS) as f32;
        for &handle in &colliders {
            let collider = &mut self.world.colliders[handle];
            collider.set_active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
            collider.set_contact_force_event_threshold(threshold);
        }
        colliders
    }

    /// A fallen body of structure `cover`'s `parts` on `body`: it lies its structure's
    /// life (later by its lowest part's place in the spread) and is gone by the deadline.
    fn add_debris(
        &mut self,
        cover: usize,
        layout: &Structure,
        body: RigidBodyHandle,
        colliders: &[ColliderHandle],
        parts: PartSet,
    ) {
        self.reserve_fragments(1);
        let fragment_id = self.allocate_id();
        let surface = layout.part(parts.iter().next().unwrap_or(0)).material.surface;
        if let Some(&first) = colliders.first() {
            track_debris_contacts(self, body, first, fragment_id, surface);
        }
        let rules = layout.debris;
        let first = parts.iter().next().unwrap_or(0);
        let stagger = (first * 7 % 13) as f64 / 12.0;
        let c = &self.covers[cover];
        let mut fragment = Fragment::new(
            fragment_id,
            body,
            rules.life + rules.life_spread * stagger + DEBRIS_CLEANUP_SECONDS,
            1.0,
            c.color,
        );
        fragment.material = Some(surface);
        fragment.source_kind = Some(c.kind);
        fragment.created_at = Some(self.elapsed);
        fragment.expires_at = Some(self.elapsed + rules.deadline);
        fragment.structure = Some(StructureDebris {
            cover,
            parts,
            velocity: from_vector(self.world.bodies[body].linvel()),
            ..StructureDebris::default()
        });
        self.fragments.push(fragment);
    }

    /// A fallen stack of brick landing: it bursts into bricks where it struck.
    fn burst_bricks(&mut self, index: usize) {
        let fragment = self.fragments.remove(index);
        let body = &self.world.bodies[fragment.body];
        let at = from_vector(body.center_of_mass());
        let mut event = SimEvent::at(SimEventType::Shatter, at.x, at.z);
        event.height = Some(at.y);
        event.size = Some(BRICKS_SIZE);
        if let Some(debris) = fragment.structure
            && let Some(layout) = self.covers[debris.cover].structure.as_ref().map(|state| state.layout.clone())
        {
            let cover = &self.covers[debris.cover];
            let part = debris.parts.iter().next().unwrap_or(0);
            event.color = Some(part_paint(cover, &layout, part));
            event.cover_kind = Some(cover.kind);
            event.id = Some(cover.id);
        }
        self.events.push(event);
        self.remove_body(fragment.body);
    }

    /// Split fallen body `index` into `bodies`, each a body of its own moving as it was:
    /// thrown out from `throw`'s point at its strength (a burst or a shell) if any,
    /// otherwise going on as it moved, no faster upward than not at all (a landing's knock
    /// does not fling the far end up), with some of its spin, tipping about its break
    /// line when it lay still. The pieces never break up again on landing.
    fn split_debris(&mut self, index: usize, layout: &Structure, bodies: Vec<PartSet>, throw: Option<(Point3, f64)>) {
        let fragment = self.fragments.remove(index);
        let Some(debris) = fragment.structure else {
            return;
        };
        let old = &self.world.bodies[fragment.body];
        let (position, rotation) = (old.translation(), *old.rotation());
        let (linvel, angvel) = (old.linvel(), old.angvel());
        let still = linvel.length() < REST_SPEED;
        let center = old.center_of_mass();
        // A body dropping through the remains it lay on keeps doing so in pieces: put back
        // among them, its pieces would be pushed out through the ground.
        let bough = interaction_groups(group::BOUGH_DEBRIS);
        let groups = old
            .colliders()
            .iter()
            .map(|&handle| self.world.colliders[handle].collision_groups())
            .find(|&groups| groups != bough);
        self.remove_body(fragment.body);
        for (k, section) in bodies.into_iter().enumerate() {
            let body = self.world.insert_body(
                RigidBodyBuilder::dynamic()
                    .translation(position)
                    .rotation(rotation.to_scaled_axis())
                    .linear_damping(FALLING_DAMPING.0)
                    .angular_damping(FALLING_DAMPING.1)
                    .can_sleep(true),
            );
            let colliders = self.insert_debris(layout, &section, body);
            if let Some(groups) = groups {
                for &handle in &colliders {
                    let collider = &mut self.world.colliders[handle];
                    if collider.collision_groups() != bough {
                        collider.set_collision_groups(groups);
                    }
                }
            }
            let rigid_body = &mut self.world.bodies[body];
            let middle = rigid_body.center_of_mass();
            let mut velocity = linvel + angvel.cross(middle - center);
            let mut spin = angvel * LANDING_SPIN;
            match throw {
                Some((from, strength)) => {
                    velocity += throw_velocity(from_vector(middle), from, strength, section.len());
                    spin = angvel;
                }
                None => {
                    velocity.y = velocity.y.min(0.0);
                    // Broken where it lay: each piece tips about the line it broke along,
                    // one way then the other.
                    let out = (middle - center).with_y(0.0);
                    if still && out.length() > 1e-3 {
                        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                        spin += Vector::Y.cross(out.normalize()) * (sign * CREASE_SPIN) as f32;
                    }
                }
            }
            rigid_body.set_linvel(velocity, true);
            rigid_body.set_angvel(spin, true);
            self.reserve_fragments(1);
            let fragment_id = self.allocate_id();
            if let Some(&first) = colliders.first() {
                let surface = layout.part(section.iter().next().unwrap_or(0)).material.surface;
                track_debris_contacts(self, body, first, fragment_id, surface);
            }
            let mut piece = fragment.clone();
            piece.id = fragment_id;
            piece.body = body;
            // A piece is a body of its own from its landing: it falls flat over the next
            // moments before it is damped to rest, rather than freezing on its edge.
            piece.created_at = Some(self.elapsed);
            // What stays together of a tree falls on, to land again ([`Structure::snaps_on_ground`]).
            let landed = !(layout.snaps_on_ground && section.len() > 1);
            piece.structure = Some(StructureDebris {
                cover: debris.cover,
                parts: section,
                velocity: from_vector(self.world.bodies[body].linvel()),
                landed,
                landed_at: self.elapsed,
                ..StructureDebris::default()
            });
            self.fragments.push(piece);
        }
    }

    /// Fallen body `index` struck at `point` in its part `struck` (a shell's hit, or the
    /// part nearest a burst): that part shatters where it lies, and the rest of the body
    /// breaks into its landing pieces thrown out from the point at `strength`. With no
    /// room for the pieces in the debris budget, all of it shatters. No random draw.
    ///
    /// Returns whether the struck part gave way: wood that is only knocked aside or
    /// snapped still lies in a shell's path, so a shell that would fly on stops in it
    /// rather than meeting it again and again.
    pub fn break_loose_body(
        &mut self,
        index: usize,
        struck: usize,
        point: Point3,
        strength: f64,
    ) -> bool {
        let fragment = &self.fragments[index];
        let Some(debris) = fragment.structure else {
            return false;
        };
        let Some(layout) = self.covers[debris.cover]
            .structure
            .as_ref()
            .map(|state| state.layout.clone())
        else {
            return false;
        };
        let parts = debris.parts;
        let struck = if parts.contains(struck) {
            struck
        } else {
            parts.iter().next().expect("a fallen body has parts")
        };
        // What would break it knocks it whole instead (a tree's log, a thick beam), unless
        // it snaps where it was struck into the pieces it breaks into alone (a limb).
        if layout.part(struck).material.breaking == Breaking::Dislodges {
            let snapped: Vec<PartSet> = layout
                .snaps
                .iter()
                .find(|snap| snap.needs == parts)
                .map(|breakup| {
                    breakup
                        .pieces
                        .iter()
                        .map(|&piece| PartSet::from_indices([piece as usize]))
                        .collect()
                })
                .unwrap_or_default();
            let room = self.fragments.len() - 1 + snapped.len() <= self.max_fragments;
            if snapped.len() > 1 && room {
                self.split_debris(index, &layout, snapped, Some((point, strength)));
            } else {
                self.throw_loose_body(index, point, strength);
            }
            return false;
        }
        let mut rest = parts;
        rest.remove(struck);
        let bodies = if rest.is_empty() {
            Vec::new()
        } else {
            layout.landing(&rest, false)
        };
        let room = self.fragments.len() - 1 + bodies.len() <= self.max_fragments;
        let shattered = if room { PartSet::from_indices([struck]) } else { parts };
        // Each shattered part's splinters from where it lies now.
        let body = &self.world.bodies[fragment.body];
        let thrown = first_few(
            body.colliders().iter().filter_map(|&handle| {
                let collider = &self.world.colliders[handle];
                let part = collider.user_data as usize;
                shattered.contains(part).then(|| {
                    let aabb = collider.compute_aabb();
                    let middle = (aabb.mins + aabb.maxs) * 0.5;
                    let piece = layout.part(part);
                    let size = (piece.max - piece.min).max_element();
                    (size, part, DVec3::new(middle.x as f64, middle.y as f64, middle.z as f64))
                })
            }),
            |a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)),
        );
        let cover = &self.covers[debris.cover];
        for &(size, part, middle) in thrown.iter().flatten() {
            let mut event = SimEvent::at(SimEventType::Shatter, middle.x, middle.z);
            event.height = Some(middle.y);
            event.size = Some(size);
            event.color = Some(part_paint(cover, &layout, part));
            event.cover_kind = Some(cover.kind);
            event.id = Some(cover.id);
            event.from = Some(point);
            self.events.push(event);
        }
        if bodies.is_empty() || !room {
            let fragment = self.fragments.remove(index);
            self.remove_body(fragment.body);
            return true;
        }
        self.split_debris(index, &layout, bodies, Some((point, strength)));
        true
    }
}

impl Simulation {
    /// Throw fallen body `index` away from `point` by `strength` (spread over its parts),
    /// out of its rest: it flies and lands again before it settles.
    pub fn throw_loose_body(&mut self, index: usize, point: Point3, strength: f64) {
        let fragment = &self.fragments[index];
        let parts = fragment.structure.map_or(1, |debris| debris.parts.len());
        let handle = fragment.body;
        let middle = from_vector(self.world.bodies[handle].center_of_mass());
        let push = throw_velocity(middle, point, strength, parts);
        let rigid_body = &mut self.world.bodies[handle];
        let velocity = rigid_body.linvel() + push;
        rigid_body.set_linvel(velocity, true);
        unrest_body(&mut self.world, handle);
        let elapsed = self.elapsed;
        let fragment = &mut self.fragments[index];
        fragment.created_at = Some(elapsed);
        if let Some(debris) = &mut fragment.structure {
            debris.velocity = from_vector(velocity);
            debris.rested = false;
            debris.damping = Damping::Falling;
            debris.window = None;
            debris.still_since = None;
            debris.landed_at = elapsed;
        }
    }
}

/// A part of a body this low (m) reaches the ground.
const GROUND_TOUCH: f32 = 0.2;

/// What a falling body of `parts` that landed hard breaks into when only its parts on the
/// ground snap off ([`Structure::snaps_on_ground`]): each of those alone, and the rest
/// together.
fn ground_snaps(simulation: &Simulation, handle: RigidBodyHandle, layout: &Structure, parts: &PartSet) -> Vec<PartSet> {
    let mut rest = *parts;
    let mut bodies = Vec::new();
    for &collider in simulation.world.bodies[handle].colliders() {
        let collider = &simulation.world.colliders[collider];
        let part = collider.user_data as usize;
        if rest.contains(part) && !layout.part(part).load && collider.compute_aabb().mins.y <= GROUND_TOUCH {
            rest.remove(part);
            bodies.push(PartSet::from_indices([part]));
        }
    }
    if !rest.is_empty() {
        bodies.push(rest);
    }
    bodies
}

/// The first [`SHATTER_EVENTS`] of `items` in `order`, in that order (equal ones as they
/// came), without sorting them all.
fn first_few<T: Copy>(
    items: impl IntoIterator<Item = T>,
    order: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> [Option<T>; SHATTER_EVENTS] {
    let mut few = [None; SHATTER_EVENTS];
    for item in items {
        let Some(at) = few
            .iter()
            .position(|kept| kept.as_ref().is_none_or(|kept| order(&item, kept).is_lt()))
        else {
            continue;
        };
        few[at..].rotate_right(1);
        few[at] = Some(item);
    }
    few
}

/// The velocity (m/s) a piece whose middle is at `at` is thrown with by a hit or burst at
/// `from` of `strength`: out from it and up, a big piece less.
fn throw_velocity(at: Point3, from: Point3, strength: f64, parts: usize) -> Vector {
    let out = Vec2::new(at.x - from.x, at.z - from.z);
    let length = out.x.hypot(out.z);
    let out = if length > 1e-6 {
        Vec2::new(out.x / length, out.z / length)
    } else {
        Vec2::new(0.0, 0.0)
    };
    let scale = strength / (parts as f64).sqrt();
    vector(
        out.x * LOOSE_THROW_OUT * scale,
        LOOSE_THROW_UP * scale,
        out.z * LOOSE_THROW_OUT * scale,
    )
}

/// The fallen structure pieces a blast at `point` reaches within `radius`: those within
/// its core break where they lie ([`Simulation::break_loose_body`]), the rest are thrown
/// up and out from it, out of the rest damping, and land over the next moments. Each body
/// is measured by its nearest part, not its middle. No random draw.
pub fn blast_structure_debris(simulation: &mut Simulation, point: Point3, radius: f64, damage: f64) {
    if damage > 0.0 {
        throw_structure_debris(simulation, point, radius, loose_throw(damage));
    }
}

/// How hard a blast of `damage` throws a structure's fallen pieces: a standard shell's
/// burst throws them at [`LOOSE_THROW_OUT`].
pub fn loose_throw(damage: f64) -> f64 {
    (damage / LOOSE_THROW_DAMAGE).min(LOOSE_THROW_MOST)
}

/// [`blast_structure_debris`] throwing the pieces `strength` times as hard as a standard
/// shell's burst.
pub fn throw_structure_debris(simulation: &mut Simulation, point: Point3, radius: f64, strength: f64) {
    if radius <= 0.0 || strength <= 0.0 {
        return;
    }
    // Loose pieces lie in the open: a burst throws them from further than it breaks
    // what stands.
    let radius = radius * LOOSE_REACH;
    let core = (radius * LOOSE_CORE_SHARE).clamp(BLAST_CORE, LOOSE_CORE_MAX);
    let at = vector(point.x, point.y, point.z);
    // The bodies there before the blast: the pieces it breaks off are thrown already.
    // A section still coming down as a structure falls as it was: a blast breaks or throws
    // only what has landed (the burst that shot out a tower's leg would otherwise have
    // crumpled the whole tower before it tipped).
    let mut listed = std::mem::take(&mut simulation.scratch.thrown_debris);
    listed.clear();
    listed.extend(
        simulation
            .fragments
            .iter()
            .enumerate()
            .filter(|(_, fragment)| {
                fragment.structure.is_some_and(|debris| debris.landed || debris.rested)
                    && fragment.life > DEBRIS_CLEANUP_SECONDS
            })
            .map(|(index, fragment)| (fragment.id, index)),
    );
    for &(id, listed_at) in &listed {
        // Breaking a body adds and evicts pieces, which moves the rest; ids are unique.
        let unmoved = simulation
            .fragments
            .get(listed_at)
            .is_some_and(|fragment| fragment.id == id);
        let found = if unmoved {
            Some(listed_at)
        } else {
            simulation.fragments.iter().position(|fragment| fragment.id == id)
        };
        let Some(index) = found else {
            continue;
        };
        let fragment = &simulation.fragments[index];
        let body = &simulation.world.bodies[fragment.body];
        let nearest = body
            .colliders()
            .iter()
            .map(|&handle| {
                let collider = &simulation.world.colliders[handle];
                let gap = collider.shape().distance_to_point(collider.position(), at, true);
                (f64::from(gap), collider.user_data as usize)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let Some((gap, part)) = nearest else {
            continue;
        };
        if gap >= radius {
            continue;
        }
        if gap < core {
            let _ = simulation.break_loose_body(index, part, point, strength);
            continue;
        }
        let falloff = 1.0 - gap / radius;
        simulation.throw_loose_body(index, point, strength * falloff);
    }
    simulation.scratch.thrown_debris = listed;
}

/// Each tick: a falling body that lands hard (on the ground, for a structure whose
/// bodies break only there) or lies jammed where it came to a stop breaks into its
/// landing pieces, its load crashing down with dust and noise; a mortared one bursts into
/// bricks. A landed piece is shoved off the remains it lies up on, laid down off a thin
/// edge, and once it has lain still a moment comes to rest for good: it no longer turns,
/// grips what it lies on and clears from the top of its pile.
pub fn settle_structure_debris(simulation: &mut Simulation) {
    unwedge(simulation);
    let mut index = 0;
    while index < simulation.fragments.len() {
        let fragment = &simulation.fragments[index];
        let Some(debris) = fragment.structure else {
            index += 1;
            continue;
        };
        let Some(layout) = simulation.covers[debris.cover]
            .structure
            .as_ref()
            .map(|state| state.layout.clone())
        else {
            index += 1;
            continue;
        };
        let handle = fragment.body;
        if debris.clear_at.is_some_and(|at| simulation.elapsed >= at) {
            set_body_groups(&mut simulation.world, handle, group::STRUCTURE_DEBRIS);
            if let Some(debris) = &mut simulation.fragments[index].structure {
                debris.clear_at = None;
            }
        }
        if !debris.rested {
            cap_loose_speed(&mut simulation.world.bodies[handle]);
        }
        let fragment = &simulation.fragments[index];
        let body = &simulation.world.bodies[handle];
        let now = from_vector(body.linvel());
        let before = debris.velocity;
        let change = (now.x - before.x)
            .hypot(now.y - before.y)
            .hypot(now.z - before.z);
        let age = simulation.elapsed - fragment.created_at.unwrap_or(simulation.elapsed);
        let alive = fragment.life > DEBRIS_CLEANUP_SECONDS;
        if alive
            && change > BRICKS_LANDING
            && debris
                .parts
                .iter()
                .all(|part| layout.part(part).material.breaking == Breaking::Bricks)
        {
            simulation.burst_bricks(index);
            continue;
        }
        // A falling tree's limbs that reach the ground cannot hold it up: they snap off
        // however gently they meet it, until it lies on its trunk.
        if layout.snaps_on_ground && !debris.landed && alive && debris.parts.len() > 1 {
            let bodies = ground_snaps(simulation, handle, &layout, &debris.parts);
            let room = simulation.fragments.len() + bodies.len() - 1 <= simulation.max_fragments;
            if bodies.len() > 1 && room {
                simulation.split_debris(index, &layout, bodies, None);
                continue;
            }
        }
        let stopped = !debris.landed
            && debris.parts.len() > 1
            && debris
                .still_since
                .is_some_and(|since| simulation.elapsed - since >= REST_HOLD);
        let jammed = stopped && !layout.debris.firm_only;
        let hard = debris.struck
            || (change > layout.debris.landing_speed
                && (!layout.debris.firm_only || on_firm(simulation, handle, debris.cover)));
        if !debris.landed && alive && (jammed || hard) {
            if debris.parts.iter().any(|part| layout.part(part).load) {
                let at = from_vector(body.center_of_mass());
                let cover = &simulation.covers[debris.cover];
                let mut crash = SimEvent::at(SimEventType::Destroy, at.x, at.z);
                crash.cover_kind = Some(cover.kind);
                crash.height = Some(CRASH_HEIGHT);
                crash.id = Some(cover.id);
                crash.size = Some(CRASH_SIZE);
                crash.color = Some(fragment.color);
                simulation.events.push(crash);
            }
            let room = |count: usize| simulation.fragments.len() + count - 1 <= simulation.max_fragments;
            let mut bodies = if layout.snaps_on_ground {
                ground_snaps(simulation, handle, &layout, &debris.parts)
            } else {
                layout.landing(&debris.parts, false)
            };
            if !room(bodies.len()) {
                bodies = layout.landing(&debris.parts, true);
            }
            if bodies.len() > 1 {
                simulation.split_debris(index, &layout, bodies, None);
                continue;
            }
            // A body that lands whole (a lone part, or one the budget has no room to
            // break) settles from now, however long it has been falling; a tree whose
            // limbs did not reach the ground falls on, to land again.
            let falls_on = layout.snaps_on_ground && debris.parts.len() > 1;
            if !falls_on && let Some(debris) = &mut simulation.fragments[index].structure {
                debris.landed = true;
                debris.landed_at = simulation.elapsed;
            }
        }
        // A firm-only body come to a stop on something else (its own remains, other
        // debris) lands whole there: it settles and rests from now, and breaks only
        // where something new strikes it.
        if stopped
            && layout.debris.firm_only
            && let Some(debris) = &mut simulation.fragments[index].structure
            && !debris.landed
        {
            debris.landed = true;
            debris.landed_at = simulation.elapsed;
        }
        let body = &simulation.world.bodies[handle];
        let up_on_remains = !debris.rested
            && f64::from(body.center_of_mass().y) >= WEDGE_HEIGHT
            && on_remains(simulation, handle);
        // Nothing rests on a thin edge: a piece on one is laid down first.
        let lies = if debris.rested {
            Lying::Flat
        } else {
            lay_down_on_edge(simulation, index, &layout, age)
        };
        let elapsed = simulation.elapsed;
        let fragment = &mut simulation.fragments[index];
        let debris = fragment.structure.as_mut().expect("checked above");
        let body = &mut simulation.world.bodies[handle];
        debris.velocity = now;
        debris.struck = false;
        let since_landing = if debris.landed {
            elapsed - debris.landed_at
        } else {
            0.0
        };
        // A lone part rests once it has had time to fall, not while it is still
        // starting to tip.
        let can_rest = debris.landed || (debris.parts.len() == 1 && age > REST_AFTER);
        let mut just_rested = false;
        if !debris.rested && (can_rest || age > REST_AFTER) && debris.wedged.is_none() {
            let still = body.linvel().length() < REST_SPEED && body.angvel().length() < REST_SPIN;
            match (still, debris.still_since) {
                (false, _) => debris.still_since = None,
                (true, None) => debris.still_since = Some(elapsed),
                _ => {}
            }
            // A window opens where it is, and opens again wherever it has got to once it
            // has moved or turned too far since.
            let (at, turn) = (body.center_of_mass(), *body.rotation());
            let (opened, from, from_turn) = *debris.window.get_or_insert((elapsed, at, turn));
            let moved = from.distance(at) > REST_DRIFT || from_turn.angle_between(turn) > REST_TURN;
            if moved {
                debris.window = Some((elapsed, at, turn));
            }
            let lain_still = !moved && elapsed - opened >= REST_WINDOW;
            let creeping = since_landing > CREEP_AFTER
                && body.linvel().length() < CREEP_SPEED
                && body.angvel().length() < CREEP_SPIN;
            // A piece rests only once it has lain still a moment, or barely moves well
            // after its landing, and lies flat: rested by a deadline whatever its speed or
            // pose, a piece still falling froze where it was (a roof stood propped on its
            // edge).
            if can_rest && !up_on_remains && lies == Lying::Flat && (lain_still || creeping) {
                // It stops, no longer turns, and grips what it lies on: the pile lies
                // still. It clears from the top, so nothing it lies on goes from under
                // it first.
                debris.rested = true;
                body.set_angvel(Vector::ZERO, false);
                body.set_linvel(Vector::ZERO, false);
                rest_body(&mut simulation.world, handle);
                just_rested = true;
            }
        }
        let body = &mut simulation.world.bodies[handle];
        // A resting piece loses any upward speed each tick rather than having its height
        // locked: between two pieces locked in height, a contact whose normal is all but
        // vertical has almost no give along it, and the solver's impulse there flung the
        // pile sideways at hundreds of m/s.
        if debris.rested && body.linvel().y > 0.0 {
            let mut velocity = body.linvel();
            velocity.y = 0.0;
            body.set_linvel(velocity, false);
        }
        debris.damping = if debris.rested {
            Damping::Rested
        } else if debris.wedged.is_some() || up_on_remains || lies == Lying::Laying {
            // Up on the remains it slides off them freely; being shoved off them or laid
            // down it keeps falling's damping.
            Damping::Sliding
        } else if since_landing > CREEP_AFTER {
            Damping::Creeping
        } else if since_landing > LANDED_SETTLE {
            Damping::Settled
        } else {
            Damping::Falling
        };
        let (linear, angular) = debris.damping.values(age);
        if body.linear_damping() != linear {
            body.set_linear_damping(linear);
            body.set_angular_damping(angular);
        }
        if just_rested {
            clear_in_turn(simulation, index);
        }
        index += 1;
    }
    clear_together(simulation);
}

/// Hold a structure's loose piece to [`MAX_ACROSS`], [`MAX_RISE`] and [`MAX_TURN`] (the
/// host each tick before its step, and a client's predicted world alike: a beam a hull
/// shoves along is held back by it).
pub fn cap_loose_speed(body: &mut rapier3d::prelude::RigidBody) {
    let v = body.linvel();
    let across = Vector::new(v.x, 0.0, v.z);
    let across = if across.length() > MAX_ACROSS {
        across * (MAX_ACROSS / across.length())
    } else {
        across
    };
    let capped = Vector::new(across.x, v.y.min(MAX_RISE), across.z);
    if capped != v {
        body.set_linvel(capped, true);
    }
    let w = body.angvel();
    if w.length() > MAX_TURN {
        body.set_angvel(w * (MAX_TURN / w.length()), true);
    }
}

/// How a fallen body lies, from its biggest part, in world axes (the structure's turn
/// applied): its build frame is the structure's, world-aligned where it fell from.
fn lie(layout: &Structure, parts: &PartSet) -> Lie {
    let Some(biggest) = parts.iter().max_by(|&a, &b| {
        let volume = |i: usize| (layout.part(i).max - layout.part(i).min).element_product();
        volume(a).total_cmp(&volume(b)).then(b.cmp(&a))
    }) else {
        return Lie::Any;
    };
    match layout.part(biggest).lie {
        Lie::Any => Lie::Any,
        Lie::Face(v) => Lie::Face(layout.to_world(v)),
        Lie::Roof(v) => Lie::Roof(layout.to_world(v)),
        Lie::Length(v) => Lie::Length(layout.to_world(v)),
    }
}

/// How a fallen body lies for [`lay_down_on_edge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lying {
    /// Flat, or propped as it may lie: it may rest.
    Flat,
    /// Being laid down off its edge.
    Laying,
    /// Too soon after it came loose, or still moving, to tell.
    Unsettled,
}

/// Fallen body `index`, come to rest on a thin edge (a roof slope on its eave, a wall
/// section on its foot, a post on its end) and not propped on what still stands, is laid
/// down a step: nothing rests on an edge. No random draw.
fn lay_down_on_edge(simulation: &mut Simulation, index: usize, layout: &Structure, age: f64) -> Lying {
    let fragment = &simulation.fragments[index];
    let Some(debris) = fragment.structure else {
        return Lying::Flat;
    };
    let lie = lie(layout, &debris.parts);
    if lie == Lie::Any || fragment.life <= DEBRIS_CLEANUP_SECONDS {
        return Lying::Flat;
    }
    let handle = fragment.body;
    let body = &simulation.world.bodies[handle];
    if age < LAY_AFTER || (!debris.laying && body.linvel().length() > LAY_SLOWER) {
        return Lying::Unsettled;
    }
    let rotation = *body.rotation();
    let propped = on_remains(simulation, handle);
    let middle = body.center_of_mass();
    let (now, target, done, start) = match lie {
        Lie::Face(normal) => {
            let now = rotation * vector(normal.x, normal.y, normal.z);
            // Leaning on what stands it is turned down away from it; otherwise over the
            // way it leans.
            let away = propped.then(|| {
                let cover = &simulation.covers[debris.cover];
                let structure = Vector::new(cover.x as f32, middle.y, cover.z as f32);
                let out = (middle - structure).with_y(0.0);
                // Turning the face toward +y tips the top over toward -face: pick the way
                // that tips it away from the structure.
                if now.with_y(0.0).dot(out) >= 0.0 { -Vector::Y } else { Vector::Y }
            });
            let target = away.unwrap_or(if now.y >= 0.0 { Vector::Y } else { -Vector::Y });
            // Leaning on what stands, anything short of flat is laid down off it.
            let (edge, laid) = if propped {
                (LAID, LAID_OFF)
            } else {
                (LEANING, LAID)
            };
            (now, target, now.y.abs() >= laid, now.y.abs() < edge)
        }
        // A roof piece on its edge is laid down the way it leans, whichever face up:
        // turning one over off its back pivoted it into the ground and never finished.
        Lie::Roof(normal) => {
            let now = rotation * vector(normal.x, normal.y, normal.z);
            let target = if now.y >= 0.0 { Vector::Y } else { -Vector::Y };
            // Leaning on what stands, anything short of flat is laid down off it.
            let (edge, laid) = if propped {
                (LAID, LAID_OFF)
            } else {
                (LEANING, LAID)
            };
            (now, target, now.y.abs() >= laid, now.y.abs() < edge)
        }
        Lie::Length(along) => {
            let now = rotation * vector(along.x, along.y, along.z);
            let flat = Vector::new(now.x, 0.0, now.z);
            let target = if flat.length() > 1e-3 { flat.normalize() } else { Vector::X };
            let rise = now.y.abs();
            (now, target, rise <= LAID_STICK, rise > ON_END)
        }
        Lie::Any => unreachable!("returned above"),
    };
    let held = propped && matches!(lie, Lie::Length(_));
    if done || !(debris.laying || start) || held {
        if let Some(debris) = &mut simulation.fragments[index].structure {
            debris.laying = false;
        }
        return Lying::Flat;
    }
    if let Some(debris) = &mut simulation.fragments[index].structure {
        debris.laying = true;
    }
    let mut axis = now.cross(target);
    if axis.length() < 1e-4 {
        // Square on its back: over about its own length.
        axis = (rotation * Vector::X).with_y(0.0);
    }
    let off = axis.length();
    if off < 1e-4 {
        return Lying::Flat;
    }
    let angle = now.angle_between(target).min(LAY_RATE * STEP as f32);
    let step = Rotation::from_axis_angle(axis / off, angle);
    // It turns about where it bears on what holds it, on the side it goes over to: the
    // edge a tipping board pivots on, not a point inside the pile under it.
    let over = (axis / off).cross(Vector::Y).with_y(0.0);
    let foot = support_point(simulation, handle, over).unwrap_or_else(|| {
        let low = body
            .colliders()
            .iter()
            .map(|&collider| simulation.world.colliders[collider].compute_aabb().mins.y)
            .fold(f32::INFINITY, f32::min);
        Vector::new(middle.x, low, middle.z)
    });
    let body = &simulation.world.bodies[handle];
    let mut at = foot + step * (body.translation() - foot);
    // Laid down, it never rises: its middle goes no higher than it was.
    let raised = (foot + step * (middle - foot)).y - middle.y;
    if raised > 0.0 {
        at.y -= raised;
    }
    let body = &mut simulation.world.bodies[handle];
    body.set_position(Pose::from_parts(at, step * rotation), true);
    body.set_linvel(Vector::ZERO, true);
    body.set_angvel(Vector::ZERO, true);
    Lying::Laying
}

/// Where body `body` bears on what it touches (the ground, the pile, the remains),
/// furthest along `over` (the way it goes over), the lowest of those there: `None`
/// while it touches nothing.
fn support_point(simulation: &Simulation, body: RigidBodyHandle, over: Vector) -> Option<Vector> {
    let mut best: Option<(f32, f32, Vector)> = None;
    for &collider in simulation.world.bodies[body].colliders() {
        for pair in simulation.world.contact_pairs_with(collider) {
            if !pair.has_any_active_contact() {
                continue;
            }
            let (first, ours_first) = if pair.collider1 == collider {
                (pair.collider1, true)
            } else {
                (pair.collider2, false)
            };
            let other = if ours_first { pair.collider2 } else { pair.collider1 };
            let (Some(ours), Some(theirs)) =
                (simulation.world.colliders.get(first), simulation.world.colliders.get(other))
            else {
                continue;
            };
            let (first_pose, second_pose) = if ours_first {
                (ours.position(), theirs.position())
            } else {
                (theirs.position(), ours.position())
            };
            for manifold in pair.manifolds() {
                let sub = manifold.subshape_poses.as_deref();
                let pose1 = sub.and_then(|poses| poses.pos1).map_or(*first_pose, |p| first_pose * p);
                let pose2 = sub.and_then(|poses| poses.pos2).map_or(*second_pose, |p| second_pose * p);
                for point in &manifold.points {
                    if point.dist > 0.01 {
                        continue;
                    }
                    // Our side of the contact.
                    let at = if ours_first {
                        pose1.transform_point(point.local_p1)
                    } else {
                        pose2.transform_point(point.local_p2)
                    };
                    let key = (at.dot(over), -at.y);
                    if best.is_none_or(|(along, low, _)| {
                        key.0 > along + 0.02 || ((key.0 - along).abs() <= 0.02 && key.1 > low)
                    }) {
                        best = Some((key.0, key.1, at));
                    }
                }
            }
        }
    }
    best.map(|(_, _, at)| at)
}

/// How high (m) a body's middle lies.
fn middle_height(simulation: &Simulation, body: RigidBodyHandle) -> f64 {
    f64::from(simulation.world.bodies[body].center_of_mass().y).max(0.0)
}

/// Time a piece of a ruin that has just come to rest to clear in turn: the pile clears
/// from the top down, so nothing fades through what lies over it. A ruin with one clock
/// ([`DebrisRules::one_clock`], a watchtower's) keeps the earliest of its resting
/// pieces', whenever each came loose; any other lies its pieces' own times, each piece
/// clearing a moment before what it lies on.
fn clear_in_turn(simulation: &mut Simulation, index: usize) {
    let fragment = &simulation.fragments[index];
    let Some(debris) = fragment.structure else {
        return;
    };
    let cover = debris.cover;
    let one_clock = simulation.covers[cover]
        .structure
        .as_ref()
        .is_some_and(|state| state.layout.debris.one_clock);
    let fell = fragment.expires_at;
    let mut ruin = std::mem::take(&mut simulation.scratch.ruin);
    ruin.clear();
    ruin.extend((0..simulation.fragments.len()).filter(|&other| {
        let piece = &simulation.fragments[other];
        other != index
            && piece.life > DEBRIS_CLEANUP_SECONDS
            && piece
                .structure
                .is_some_and(|debris| debris.rested && debris.cover == cover)
            && fell_together(piece.expires_at, fell)
    }));
    let height = middle_height(simulation, fragment.body);
    let floor = DEBRIS_CLEANUP_SECONDS + CLEAR_TOP_FIRST;
    if one_clock {
        // When a piece lying on the ground would clear.
        let ground = ruin
            .iter()
            .map(|&other| {
                let piece = &simulation.fragments[other];
                piece.life + middle_height(simulation, piece.body) * CLEAR_TOP_FIRST
            })
            .fold(fragment.life, f64::min);
        simulation.fragments[index].life = (ground - height * CLEAR_TOP_FIRST).max(floor);
        simulation.scratch.ruin = ruin;
        return;
    }
    // Before what it lies on, after what lies on it: each piece keeps one of the ruin's
    // own staggered times, and resting only swaps times with a piece it lies on or under
    // when they are the wrong way round. Pulling each piece a moment before what it lay
    // on chained a whole heap's times together, and it went in one batch.
    // Its own time in the ruin's schedule, from when the ruin fell: the pieces a landing
    // breaks a body into all start from that body's time, so a heap would go at once.
    if let (Some(expires), Some(state)) = (fragment.expires_at, &simulation.covers[cover].structure) {
        let rules = state.layout.debris;
        let first = debris.parts.iter().next().unwrap_or(0);
        let stagger = ((first * 7 + fragment.id as usize * 5) % 13) as f64 / 12.0;
        let fell = expires - rules.deadline;
        let at = fell + rules.life + rules.life_spread * stagger + DEBRIS_CLEANUP_SECONDS;
        simulation.fragments[index].life = (at - simulation.elapsed).min(expires - simulation.elapsed);
    }
    let fragment = &simulation.fragments[index];
    let mine = plan_box(simulation, fragment.body);
    for &other in &ruin {
        let piece = &simulation.fragments[other];
        if !plan_box(simulation, piece.body).overlaps(&mine) {
            continue;
        }
        let above = middle_height(simulation, piece.body) > height;
        let (my_life, its_life) = (simulation.fragments[index].life, piece.life);
        let wrong_way = if above { its_life > my_life } else { my_life > its_life };
        if wrong_way {
            simulation.fragments[index].life = its_life;
            simulation.fragments[other].life = my_life;
        }
    }
    let life = simulation.fragments[index].life;
    simulation.fragments[index].life = life.max(floor);
    simulation.scratch.ruin = ruin;
}

/// A body's footprint on the ground plane, a little grown so pieces lying edge to edge
/// count as on each other.
fn plan_box(simulation: &Simulation, body: RigidBodyHandle) -> PlanBox {
    let mut plan = PlanBox {
        min: [f32::INFINITY; 2],
        max: [f32::NEG_INFINITY; 2],
    };
    for &collider in simulation.world.bodies[body].colliders() {
        let aabb = simulation.world.colliders[collider].compute_aabb();
        plan.min = [plan.min[0].min(aabb.mins.x), plan.min[1].min(aabb.mins.z)];
        plan.max = [plan.max[0].max(aabb.maxs.x), plan.max[1].max(aabb.maxs.z)];
    }
    plan
}

struct PlanBox {
    min: [f32; 2],
    max: [f32; 2],
}

impl PlanBox {
    fn overlaps(&self, other: &PlanBox) -> bool {
        (0..2).all(|k| self.min[k] - ON_EACH_OTHER < other.max[k] && other.min[k] - ON_EACH_OTHER < self.max[k])
    }
}

/// Pieces whose footprints come this near (m) count as lying on each other.
const ON_EACH_OTHER: f32 = 0.05;

/// The splinters and dust (m) a timber knocked out whole throws off its struck face.
const DISLODGED_DUST: f64 = 0.7;
/// How a timber knocked out whole flies: out from the hit (m/s), up (m/s), end over end
/// (rad/s), and how long (s) it passes through what stands round it.
const DISLODGE_THROW: f64 = 6.0;
const DISLODGE_LIFT: f64 = 1.2;
const DISLODGE_SPIN: f64 = 2.0;
const DISLODGE_CLEAR: f64 = 0.3;

/// Pieces that fell within this many seconds of each other (by their clearing deadlines,
/// which a landing's pieces keep from their body) are one ruin, which clears top first
/// and as a whole; a later fall onto an older ruin keeps its own time, or a collapse a
/// few shells after earlier hits faded seconds after it came down.
const FELL_TOGETHER: f64 = 0.5;

fn fell_together(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= FELL_TOGETHER,
        _ => false,
    }
}

/// A resting piece of a ruin about to clear takes what it holds up along: each resting
/// piece of the same ruin touching it, other than one it lies on, clears from then on as
/// it does, and so on up the pile, so nothing drops as what held it goes (a piece leaning
/// on a taller one, which the order by height clears first). Done [`CLEAR_AHEAD`] before
/// it starts to fade, while it still touches them: a fading piece passes through
/// everything.
fn clear_together(simulation: &mut Simulation) {
    let rested = |piece: &Fragment| piece.structure.is_some_and(|debris| debris.rested);
    let mut along = std::mem::take(&mut simulation.scratch.along);
    along.clear();
    for fragment in &simulation.fragments {
        if !rested(fragment) || fragment.life > DEBRIS_CLEANUP_SECONDS + CLEAR_AHEAD {
            continue;
        }
        let cover = fragment.structure.map(|debris| debris.cover);
        for &collider in simulation.world.bodies[fragment.body].colliders() {
            for pair in simulation.world.contact_pairs_with(collider) {
                // The contact normals out of this piece: one pointing well down is a
                // piece it lies on.
                let (other, out) = if pair.collider1 == collider {
                    (pair.collider2, 1.0)
                } else {
                    (pair.collider1, -1.0)
                };
                let holds = pair.has_any_active_contact()
                    && pair.manifolds().iter().any(|manifold| {
                        !manifold.points.is_empty() && out * manifold.data.normal.y > -BENEATH
                    });
                if !holds {
                    continue;
                }
                let Some(body) = simulation.world.colliders.get(other).and_then(|other| other.parent()) else {
                    continue;
                };
                let Some(index) = simulation.fragments.iter().position(|piece| piece.body == body) else {
                    continue;
                };
                let piece = &simulation.fragments[index];
                if piece.structure.map(|debris| debris.cover) == cover
                    && rested(piece)
                    && fell_together(piece.expires_at, fragment.expires_at)
                    && piece.life > fragment.life
                {
                    along.push((index, fragment.life));
                }
            }
        }
    }
    for &(index, life) in &along {
        let piece = &mut simulation.fragments[index];
        piece.life = piece.life.min(life);
    }
    simulation.scratch.along = along;
}

/// Whether any of `body`'s colliders touches the ground.
/// Whether fallen body `body` of structure `cover` touches the ground or a cover other
/// than what stands of its own structure.
fn on_firm(simulation: &Simulation, body: RigidBodyHandle, cover: usize) -> bool {
    simulation.world.bodies[body].colliders().iter().any(|&collider| {
        simulation.world.contact_pairs_with(collider).any(|pair| {
            let other = if pair.collider1 == collider {
                pair.collider2
            } else {
                pair.collider1
            };
            let ground = simulation.world.colliders.get(other).is_some_and(|other| {
                other.collision_groups().memberships.bits() & GROUND_MEMBERSHIP != 0
            });
            let struck = simulation
                .cover_by_collider
                .get(&other)
                .is_some_and(|&index| index != cover);
            pair.has_any_active_contact() && (ground || struck)
        })
    })
}

/// The ground's collision membership ([`group::GROUND`]).
const GROUND_MEMBERSHIP: u32 = group::GROUND >> 16;

/// Whether fallen body `body` touches a structure's standing part.
fn on_remains(simulation: &Simulation, body: RigidBodyHandle) -> bool {
    simulation.world.bodies[body].colliders().iter().any(|&collider| {
        simulation.world.contact_pairs_with(collider).any(|pair| {
            let other = if pair.collider1 == collider {
                pair.collider2
            } else {
                pair.collider1
            };
            pair.has_any_active_contact() && simulation.structure_cover(other).is_some()
        })
    })
}

/// A fallen piece come to rest up on what still stands of a structure (a roof slab lying
/// across two gables like a lid, a floor strip on wall stubs) slides off it: it is shoved
/// out from the structure's middle each tick it lies there, and if it is still there after
/// [`WEDGE_GIVE_UP`] it drops through the remains. What it lay on stands: nothing breaks
/// that nothing struck. No random draw.
fn unwedge(simulation: &mut Simulation) {
    let elapsed = simulation.elapsed;
    for index in 0..simulation.fragments.len() {
        let fragment = &simulation.fragments[index];
        let (Some(debris), Some(created)) = (fragment.structure, fragment.created_at) else {
            continue;
        };
        if debris.rested || elapsed - created < WEDGE_AFTER || fragment.life <= DEBRIS_CLEANUP_SECONDS {
            continue;
        }
        let body = &simulation.world.bodies[fragment.body];
        // A piece standing on the ground leaning on the remains is not wedged up on them:
        // it is laid down ([`lay_down_on_edge`]).
        let foot = body
            .colliders()
            .iter()
            .map(|&collider| simulation.world.colliders[collider].compute_aabb().mins.y)
            .fold(f32::INFINITY, f32::min);
        let resting_up = f64::from(body.center_of_mass().y) >= WEDGE_HEIGHT
            && f64::from(foot) > WEDGE_FOOT
            && f64::from(body.linvel().length()) <= WEDGE_SPEED;
        let on_remains = resting_up && on_remains(simulation, fragment.body);
        if !on_remains {
            if debris.wedged.is_some()
                && !resting_up
                && let Some(debris) = &mut simulation.fragments[index].structure
            {
                debris.wedged = None;
            }
            continue;
        }
        let cover = &simulation.covers[debris.cover];
        let at = from_vector(body.center_of_mass());
        let out = Vec2::new(at.x - cover.x, at.z - cover.z);
        let length = out.x.hypot(out.z).max(1e-6);
        let handle = simulation.fragments[index].body;
        let since = {
            let debris = simulation.fragments[index].structure.as_mut().expect("checked above");
            *debris.wedged.get_or_insert(elapsed)
        };
        if elapsed - since > WEDGE_GIVE_UP {
            // Still up there: it comes down through what holds it.
            set_body_groups(&mut simulation.world, handle, group::STRUCTURE_DEBRIS_THROUGH);
        }
        let body = &mut simulation.world.bodies[handle];
        body.set_linear_damping(FALLING_DAMPING.0);
        body.set_angular_damping(FALLING_DAMPING.1);
        let push = vector(
            out.x / length * WEDGE_PUSH,
            -WEDGE_PUSH * 0.5,
            out.z / length * WEDGE_PUSH,
        );
        body.set_linvel(push, true);
    }
}

/// Falling bodies landing hard on standing parts break them: a standing part takes
/// [`CRUSH_DAMAGE`] for every unit of impulse falling bodies struck it with this tick,
/// from the contact forces past their weight that the step reported (a roof slab coming
/// down on a wall stub, a tower's top on a crate, not one resting there). A body strikes
/// with no more than its momentum (with a bounce), shared among the parts it lands on.
/// Parts are taken in structure, then part order: no random draw.
pub fn crush_structure_parts(simulation: &mut Simulation, contacts: &[ContactForce]) {
    let mut scratch = std::mem::take(&mut simulation.crush_scratch);
    crush_with(simulation, contacts, &mut scratch);
    simulation.crush_scratch = scratch;
}

/// [`crush_structure_parts`]' lists, kept from tick to tick: a collapse reports blows for
/// many ticks running.
#[derive(Clone, Debug, Default)]
pub struct CrushScratch {
    /// (standing part, falling body, impulse, the body's middle)
    blows: Vec<((usize, usize), RigidBodyHandle, f64, Point3)>,
    bodies: Vec<RigidBodyHandle>,
    /// (standing part, its impulse, the middle of the first body that struck it)
    struck: Vec<((usize, usize), f64, Point3)>,
}

fn crush_with(simulation: &mut Simulation, contacts: &[ContactForce], scratch: &mut CrushScratch) {
    let CrushScratch {
        blows,
        bodies,
        struck,
    } = scratch;
    blows.clear();
    for contact in contacts {
        for (falling, standing) in [
            (contact.collider1, contact.collider2),
            (contact.collider2, contact.collider1),
        ] {
            let Some(body) = simulation
                .world
                .colliders
                .get(falling)
                .and_then(|collider| collider.parent())
            else {
                continue;
            };
            let from = from_vector(simulation.world.bodies[body].center_of_mass());
            let Some(part) = simulation.structure_part(standing, from) else {
                continue;
            };
            // A body being shoved off the remains presses on them; it strikes nothing.
            if !simulation.fragments.iter().any(|fragment| {
                fragment.body == body
                    && fragment
                        .structure
                        .is_some_and(|debris| debris.wedged.is_none())
            }) {
                continue;
            }
            let impulse = f64::from(contact.total_force_magnitude) * STEP;
            blows.push((part, body, impulse, from));
        }
    }
    if blows.is_empty() {
        return;
    }
    bodies.clear();
    bodies.extend(blows.iter().map(|blow| blow.1));
    bodies.sort_by_key(|body| body.into_raw_parts());
    bodies.dedup();
    for &body in bodies.iter() {
        // How fast the body came at each part it struck: last tick's velocity, turning
        // about its middle as it turns now (a strip pivoting on a stub's edge barely moves
        // there, however fast its top swings).
        let velocity = simulation
            .fragments
            .iter()
            .find(|fragment| fragment.body == body)
            .and_then(|fragment| fragment.structure)
            .map_or(Point3::new(0.0, 0.0, 0.0), |debris| debris.velocity);
        let rigid = &simulation.world.bodies[body];
        let (middle, spin) = (from_vector(rigid.center_of_mass()), rigid.angvel());
        let mut speed: f64 = 0.0;
        for blow in blows.iter_mut().filter(|blow| blow.1 == body) {
            let (cover, part) = blow.0;
            let Some(at) = part_center(&simulation.covers[cover], part) else {
                continue;
            };
            let arm = vector(at.x - middle.x, at.y - middle.y, at.z - middle.z);
            let turning = from_vector(spin.cross(arm));
            let (vx, vy, vz) = (velocity.x + turning.x, velocity.y + turning.y, velocity.z + turning.z);
            let there = vx.hypot(vy).hypot(vz);
            if there < CRUSH_SPEED {
                blow.2 = 0.0;
            } else {
                speed = speed.max(there);
            }
        }
        let total: f64 = blows.iter().filter(|blow| blow.1 == body).map(|blow| blow.2).sum();
        let carried = f64::from(simulation.world.bodies[body].mass()) * speed * CRUSH_BOUNCE;
        if total > carried && total > 0.0 {
            let scale = carried / total;
            for blow in blows.iter_mut().filter(|blow| blow.1 == body) {
                blow.2 *= scale;
            }
        }
    }
    blows.retain(|blow| blow.2 > 0.0);
    struck.clear();
    for &(part, _, impulse, from) in blows.iter() {
        match struck.iter_mut().find(|(key, _, _)| *key == part) {
            Some((_, total, _)) => *total += impulse,
            None => struck.push((part, impulse, from)),
        }
    }
    struck.sort_by_key(|((cover, part), _, _)| (*cover, *part));
    for &((cover, part), impulse, from) in struck.iter() {
        let Some(center) = part_center(&simulation.covers[cover], part) else {
            continue;
        };
        let along = Vec2::new(center.x - from.x, center.z - from.z);
        if simulation.strike_structure(cover, part, impulse * CRUSH_DAMAGE, false, center, along, Some(NOBODY)) {
            // What broke it has landed, there and then.
            for &(_, body, _, _) in blows.iter().filter(|blow| blow.0 == (cover, part)) {
                if let Some(debris) = simulation
                    .fragments
                    .iter_mut()
                    .find(|fragment| fragment.body == body)
                    .and_then(|fragment| fragment.structure.as_mut())
                {
                    debris.struck = true;
                }
            }
        }
    }
}

/// Whether `fragment` is a fallen body of a structure's parts.
pub fn structure_debris(fragment: &Fragment) -> bool {
    fragment.structure.is_some()
}

/// The fragment whose body holds `collider`: for a structure's fallen parts any of its
/// colliders, for other debris its first.
pub fn fragment_with_collider(simulation: &Simulation, collider: ColliderHandle) -> Option<usize> {
    let body = simulation.world.colliders.get(collider)?.parent()?;
    simulation.fragments.iter().position(|fragment| {
        fragment.body == body
            && (fragment.structure.is_some()
                || simulation.world.bodies[body].colliders().first() == Some(&collider))
    })
}

/// The world position of structure `cover`'s part `index`'s middle.
pub fn part_center(cover: &Cover, index: usize) -> Option<Point3> {
    let state = cover.structure.as_ref()?;
    let part = state.layout.parts.get(index)?;
    let center = state.layout.to_world(part.middle());
    Some(Point3::new(cover.x + center.x, center.y, cover.z + center.z))
}

/// Structure `cover`'s part `index` as a world box (min, max).
pub fn part_box(cover: &Cover, index: usize) -> Option<(Point3, Point3)> {
    let state = cover.structure.as_ref()?;
    if index >= state.layout.parts.len() {
        return None;
    }
    let (min, max) = state.layout.world_box(index);
    Some((
        Point3::new(cover.x + min.x, min.y, cover.z + min.z),
        Point3::new(cover.x + max.x, max.y, cover.z + max.z),
    ))
}

/// Whether every cover of `kind` is built of parts ([`super::structure::structure_of`]).
pub fn structural(kind: CoverKind) -> bool {
    matches!(
        kind,
        CoverKind::House | CoverKind::Tower | CoverKind::Timber | CoverKind::Cargo | CoverKind::Tree
    )
}
