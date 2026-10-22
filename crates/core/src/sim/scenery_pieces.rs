//! Physical breakup of destroyed scenery into its authored major components. Dust, foliage
//! and chips remain presentation particles. Dimensions are shared by simple colliders and
//! instanced unit geometry.

use std::f64::consts::PI;

use glam::DVec3;
use rapier3d::prelude::{ColliderBuilder, Pose, RigidBodyBuilder, RigidBodyType, Rotation, Vector};

use super::crate_layout::{CrateBreak, CratePiece, CrateSection, crate_sections};
use super::data::group;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{DebrisMaterial, debris_material, track_debris_contacts};
use super::house_layout::{
    HOUSE_HEAP, HouseDesign, HousePart, HousePiece, PLINTH, RoofForm, WALL_DEPTH, wall_frames,
};
use super::math::{Point3, Quat4, Random, Vec2};
use super::physics::{
    convex_hull, from_rotation, from_vector, interaction_groups, to_rotation, to_vector, vector,
};
use super::simulation::Simulation;
use super::simulation_rules::GRAVITY;
use super::timber_layout::{TimberWall, timber_damage_stage, timber_parts};
use super::tower_layout::{TOWER_BASE, TowerPiece};
use super::tree_proportions::tree_proportions;
use super::types::{Cover, CoverKind, Fragment, FragmentShape};
use crate::geometry::math::smoothstep;

/// How a collapsed cottage's stub of wall is led down: from its pose when the
/// cottage stood (`origin`, `rotation`), dropped onto the ground where the plinth
/// was and leaned `lean` radians in about `axis` through its foot (`pivot`, at
/// its standing height), all in world space. Led rather than thrown, the stub
/// stands as a ragged ruin the falling roof rests against instead of being
/// bowled out flat round the footprint; it is let go once the pile settles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseStub {
    pub origin: Vector,
    pub rotation: Rotation,
    pub pivot: Vector,
    pub axis: Vector,
    pub lean: f32,
}

/// A movable cover's world pose when it was destroyed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverPose {
    pub position: Point3,
    pub rotation: Quat4,
}

/// Seconds a scenery piece stays solid before its fade, and its hard deadline.
const PIECE_LIFE: f64 = 9.5;
const PIECE_DEADLINE: f64 = 18.0;
/// Draws the old fragment path consumed per piece: 3 placement/size and 8 body/lifetime.
const LEGACY_DRAWS_PER_PIECE: usize = 11;

struct Breakup<'a> {
    cover: &'a Cover,
    rng: Random,
    pieces: Vec<u32>,
}

impl Breakup<'_> {
    /// A box-shaped piece (a cylinder for logs).
    fn piece(
        &mut self,
        simulation: &mut Simulation,
        shape: FragmentShape,
        center: [f64; 3],
        size: [f64; 3],
        color: Option<u32>,
        material: DebrisMaterial,
    ) -> usize {
        let [w, h, d] = size;
        let builder = if shape == FragmentShape::Log {
            ColliderBuilder::cylinder((h / 2.0) as f32, (w / 2.0) as f32)
        } else {
            ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32)
        };
        self.piece_with(simulation, shape, center, size, color, material, builder)
    }

    /// A piece centred at `[x, y, z]` (relative to the cover, y from the ground)
    /// whose main collider is `builder`; `[w, h, d]` are its nominal dimensions.
    #[allow(clippy::too_many_arguments)]
    fn piece_with(
        &mut self,
        simulation: &mut Simulation,
        shape: FragmentShape,
        [x, y, z]: [f64; 3],
        [w, h, d]: [f64; 3],
        color: Option<u32>,
        material: DebrisMaterial,
        builder: ColliderBuilder,
    ) -> usize {
        let cover = self.cover;
        simulation.reserve_fragments(1);
        let id = simulation.allocate_id();
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(cover.x + x, y, cover.z + z))
                .linear_damping(0.12)
                .angular_damping(0.25)
                .can_sleep(true),
        );
        let surface = debris_material(material);
        let density = if material == DebrisMaterial::Metal {
            0.65
        } else {
            0.35
        };
        let collider = simulation.world.insert_collider(
            builder
                .collision_groups(interaction_groups(group::PUSHABLE_DEBRIS))
                .mass(0.18f64.max(w * h * d * density) as f32)
                .friction(surface.friction as f32)
                .restitution(surface.restitution as f32),
            Some(body),
        );
        let angle = self.rng.range(0.0, PI * 2.0);
        let outward = x.hypot(z);
        let (nx, nz) = if outward > 0.1 {
            (x / outward, z / outward)
        } else {
            (angle.cos(), angle.sin())
        };
        let speed = self.rng.range(2.0, 5.0);
        let rigid_body = &mut simulation.world.bodies[body];
        let mass = rigid_body.mass() as f64;
        let lift = self.rng.range(3.0, 7.0);
        rigid_body.apply_impulse_at_point(
            vector(nx * speed * mass, lift * mass, nz * speed * mass),
            vector(cover.x + x + w * 0.15, y - h * 0.2, cover.z + z + d * 0.15),
            true,
        );
        track_debris_contacts(simulation, body, collider, id, material);
        let mut fragment = Fragment::new(
            id,
            body,
            PIECE_LIFE + DEBRIS_CLEANUP_SECONDS,
            1.0,
            color.unwrap_or(cover.color),
        );
        fragment.shape = Some(shape);
        fragment.dimensions = Some(Point3::new(w, h, d));
        fragment.material = Some(material);
        fragment.source_kind = Some(cover.kind);
        fragment.expires_at = Some(simulation.elapsed + PIECE_DEADLINE);
        simulation.fragments.push(fragment);
        self.pieces.push(id);
        simulation.fragments.len() - 1
    }

    /// A section of a broken crate where it stood, thrown away from the shot that
    /// broke it: the struck wall caves in and is carried through, the far wall and
    /// lid blow out, the sides spread, and every section tumbles, all the harder for
    /// a larger `force` (the crate's, about 1). Without a known direction the crate
    /// bursts outwards.
    fn crate_section(
        &mut self,
        simulation: &mut Simulation,
        section: &CrateSection,
        kick: Option<Vec2>,
        force: f64,
    ) {
        let shape = match section.piece {
            CratePiece::Skid | CratePiece::Plank => FragmentShape::Beam,
            _ => FragmentShape::Panel,
        };
        let index = self.piece(
            simulation,
            shape,
            section.center,
            section.size,
            None,
            DebrisMaterial::Wood,
        );
        simulation.fragments[index].crate_piece = Some(section.piece);
        let [nx, ny, nz] = section.normal;
        let rng = &mut self.rng;
        let lid = matches!(
            section.piece,
            CratePiece::Lid | CratePiece::LidFront | CratePiece::LidMiddle | CratePiece::LidBack
        );
        // (speed along the shot, speed out of the section's face, lift, spin)
        let (carried, outward, lift, spin) = match section.piece {
            CratePiece::Skid => (rng.range(1.0, 2.0), 0.0, rng.range(1.5, 2.5), 3.0),
            CratePiece::Plank => (rng.range(9.0, 12.0), 0.0, rng.range(4.0, 6.0), 12.0),
            _ if lid => (rng.range(2.0, 4.0), 0.0, rng.range(5.0, 8.0), 6.0),
            _ => {
                let lift = rng.range(3.0, 6.0);
                match kick.map(|kick| nx * kick.x + nz * kick.z) {
                    // The struck wall caves in and is carried through.
                    Some(facing) if facing < -0.5 => (rng.range(6.0, 9.0), 0.5, lift, 8.0),
                    // The far wall blows out ahead of it.
                    Some(facing) if facing > 0.5 => {
                        (rng.range(5.0, 8.0), rng.range(1.0, 2.0), lift, 8.0)
                    }
                    // The sides spread.
                    Some(_) => (rng.range(2.5, 4.5), rng.range(2.5, 4.5), lift, 8.0),
                    None => (0.0, rng.range(4.0, 7.0), lift, 8.0),
                }
            }
        };
        let (carried, outward, lift, spin) =
            (carried * force, outward * force, lift * force, spin * force);
        let (vx, vz) = match kick {
            Some(kick) => (
                kick.x * carried + nx * outward,
                kick.z * carried + nz * outward,
            ),
            None => {
                // Without a direction the crate bursts outwards; the lid, skid and
                // plank scatter at random.
                let angle = rng.range(0.0, PI * 2.0);
                let speed = carried.max(outward).max(2.0);
                if ny == 0.0 && section.piece != CratePiece::Plank {
                    (nx * outward, nz * outward)
                } else {
                    (angle.cos() * speed, angle.sin() * speed)
                }
            }
        };
        let angular = [
            rng.range(-spin, spin),
            rng.range(-spin, spin) * 0.6,
            rng.range(-spin, spin),
        ];
        let [rx, ry, rz] = section.rotation;
        let body = simulation.fragments[index].body;
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_rotation(to_rotation(Quat4::from_euler_xyz(rx, ry, rz)), true);
        rigid_body.set_linvel(vector(vx, lift, vz), true);
        rigid_body.set_angvel(vector(angular[0], angular[1], angular[2]), true);
        // Thin boards thrown hard would otherwise tunnel through each other.
        rigid_body.enable_ccd(true);
    }
}

/// Break destroyed scenery into physical pieces. Returns false for kinds without authored
/// pieces, whose callers fall back to small fragments.
pub fn break_scenery(
    simulation: &mut Simulation,
    cover_index: usize,
    pose: Option<CoverPose>,
    previous_hp: f64,
) -> bool {
    let mut cover = simulation.covers[cover_index].clone();
    // Navigation bounds expand as a barrel tips; fragments keep its original dimensions.
    if let Some(motion) = cover.motion {
        cover.w = motion.w;
        cover.d = motion.d;
    }
    let legacy_count = match cover.kind {
        CoverKind::Tower => 10,
        CoverKind::Tree => 9,
        CoverKind::Timber => 7,
        CoverKind::Cargo | CoverKind::Drum => 3,
        // Cottages stood for good before they could fall: they have no legacy draws.
        CoverKind::House => 0,
        _ => return false,
    };
    // Preserve the old fragment path's draws so cosmetic authoring cannot reshuffle seeded
    // combat and bot decisions. New piece motion uses its own stream below.
    for _ in 0..legacy_count * LEGACY_DRAWS_PER_PIECE {
        simulation.rng.next();
    }
    let mut breakup = Breakup {
        cover: &cover,
        rng: Random::new(cover.id as f64 * 73_856_093.0 + simulation.seed),
        pieces: Vec::new(),
    };
    let wood = DebrisMaterial::Wood;
    match cover.kind {
        CoverKind::Cargo => {
            // Each wall and the lid come away whole or in pieces, with a plank or
            // two and maybe a skid (5 to 18 bodies, about 11), each drawn as its
            // section of the crate, every crate differently and with its own force.
            // Also seeded by how many covers have fallen, so a crate that rebuilds
            // in place breaks differently each time.
            breakup.rng.state += f64::from(simulation.destroyed) * 19_349_663.0;
            let how = CrateBreak::random(|| breakup.rng.next());
            let force = breakup.rng.range(0.75, 1.3);
            for section in crate_sections(cover.w, cover.d, cover.h, &how) {
                breakup.crate_section(simulation, &section, cover.kick, force);
            }
        }
        CoverKind::Timber => {
            let stage = timber_damage_stage(previous_hp, cover.max_hp);
            for part in timber_parts(&TimberWall::of(&cover), stage) {
                let index = breakup.piece(
                    simulation,
                    FragmentShape::Beam,
                    [part.x, part.y, part.z],
                    [part.w, part.h, part.d],
                    Some(part.color),
                    wood,
                );
                let body = simulation.fragments[index].body;
                let rotation = Quat4::from_euler_xyz(0.0, part.yaw, part.lean);
                simulation.fragments[index].timber_part = Some(part);
                let collider = simulation.world.bodies[body].colliders()[0];
                simulation.world.colliders[collider]
                    .set_collision_groups(interaction_groups(group::TIMBER_DEBRIS));
                let rigid_body = &mut simulation.world.bodies[body];
                rigid_body.enable_ccd(true);
                rigid_body.set_additional_solver_iterations(2);
                if let Some(kick) = cover.kick {
                    let velocity = from_vector(rigid_body.linvel());
                    rigid_body.set_linvel(
                        vector(
                            velocity.x * 0.45 + kick.x * 3.0,
                            velocity.y * 0.65,
                            velocity.z * 0.45 + kick.z * 3.0,
                        ),
                        true,
                    );
                }
                rigid_body.set_rotation(to_rotation(rotation), true);
            }
        }
        CoverKind::Tree => {
            let proportions = tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h);
            let (family, height, radius, stump) = (
                proportions.family,
                proportions.height,
                proportions.radius,
                proportions.stump_height,
            );
            let length = height * if family < 3 { 0.98 } else { 0.78 } - stump;
            let center = stump + length / 2.0;
            let trunk = breakup.piece(
                simulation,
                FragmentShape::Log,
                [0.0, center, 0.0],
                [radius * 2.0, length, radius * 2.0],
                Some(0x98734f),
                wood,
            );
            simulation.fragments[trunk].tree_cover_id = Some(cover.id);
            simulation.fragments[trunk].tree_center_y = Some(center);
            let trunk_body = simulation.fragments[trunk].body;
            // A light crown volume keeps foliage above the ground as the trunk rolls.
            simulation.world.insert_collider(
                ColliderBuilder::ball((cover.w.min(cover.d) * 0.34) as f32)
                    .translation(vector(0.0, height * 0.72 - center, 0.0))
                    .collision_groups(interaction_groups(group::FRAGMENT))
                    .mass(0.12)
                    .friction(0.9)
                    .restitution(0.05),
                Some(trunk_body),
            );
            // A small sideways lean initiates a gravity-driven fall instead of a launch.
            let angle = breakup.rng.range(0.0, PI * 2.0);
            let rigid_body = &mut simulation.world.bodies[trunk_body];
            rigid_body.set_linvel(vector(angle.sin() * 0.45, 0.0, angle.cos() * 0.45), true);
            rigid_body.set_angvel(vector(angle.cos() * 0.65, 0.0, -angle.sin() * 0.65), true);
            breakup.piece(
                simulation,
                FragmentShape::Beam,
                [radius, stump, 0.0],
                [radius * 0.3, radius * 1.4, radius * 0.25],
                Some(0xb59a69),
                wood,
            );
        }
        CoverKind::Drum => {
            // Internal pressure tears the thin wall into small curled sheets, not a
            // surviving cylinder. Offset each sheet so the blast spreads them radially.
            let phase = breakup.rng.range(0.0, PI * 2.0);
            for i in 0..3 {
                let angle = phase + (i as f64 * PI * 2.0) / 3.0;
                let y = cover.h * breakup.rng.range(0.3, 0.6);
                let w = cover.w * breakup.rng.range(0.28, 0.4);
                let h = cover.h * breakup.rng.range(0.25, 0.4);
                let scrap = breakup.piece(
                    simulation,
                    FragmentShape::DrumShell,
                    [angle.cos() * cover.w * 0.3, y, angle.sin() * cover.d * 0.3],
                    [w, h, 0.12],
                    Some(if i == 1 { 0x493e35 } else { 0x765443 }),
                    DebrisMaterial::Metal,
                );
                let body = simulation.fragments[scrap].body;
                simulation.world.bodies[body].set_rotation(to_rotation(Quat4::yaw(-angle)), true);
            }
            breakup.piece(
                simulation,
                FragmentShape::DrumLid,
                [0.0, cover.h, 0.0],
                [cover.w * 0.65, 0.16, cover.w * 0.65],
                Some(0x574e3e),
                DebrisMaterial::Metal,
            );
        }
        CoverKind::Tower => topple_tower(simulation, &mut breakup),
        CoverKind::House => collapse_house(simulation, &mut breakup),
        _ => return false,
    }
    if let Some(pose) = pose {
        // Carry the authored breakup into the barrel's current world pose before the blast.
        let rotation = pose.rotation;
        for id in &breakup.pieces {
            let Some(fragment) = simulation
                .fragments
                .iter()
                .find(|fragment| fragment.id == *id)
            else {
                continue;
            };
            let body = &mut simulation.world.bodies[fragment.body];
            let p = from_vector(body.translation());
            let local = Point3::new(p.x - cover.x, p.y - cover.h / 2.0, p.z - cover.z);
            let moved = rotation.rotate(local);
            body.set_translation(
                to_vector(Point3::new(
                    moved.x + pose.position.x,
                    moved.y + pose.position.y,
                    moved.z + pose.position.z,
                )),
                true,
            );
            let q = from_rotation(*body.rotation());
            body.set_rotation(to_rotation(rotation.multiply(q)), true);
            let velocity = rotation.rotate(from_vector(body.linvel()));
            let spin = rotation.rotate(from_vector(body.angvel()));
            body.set_linvel(to_vector(velocity), true);
            body.set_angvel(to_vector(spin), true);
        }
    }
    true
}

/// Angular speed (rad/s) the watchtower's top starts tipping at, and how fast its
/// legs give way.
const TOWER_TIP: (f64, f64) = (0.55, 0.85);
const TOWER_SAG: f64 = 1.2;
/// How fast the legs and bracing slide, and how fast they fold out of their
/// own plane.
const LEG_KICK: (f64, f64) = (1.7, 2.3);
const LEG_SPIN: (f64, f64) = (1.2, 1.6);
/// Extra outward speed that peels the cabin walls off the falling deck.
const WALL_PEEL: (f64, f64) = (0.4, 1.0);
/// Random spin added to every piece about each axis.
const PIECE_WOBBLE: f64 = 0.25;

/// Break the watchtower into the parts its model is built from and topple it
/// away from the hit that destroyed it: the legs splay apart and the bracing is
/// crushed, so nothing holds the deck up, while the deck, cabin and roof tip
/// over about the footings and sag, crashing down beside the tower rather than
/// bursting upward.
fn topple_tower(simulation: &mut Simulation, breakup: &mut Breakup) {
    let fall = breakup.cover.kick.unwrap_or_else(|| {
        let angle = breakup.rng.range(0.0, PI * 2.0);
        Vec2::new(angle.cos(), angle.sin())
    });
    let fall = DVec3::new(fall.x, 0.0, fall.z);
    // Turning about up x fall tips the top toward `fall`.
    let axis = DVec3::Y.cross(fall);
    let tip = breakup.rng.range(TOWER_TIP.0, TOWER_TIP.1);
    let pivot = DVec3::new(0.0, TOWER_BASE.height, 0.0);
    for piece in TowerPiece::ALL {
        let bounds = piece.bounds();
        let center = DVec3::from_array(bounds.center);
        let [w, h, d] = bounds.size;
        let builder = match piece.hull() {
            Some(points) => {
                let local: Vec<f32> = points
                    .iter()
                    .flat_map(|p| (DVec3::from_array(*p) - center).to_array())
                    .map(|v| v as f32)
                    .collect();
                convex_hull(&local)
            }
            None => ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32),
        };
        let material = if piece.metal() {
            DebrisMaterial::Metal
        } else {
            DebrisMaterial::Wood
        };
        let index = breakup.piece_with(
            simulation,
            piece.shape(),
            bounds.center,
            bounds.size,
            None,
            material,
            builder,
        );
        simulation.fragments[index].tower_piece = Some(piece);
        let body = simulation.fragments[index].body;
        let surface = debris_material(material);
        for extra in piece.extra_boxes() {
            let [ew, eh, ed] = extra.size;
            let offset = DVec3::from_array(extra.center) - center;
            simulation.world.insert_collider(
                ColliderBuilder::cuboid((ew / 2.0) as f32, (eh / 2.0) as f32, (ed / 2.0) as f32)
                    .translation(vector(offset.x, offset.y, offset.z))
                    // Like a felled crown, these only keep the piece off the ground.
                    .collision_groups(interaction_groups(group::FRAGMENT))
                    .mass(0.05)
                    .friction(surface.friction as f32)
                    .restitution(surface.restitution as f32),
                Some(body),
            );
        }
        let wobble = DVec3::new(
            breakup.rng.range(-PIECE_WOBBLE, PIECE_WOBBLE),
            breakup.rng.range(-PIECE_WOBBLE, PIECE_WOBBLE),
            breakup.rng.range(-PIECE_WOBBLE, PIECE_WOBBLE),
        );
        let (linear, angular) = if piece.grounded() {
            let kick = breakup.rng.range(LEG_KICK.0, LEG_KICK.1);
            let spin = breakup.rng.range(LEG_SPIN.0, LEG_SPIN.1);
            let side = piece.side();
            if matches!(piece, TowerPiece::WestBent | TowerPiece::EastBent) {
                // A bent stands in a y-z plane, so it folds about z: the two
                // bents splay apart, tops outward, and the deck drops between.
                (
                    DVec3::X * side * kick * 0.5 + fall * kick * 0.3,
                    -DVec3::Z * side * spin + wobble,
                )
            } else {
                // The face bracing stands in an x-y plane and falls flat outward.
                let collider = simulation.world.bodies[body].colliders()[0];
                simulation.world.colliders[collider]
                    .set_collision_groups(interaction_groups(group::CRUSHED_FRAME));
                (
                    DVec3::Z * side * kick * 0.25 + fall * kick * 0.4,
                    DVec3::X * side * spin + wobble,
                )
            }
        } else {
            let mut linear = axis.cross(center - pivot) * tip - DVec3::Y * TOWER_SAG;
            if matches!(
                piece,
                TowerPiece::FrontWall
                    | TowerPiece::BackWall
                    | TowerPiece::WestWall
                    | TowerPiece::EastWall
            ) {
                let out = DVec3::new(center.x, 0.0, center.z).normalize_or_zero();
                linear += out * breakup.rng.range(WALL_PEEL.0, WALL_PEEL.1);
            }
            (linear, axis * tip + wobble)
        };
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_linvel(vector(linear.x, linear.y, linear.z), true);
        rigid_body.set_angvel(vector(angular.x, angular.y, angular.z), true);
    }
}

/// How fast (rad/s) a cottage's upper wall shards start to tip over the break
/// under them, and the chance one folds in onto the floor rather than out: the
/// roof comes down on the walls that fall in, so the wreck keeps its siding face
/// up under the roof instead of laying its plaster out round the footprint like
/// an unfolded box. Likelier still on the wall the shot struck.
const WALL_TIP: (f64, f64) = (2.0, 2.6);
const FALLS_IN: f64 = 0.96;
const STRUCK_FALLS_IN: f64 = 1.0;
const FAR_FALLS_IN: f64 = 0.85;
/// The lower shards stand on as ragged stubs of wall round the pile, leaning in
/// against it this far (rad), over this long (s) from when they lean, and let
/// go as loose debris once the pile has settled.
const STUB_LEAN: (f64, f64) = (0.12, 0.32);
const STUB_LEAN_TIME: f64 = 0.3;
const STUB_RELEASE: f64 = 3.5;
/// The chance a gable wall's upper shard folds in under the roof's rake.
const GABLE_FALLS_IN: f64 = 0.4;

/// How fast (m/s) an upper wall shard starts down as the wall under it gives way,
/// and how fast it is carried clear of the stub under it falling in and out.
const UPPER_DROP: f64 = 0.1;
const UPPER_CLEAR: (f64, f64) = (0.3, 0.0);
/// How much slower an upper shard tips falling out than in.
const OUT_TIP: f64 = 0.6;
/// When (s) a stub of wall, landed, starts to lean in.
const STUB_LEAN_AT: f64 = 0.05;
/// Brick spill from the snapped chimney: count, brick size and how hard they are
/// thrown off the stack (m/s): enough to rattle down the pile and spill round it.
const LOOSE_BRICKS: usize = 12;
const BRICK_SIZE: [f64; 3] = [0.24, 0.08, 0.12];
const BRICK_THROW: (f64, f64) = (0.4, 1.6);
/// Sideways drift (m/s) and twist (rad/s) that keep neighbouring shards from
/// falling as one sheet.
const WALL_DRIFT: f64 = 0.35;
const WALL_TWIST: f64 = 0.45;
/// How fast the roof's slabs fold in about the ridge as it gives way (rad/s) and
/// how fast they drop: the roof pancakes, coming down onto the buckling walls
/// still lying as it stood, its ridge sagging in, rather than hinging out.
const ROOF_FOLD: (f64, f64) = (0.1, 0.25);
const ROOF_DROP: (f64, f64) = (0.0, 0.4);
/// Damping that settles the heavy shards into the pile instead of tumbling; a
/// roof slab's spin is held harder, so a knock from a wall does not flip it.
const SHARD_DAMPING: (f32, f32) = (0.3, 0.9);
const ROOF_DAMPING: (f32, f32) = (0.3, 4.0);
/// A stub of wall's damping and how many times heavier than its panel it is:
/// it settles where it leans instead of being bowled over by the pile.
const STUB_DAMPING: (f32, f32) = (3.0, 2.0);
const STUB_WEIGHT: f32 = 4.0;
/// Density of a cottage shard's collider (per cubic metre of its hull), and the
/// share of it an eave slab's overhang takes: thin boards and a gutter.
const SHARD_DENSITY: f32 = 0.35;
const OVERHANG_DENSITY: f32 = 0.3;
/// Seconds a falling roof slab keeps its pitch before it may turn: long enough to
/// come down onto the wreck as a roof, not a tumbling sheet.
const ROOF_HOLD: f64 = 0.9;
/// Seconds the roof hangs before it gives way onto the buckling walls, and how
/// fast (m/s) it starts down.
const ROOF_GIVE: f64 = 0.2;
const ROOF_GIVE_DROP: f32 = 0.5;
/// How fast (m/s) the ridge slabs start down as the roof gives, ahead of the eave
/// slabs held up on the walls: the ridge snaps and caves into the house first, so
/// the roof comes down broken, not as one sheet that lands whole on the pile.
const RIDGE_GIVE_DROP: f32 = 1.8;
/// The twist (rad/s about its face's normal) a ridge slab takes as it is freed to
/// turn, each strip the other way, so the broken ridge lies skewed, not straight.
const RIDGE_SNAP_TWIST: f32 = 0.5;
/// How fast the chimney topples off the roof, and how fast (m/s) it is carried
/// into the wreck: gently, so its stack lands broken on the pile.
const CHIMNEY_TIP: (f64, f64) = (2.0, 2.6);
const CHIMNEY_THROW: f64 = 0.5;
/// The rubble mound's paint (crushed plaster, brick and lath) and its grip, high
/// enough that a shard rests on its slopes.
const MOUND_RUBBLE: u32 = 0x8f8170;
const MOUND_FRICTION: f32 = 1.3;

/// How far the blast carries every piece along its direction (m/s).
const BLAST_CARRY: f64 = 0.7;
/// Rafters thrown loose from the roof: count, section and stain.
const LOOSE_RAFTERS: usize = 3;
const RAFTER_SECTION: f64 = 0.14;
const RAFTER_COLOR: u32 = 0x9a7650;
/// Clapboards and laths torn loose from the walls: count, size and how hard they
/// are thrown out low.
const LOOSE_BOARDS: usize = 5;
const BOARD_SIZE: [f64; 3] = [0.14, 0.03, 1.3];
const BOARD_THROW: (f64, f64) = (2.0, 4.5);
/// How far outside the siding a loose board starts (m), clear of its wall.
const BOARD_CLEARANCE: f64 = 0.4;
/// The chimney's brick, for its tint and dust.
const CHIMNEY_BRICK: u32 = 0x9c4a32;
/// Seconds a cottage's pieces lie on its heap before they sink into it: long
/// enough that the collapse settles into a pile that stays a while; the heap
/// carries the cottage's remains on for the rest of the round.
const HOUSE_PIECE_LIFE: f64 = 12.0;
/// Seconds after a collapse when its slowed, lying shards are held by heavier
/// damping (linear, angular), so the pile comes to rest instead of creeping, the
/// speeds (m/s, rad/s) below which a shard counts as slowed and the height (m) of
/// its centre below which it lies on the pile rather than leans, still to topple.
const SETTLE_AFTER: f64 = 1.6;
const SETTLED_DAMPING: (f32, f32) = (4.0, 6.0);
const SETTLED_SPEED: (f32, f32) = (0.6, 0.8);
const LYING: f32 = 2.2;
/// A shard still standing on edge once the collapse has come down, slowed and
/// propped on the wreck, is tipped over to lie on the pile: the window after the
/// settle in which it is (s), how close to level its face's normal must come to
/// count as standing, and how fast (rad/s, m/s) it is tipped over the way it
/// leans, or away from the footprint's middle onto open ground.
const TOPPLE_WINDOW: (f64, f64) = (1.0, 3.8);
const STANDING: f32 = 0.66;
/// How upright (its axis's rise) a fallen chimney stack may stand before it is
/// knocked over.
const CHIMNEY_UPRIGHT: f32 = 0.75;
/// Tries at tipping a roof slab back shingles up before it is tipped outward.
const ROOF_TOPPLES: u8 = 3;
/// The spin (rad/s per unit of tilt) that starts a roof slab lying back when it
/// is freed to turn.
const ROOF_SETTLE_SPIN: f32 = 1.4;

const TOPPLE_SPIN: f32 = 2.0;
/// Tries at tipping a standing wall shard onto the pile before it is tipped out.
const INWARD_TOPPLES: u8 = 3;
/// How far (m) past the cover box a falling roof slab's or chimney's centre may
/// slide.
const CONTAINED: f64 = 0.5;

const TOPPLE_SHOVE: f32 = 0.7;
/// How far a standing shard's top must lean (sine of the angle) to fall that way,
/// and how many times one is tipped before it is left as it rests.
const LEANING: f32 = 0.08;
const TOPPLES: u8 = 6;
/// A little lift (m/s) with each tip, to clear what the shard's foot is caught on.
const TOPPLE_LIFT: f32 = 0.6;
const LOOSE_LIFE: f64 = 2.5;

/// Bring a cottage down along the cracks it is pre-fractured with, as a pancake:
/// each wall breaks into panels that buckle in onto the floor (out behind, over
/// the heap), each twisting its own way, while the roof's broken slabs come
/// straight down onto them still holding their pitch, then settle shingles up on
/// the wreck; the chimney topples off, rafters and clapboards are thrown loose,
/// and the back wall breaks off above the stub `damage_cover` leaves standing in
/// the heap.
fn collapse_house(simulation: &mut Simulation, breakup: &mut Breakup) {
    let cover = breakup.cover;
    let design = HouseDesign::of(cover.x, cover.z, cover.w, cover.d, cover.h);
    let roof = design.roof();
    let kick = breakup.cover.kick;
    let fall = kick.unwrap_or_else(|| {
        let angle = breakup.rng.range(0.0, PI * 2.0);
        Vec2::new(angle.cos(), angle.sin())
    });
    // Motion is worked out in the build frame and carried into the world.
    let fall = design.to_local(DVec3::new(fall.x, 0.0, fall.z));
    let turn = Quat4::yaw(design.yaw());
    let frames = wall_frames(design.w, design.d);
    for shape in design.pieces() {
        let part = shape.piece.part;
        let local: Vec<f32> = shape
            .hull
            .iter()
            .flat_map(|p| (*p - shape.center).to_array())
            .map(|v| v as f32)
            .collect();
        let (color, material) = match part {
            HousePart::Chimney => (CHIMNEY_BRICK, DebrisMaterial::Concrete),
            HousePart::Mound => (MOUND_RUBBLE, DebrisMaterial::Concrete),
            _ if part.is_roof() => (design.style.roof, DebrisMaterial::Wood),
            _ => (design.style.siding, DebrisMaterial::Wood),
        };
        let at = design.to_world(shape.center);
        let index = breakup.piece_with(
            simulation,
            FragmentShape::Panel,
            at.to_array(),
            shape.size.to_array(),
            Some(color),
            material,
            convex_hull(&local),
        );
        let fragment = &mut simulation.fragments[index];
        fragment.house_piece = Some(shape.piece);
        fragment.house_id = Some(cover.id);
        let body = fragment.body;
        if part == HousePart::Mound {
            // The rubble the walls crush into lies where it heaps: a fixed pile
            // the shards come to rest on, which nothing else touches.
            let rigid_body = &mut simulation.world.bodies[body];
            rigid_body.set_body_type(RigidBodyType::Fixed, true);
            rigid_body.set_rotation(to_rotation(turn), true);
            let collider = rigid_body.colliders()[0];
            let collider = &mut simulation.world.colliders[collider];
            collider.set_collision_groups(interaction_groups(group::HOUSE_MOUND));
            collider.set_friction(MOUND_FRICTION);
            continue;
        }
        let collider = simulation.world.bodies[body].colliders()[0];
        simulation.world.colliders[collider]
            .set_collision_groups(interaction_groups(group::HOUSE_SHARD));
        if !shape.overhang.is_empty() {
            // The slab's reach over the walls, its eaves and gutter, joins it
            // once the roof gives way ([`settle_house_pieces`]).
            let reach: Vec<f32> = shape
                .overhang
                .iter()
                .flat_map(|p| (*p - shape.center).to_array())
                .map(|v| v as f32)
                .collect();
            let surface = debris_material(material);
            simulation.world.insert_collider(
                convex_hull(&reach)
                    .collision_groups(interaction_groups(group::HOUSE_SHARD))
                    .density(SHARD_DENSITY * OVERHANG_DENSITY)
                    .friction(surface.friction as f32)
                    .restitution(surface.restitution as f32)
                    .enabled(false),
                Some(body),
            );
        }
        let rng = &mut breakup.rng;
        let mut stub = None;
        let (linear, angular) = if let Some(wall) = part.wall_index() {
            let (_, out, along, _) = frames[wall];
            let lower = lower_wall(shape.piece);
            // A gable wall's upper shard carries the gable up under the rake: it
            // cannot fold in under the roof, so it mostly falls out.
            let gable = roof.form == RoofForm::Gable && wall < 2 && !lower;
            let facing = out.dot(fall);
            let chance = if gable {
                GABLE_FALLS_IN
            } else if kick.is_some() && facing < -0.5 {
                STRUCK_FALLS_IN
            } else if kick.is_some() && facing > 0.5 {
                FAR_FALLS_IN
            } else {
                FALLS_IN
            };
            // Over the heap a lower shard has no floor to stand on: it falls out
            // behind, leaving the heap's own stub. An upper one may fold in onto
            // the heap and lie across it.
            let over_heap = shape.center.z < -design.d / 2.0 + HOUSE_HEAP.inset + HOUSE_HEAP.depth;
            let inward = if lower {
                !over_heap
            } else {
                rng.next() < chance
            };
            let toward = if inward { -out } else { out };
            // Falling out, an upper shard tips slower and slides down the stub
            // under it to lean on it rather than being flung out flat.
            let tip = rng.range(WALL_TIP.0, WALL_TIP.1) * if inward { 1.0 } else { OUT_TIP };
            // It tips about the foot of the face it falls over.
            let bottom = shape.hull.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let foot = shape.center - out * shape.center.dot(out)
                + out * if inward { -WALL_DEPTH } else { 0.0 }
                + frames[wall].0.dot(out) * out;
            let pivot = DVec3::new(foot.x, bottom, foot.z);
            if lower && !over_heap {
                // A lower shard stays a ragged stub of wall, led down onto the
                // ground where the plinth was and leaned in against the pile
                // ([`settle_house_pieces`]).
                let pivot = design.to_world(DVec3::new(foot.x, bottom, foot.z));
                let axis = design.to_world(DVec3::Y.cross(-out));
                stub = Some(HouseStub {
                    origin: Vector::ZERO,
                    rotation: Rotation::IDENTITY,
                    pivot: vector(cover.x + pivot.x, pivot.y, cover.z + pivot.z),
                    axis: vector(axis.x, axis.y, axis.z),
                    lean: rng.range(STUB_LEAN.0, STUB_LEAN.1) as f32,
                });
                (DVec3::ZERO, DVec3::ZERO)
            } else {
                // An upper one tips over the stub under it, twisting its own way,
                // and is carried clear of the stub's top rather than dragging it
                // along; it does not drop onto it, as landing on its leading edge
                // would kick it back the other way.
                let angular = DVec3::Y.cross(toward) * tip
                    + DVec3::Y * rng.range(-WALL_TWIST, WALL_TWIST)
                    + toward * rng.range(-0.3, 0.3);
                let clear = if inward { UPPER_CLEAR.0 } else { UPPER_CLEAR.1 };
                (
                    angular.cross(shape.center - pivot)
                        + along * rng.range(-WALL_DRIFT, WALL_DRIFT)
                        + toward * clear
                        - DVec3::Y * UPPER_DROP
                        + fall * BLAST_CARRY * 0.4,
                    angular,
                )
            }
        } else if part.is_roof() {
            let outward = DVec3::new(shape.center.x, 0.0, shape.center.z).normalize_or_zero();
            let wobble = DVec3::new(
                rng.range(-0.08, 0.08),
                rng.range(-0.1, 0.1),
                rng.range(-0.08, 0.08),
            );
            let drop = -DVec3::Y * rng.range(ROOF_DROP.0, ROOF_DROP.1) + fall * BLAST_CARRY * 0.3;
            let fold = rng.range(ROOF_FOLD.0, ROOF_FOLD.1);
            (drop, DVec3::Y.cross(-outward) * fold + wobble)
        } else if shape.piece.shard == 0 {
            // The stump drops with the roof it pierced, leaning off it.
            let away = DVec3::new(shape.center.x, 0.0, shape.center.z).normalize_or_zero();
            (
                -DVec3::Y * 0.6 + away * 0.2,
                DVec3::Y.cross(away) * rng.range(0.3, 0.7),
            )
        } else {
            // The top topples into the wreck (or the way a blast drives it), to
            // lie broken on the pile rather than bowl out across the yard.
            let away = DVec3::new(shape.center.x, 0.0, shape.center.z).normalize_or(DVec3::X);
            let toward = if kick.is_some() { fall } else { -away };
            let tip = rng.range(CHIMNEY_TIP.0, CHIMNEY_TIP.1);
            (
                toward * CHIMNEY_THROW - DVec3::Y * 0.4,
                DVec3::Y.cross(toward) * tip + DVec3::Y * rng.range(-0.4, 0.4),
            )
        };
        let [linear, angular] = [linear, angular].map(|v| {
            let world = design.to_world(v);
            vector(world.x, world.y, world.z)
        });
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_rotation(to_rotation(turn), true);
        rigid_body.set_linvel(linear, true);
        rigid_body.set_angvel(angular, true);
        if let Some(mut stub) = stub {
            stub.origin = rigid_body.translation();
            stub.rotation = *rigid_body.rotation();
            rigid_body.set_body_type(RigidBodyType::KinematicPositionBased, true);
            simulation.fragments[index].house_stub = Some(stub);
        }
        let rigid_body = &mut simulation.world.bodies[body];
        let (linear, angular) = shard_damping(shape.piece, 0.0);
        rigid_body.set_linear_damping(linear);
        rigid_body.set_angular_damping(angular);
        // Weighed by what the shard is, not its bounds: a roof slab's sloping box
        // would make it many times its walls' weight, and it would bowl them out
        // of its way. A stub of wall carries the framing and masonry below the
        // break, heavy enough that the pile landing on it does not knock it over.
        let collider = rigid_body.colliders()[0];
        let density = if lower_wall(shape.piece) {
            SHARD_DENSITY * STUB_WEIGHT
        } else {
            SHARD_DENSITY
        };
        simulation.world.colliders[collider].set_density(density);
        let rigid_body = &mut simulation.world.bodies[body];
        if part.is_roof() {
            // The roof comes straight down as it stood, crushing the walls under
            // it and landing on their stubs, and only then breaks along its
            // rafter lines and settles on the wreck.
            rigid_body.lock_rotations(true, true);
        }
        if part.is_roof() {
            // The roof hangs a moment while the walls under it buckle, so they
            // fold in beneath it rather than being shoved out by its edge.
            rigid_body.lock_translations(true, true);
        }
    }
    // Bricks spilling off the snapped stack, thrown the way the top falls.
    let flue = design.flue();
    let snap = design.chimney_snap();
    let toward = if kick.is_some() {
        fall
    } else {
        -DVec3::new(flue.x, 0.0, flue.z).normalize_or(DVec3::X)
    };
    for i in 0..LOOSE_BRICKS {
        let around = i as f64 / LOOSE_BRICKS as f64 * PI * 2.0;
        // Courses shed down the stack as well as at the snap.
        let course = (i % 3) as f64 * 0.22;
        let local = DVec3::new(
            flue.x + around.cos() * 0.45,
            snap - course + breakup.rng.range(-0.1, 0.2),
            flue.z + around.sin() * 0.45,
        );
        let at = design.to_world(local);
        let index = breakup.piece(
            simulation,
            FragmentShape::Beam,
            at.to_array(),
            BRICK_SIZE,
            Some(CHIMNEY_BRICK),
            DebrisMaterial::Concrete,
        );
        let body = simulation.fragments[index].body;
        let spread = DVec3::new(around.cos(), 0.0, around.sin());
        let throw = design.to_world(
            toward * breakup.rng.range(BRICK_THROW.0, BRICK_THROW.1)
                + spread * breakup.rng.range(0.3, 1.0)
                + DVec3::Y * breakup.rng.range(0.5, 1.8),
        );
        let spin = DVec3::new(
            breakup.rng.range(-8.0, 8.0),
            breakup.rng.range(-6.0, 6.0),
            breakup.rng.range(-8.0, 8.0),
        );
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_rotation(
            to_rotation(Quat4::from_euler_xyz(0.0, breakup.rng.range(0.0, PI), 0.0)),
            true,
        );
        rigid_body.set_linvel(vector(throw.x, throw.y, throw.z), true);
        rigid_body.set_angvel(vector(spin.x, spin.y, spin.z), true);
        rigid_body.enable_ccd(true);
    }
    // Rafters from under the roof, thrown loose across the slopes. They start
    // clear of the walls and the deck, low in the rooms, so they never shove the
    // roof as it comes down.
    let length = (design.w / 2.0 - WALL_DEPTH - 0.3)
        .min((roof.eave_x * roof.eave_x + (roof.ridge - roof.eave_y).powi(2)).sqrt() * 0.75);
    for i in 0..LOOSE_RAFTERS {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let local = DVec3::new(
            side * design.w * 0.2,
            PLINTH + 0.6,
            breakup.rng.range(-design.d * 0.3, design.d * 0.3),
        );
        let at = design.to_world(local);
        let index = breakup.piece(
            simulation,
            FragmentShape::Beam,
            at.to_array(),
            [RAFTER_SECTION, RAFTER_SECTION, length],
            Some(RAFTER_COLOR),
            DebrisMaterial::Wood,
        );
        let body = simulation.fragments[index].body;
        let lean = side * 0.2;
        let swing = breakup.rng.range(-0.25, 0.25);
        let rotation = Quat4::from_euler_xyz(0.0, design.yaw() + PI / 2.0 + swing, lean);
        let spin = DVec3::new(
            breakup.rng.range(-3.0, 3.0),
            breakup.rng.range(-2.0, 2.0),
            breakup.rng.range(-3.0, 3.0),
        );
        let throw = design.to_world(DVec3::new(side * 1.2, -0.3, 0.0) + fall * BLAST_CARRY);
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_rotation(to_rotation(rotation), true);
        rigid_body.set_linvel(vector(throw.x, throw.y, throw.z), true);
        rigid_body.set_angvel(vector(spin.x, spin.y, spin.z), true);
        rigid_body.enable_ccd(true);
    }
    // Clapboards and laths torn off the walls, skidding out low round the footprint.
    for i in 0..LOOSE_BOARDS {
        let wall = i % 4;
        let (face, out, along, half) = frames[wall];
        // Just outside the siding, so a board never starts inside its wall.
        let local = face
            + out * BOARD_CLEARANCE
            + along * breakup.rng.range(-half * 0.7, half * 0.7)
            + DVec3::Y * breakup.rng.range(PLINTH + 0.4, roof.wall - 0.4);
        let at = design.to_world(local);
        let index = breakup.piece(
            simulation,
            FragmentShape::Beam,
            at.to_array(),
            BOARD_SIZE,
            Some(RAFTER_COLOR),
            DebrisMaterial::Wood,
        );
        let body = simulation.fragments[index].body;
        // Lying along the wall it was torn from.
        let yaw = design.yaw() + breakup.rng.range(-0.3, 0.3) + out.z.abs() * PI / 2.0;
        let rotation = Quat4::from_euler_xyz(breakup.rng.range(-0.3, 0.3), yaw, 0.0);
        let throw = design.to_world(
            out * breakup.rng.range(BOARD_THROW.0, BOARD_THROW.1)
                + along * breakup.rng.range(-1.0, 1.0)
                + DVec3::Y * breakup.rng.range(0.5, 2.0),
        );
        let spin = DVec3::new(
            breakup.rng.range(-5.0, 5.0),
            breakup.rng.range(-4.0, 4.0),
            breakup.rng.range(-5.0, 5.0),
        );
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_rotation(to_rotation(rotation), true);
        rigid_body.set_linvel(vector(throw.x, throw.y, throw.z), true);
        rigid_body.set_angvel(vector(spin.x, spin.y, spin.z), true);
        rigid_body.enable_ccd(true);
    }
    // The shards lie a while; the loose rafters and boards, which the heap also
    // shows, go sooner.
    for fragment in simulation
        .fragments
        .iter_mut()
        .filter(|fragment| breakup.pieces.contains(&fragment.id))
    {
        let life = if fragment.house_piece.is_some() {
            HOUSE_PIECE_LIFE
        } else {
            LOOSE_LIFE
        } + DEBRIS_CLEANUP_SECONDS;
        fragment.life = life;
        fragment.expires_at = Some(simulation.elapsed + life);
    }
}

/// Whether a wall shard is a lower one, standing under its wall's split.
fn lower_wall(piece: HousePiece) -> bool {
    piece.part.wall_index().is_some_and(|wall| wall != 1) && piece.shard.is_multiple_of(2)
}

/// Bring a collapsed cottage's pile to rest: a shard that has slowed once the
/// collapse has had time to come down is held by heavy damping until something
/// throws it about again.
pub fn settle_house_pieces(simulation: &mut Simulation) {
    let full = HOUSE_PIECE_LIFE + DEBRIS_CLEANUP_SECONDS;
    for fragment in &mut simulation.fragments {
        let Some(piece) = fragment.house_piece else {
            continue;
        };
        if piece.part == HousePart::Mound {
            continue;
        }
        let age = full - fragment.life;
        if piece.part.is_roof() || piece.part == HousePart::Chimney {
            // The roof and chimney come down on the wreck, not across the yard: a
            // slab sliding out past the footprint's edge is stopped there.
            let cover = fragment
                .house_id
                .and_then(|id| simulation.covers.iter().find(|cover| cover.id == id));
            if let Some(cover) = cover {
                let body = &mut simulation.world.bodies[fragment.body];
                contain(body, cover);
            }
        }
        if let Some(stub) = fragment.house_stub {
            let body = &mut simulation.world.bodies[fragment.body];
            if age >= STUB_RELEASE {
                // The pile has settled round it: it is debris like the rest.
                fragment.house_stub = None;
                body.set_body_type(RigidBodyType::Dynamic, false);
                body.set_linvel(Vector::ZERO, false);
                body.set_angvel(Vector::ZERO, false);
                // It keeps its lean: the pile resting on it cannot tip it out
                // flat, and a tank shoves it along the ground.
                body.lock_rotations(true, false);
                body.sleep();
            } else {
                let fall = age as f32;
                let drop = (0.5 * GRAVITY as f32 * fall * fall).min(PLINTH as f32);
                let lean = smoothstep(age, STUB_LEAN_AT, STUB_LEAN_AT + STUB_LEAN_TIME) as f32;
                let turn = Rotation::from_axis_angle(stub.axis, stub.lean * lean);
                let at = stub.pivot + turn * (stub.origin - stub.pivot) - Vector::Y * drop;
                body.set_next_kinematic_position(Pose::from_parts(at, turn * stub.rotation));
            }
            continue;
        }
        if piece.part.is_roof() && age >= ROOF_GIVE {
            let body = &mut simulation.world.bodies[fragment.body];
            if body.is_translation_locked() {
                body.lock_translations(false, true);
                let drop = if piece.shard.is_multiple_of(2) {
                    RIDGE_GIVE_DROP
                } else {
                    ROOF_GIVE_DROP
                };
                body.set_linvel(Vector::Y * -drop, true);
                // Its eaves now rest on whatever stands under them.
                for &collider in body.colliders() {
                    simulation.world.colliders[collider].set_enabled(true);
                }
            }
        }
        if piece.part.is_roof() && age >= ROOF_HOLD {
            let body = &mut simulation.world.bodies[fragment.body];
            if body.is_rotation_locked()[0] {
                body.lock_rotations(false, true);
                // Free to turn, the slab starts to lie back on the wreck, shingles
                // up, rather than resting on its eave like a lean-to.
                let normal = fragment.house_id.and_then(|id| {
                    simulation
                        .covers
                        .iter()
                        .find(|cover| cover.id == id)
                        .map(|cover| *body.rotation() * shard_normal(cover, piece.part))
                });
                if let Some(normal) = normal {
                    let axis = normal.cross(Vector::Y);
                    let twist = if !piece.shard.is_multiple_of(2) {
                        0.0
                    } else if (piece.shard / 2).is_multiple_of(2)
                        == (piece.part == HousePart::RightRoof)
                    {
                        RIDGE_SNAP_TWIST
                    } else {
                        -RIDGE_SNAP_TWIST
                    };
                    body.set_angvel(
                        body.angvel() + axis * ROOF_SETTLE_SPIN + normal * twist,
                        true,
                    );
                }
            }
        }
        if age < TOPPLE_WINDOW.0 {
            continue;
        }
        let body = &mut simulation.world.bodies[fragment.body];
        // A shard asleep on edge, propped on the pile, is still woken to fall.
        let sleeping = body.is_sleeping();
        let still = sleeping
            || body.linvel().length() < SETTLED_SPEED.0 && body.angvel().length() < SETTLED_SPEED.1;
        if still
            && age < TOPPLE_WINDOW.1
            && fragment.house_topples < TOPPLES
            && piece.part == HousePart::Chimney
        {
            // A stack that landed on its end is knocked over to lie in the wreck.
            let up = *body.rotation() * Vector::Y;
            if up.y > CHIMNEY_UPRIGHT {
                let side = *body.rotation() * Vector::X;
                let fall = Vector::new(side.x, 0.0, side.z).normalize();
                fragment.house_topples += 1;
                tip(body, Vector::Y.cross(fall), fall, 1.0);
                continue;
            }
        }
        // The lower wall shards stand on as the ruin's stubs, and an eave slab
        // propped on them stays as it leans: tipped, it would turn over the stub
        // onto its rafters.
        if still
            && age < TOPPLE_WINDOW.1
            && fragment.house_topples < TOPPLES
            && piece.part != HousePart::Chimney
            && !lower_wall(piece)
            && !(piece.part.is_roof() && piece.shard % 2 == 1)
        {
            let house = fragment.house_id.and_then(|id| {
                simulation
                    .covers
                    .iter()
                    .find(|cover| cover.id == id)
                    .map(|cover| (cover.x, cover.z, shard_normal(cover, piece.part)))
            });
            if let Some((x, z, local)) = house {
                // The shard's body keeps the build frame's axes turned by its pose.
                let normal = *body.rotation() * local;
                if normal.y.abs() < STANDING {
                    let p = body.translation();
                    let flat = Vector::new(normal.x, 0.0, normal.z).normalize();
                    let away = Vector::new(p.x - x as f32, 0.0, p.z - z as f32);
                    let outward = if flat.dot(away) >= 0.0 { flat } else { -flat };
                    let tries = fragment.house_topples;
                    let fall = if piece.part.is_roof() && tries < ROOF_TOPPLES {
                        // A roof slab falls back shingles up, the way that turns
                        // its top toward the sky, harder each try.
                        -flat
                    } else if tries >= INWARD_TOPPLES {
                        // Blocked by the pile: over onto the open ground outside,
                        // harder each try; past the last it is left as it rests.
                        outward
                    } else if tries > 0 {
                        // Onto the pile, harder each try.
                        -outward
                    } else {
                        // A wall falls the way it already leans; one balanced on
                        // edge falls in, onto the pile.
                        let lean = (*body.rotation() * Vector::Y).dot(flat);
                        if lean.abs() > LEANING {
                            flat * lean.signum()
                        } else {
                            -outward
                        }
                    };
                    fragment.house_topples += 1;
                    tip(
                        body,
                        Vector::Y.cross(fall),
                        fall,
                        1.0 + f32::from(tries) * 0.5,
                    );
                    continue;
                }
            }
        }
        if sleeping || age < SETTLE_AFTER {
            continue;
        }
        let slow = body.translation().y < LYING && still;
        let (linear, angular) = if slow {
            SETTLED_DAMPING
        } else {
            shard_damping(piece, age)
        };
        if body.linear_damping() != linear {
            body.set_linear_damping(linear);
            body.set_angular_damping(angular);
        }
    }
}

/// How hard a falling shard's motion and spin are damped `age` seconds into the
/// collapse: a roof slab's spin hard while it comes down as a roof.
fn shard_damping(piece: HousePiece, age: f64) -> (f32, f32) {
    if lower_wall(piece) {
        STUB_DAMPING
    } else if piece.part.is_roof() && age < ROOF_HOLD {
        ROOF_DAMPING
    } else {
        SHARD_DAMPING
    }
}

/// Stop a roof slab or the chimney sliding out past the footprint's edge (its
/// centre more than [`CONTAINED`] outside the cover box): its outward motion
/// there is taken away, as the rubble it ploughs into would.
fn contain(body: &mut rapier3d::prelude::RigidBody, cover: &Cover) {
    let p = body.translation();
    let mut v = body.linvel();
    let (dx, dz) = (p.x - cover.x as f32, p.z - cover.z as f32);
    let (hx, hz) = (
        (cover.w / 2.0 + CONTAINED) as f32,
        (cover.d / 2.0 + CONTAINED) as f32,
    );
    let mut stopped = false;
    if dx.abs() > hx && v.x * dx > 0.0 {
        v.x = 0.0;
        stopped = true;
    }
    if dz.abs() > hz && v.z * dz > 0.0 {
        v.z = 0.0;
        stopped = true;
    }
    if stopped {
        body.set_linvel(v, true);
    }
}

/// Tip a shard over about `axis`, shoving it `toward` with a little lift, with
/// the damping of a falling shard so the pile's heavy settling damping does not
/// hold it on edge.
fn tip(body: &mut rapier3d::prelude::RigidBody, axis: Vector, toward: Vector, effort: f32) {
    body.set_linear_damping(SHARD_DAMPING.0);
    body.set_angular_damping(SHARD_DAMPING.1);
    body.set_angvel(axis * TOPPLE_SPIN * effort, true);
    body.set_linvel(
        (toward * TOPPLE_SHOVE + Vector::Y * TOPPLE_LIFT) * effort,
        true,
    );
}

/// The outward normal of a cottage part's face in its build frame: a wall's
/// outside, a roof slope's top.
fn shard_normal(cover: &Cover, part: HousePart) -> Vector {
    let design = HouseDesign::of(cover.x, cover.z, cover.w, cover.d, cover.h);
    let pitch = design.roof().pitch;
    let n = match part.wall_index() {
        Some(index) => wall_frames(design.w, design.d)[index].1,
        None => match part {
            HousePart::RightRoof => DVec3::new(pitch, 1.0, 0.0),
            HousePart::LeftRoof => DVec3::new(-pitch, 1.0, 0.0),
            HousePart::FrontRoof => DVec3::new(0.0, 1.0, pitch),
            HousePart::BackRoof => DVec3::new(0.0, 1.0, -pitch),
            _ => DVec3::Y,
        },
    }
    .normalize();
    vector(n.x, n.y, n.z)
}
