//! A cottage's structure in play: its standing parts as colliders on the cottage's
//! fixed body, shells and blasts breaking the parts they reach, and the parts the
//! frame no longer holds falling as rigid bodies of their own.
//!
//! A broken part is gone at once (a `Shatter` event throws its splinters); the parts
//! that lose their support with it come down together, one dynamic body for each group
//! still joined to each other ([`HouseStructure::clusters`]). A body that lands hard
//! breaks up into its parts while the debris budget has room, and every loose part
//! settles, lies a while and sinks away like other debris, so a ruin never piles up.
//! What still stands stays standing: posts, wall stubs and a chimney over the rubble.
//!
//! No random draw: every variation comes from the cottage's design and part indices.

use std::sync::Arc;

use glam::DVec3;
use rapier3d::prelude::{
    ActiveEvents, ColliderBuilder, ColliderHandle, RigidBodyBuilder, RigidBodyHandle,
};

use super::data::group;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{
    DebrisMaterial, blast_debris, debris_material, track_debris_contacts,
};
use super::house_layout::HouseDesign;
use super::house_structure::{CHIMNEY_PART, HouseStructure, PartKind, PartSet, StructurePart};
use super::math::{Point3, Vec2};
use super::navigation::Footprint;
use super::data::STEP;
use super::physics::{ContactForce, from_vector, interaction_groups, query_filter, vector};
use super::simulation::Simulation;
use super::types::{Cover, CoverKind, Fragment, SimEvent, SimEventType, Team};

/// A shell fused on striking a cottage flies on this far (m) before it bursts, so an
/// HE round through a wall goes off in the room behind it.
pub const FUSE_DISTANCE: f64 = 1.0;
/// An HE shell's burst inside a cottage: its reach (m) over the parts round it; the
/// damage falls off to nothing at the edge.
pub const HE_BLAST_RADIUS: f64 = 1.4;
/// Blasts from outside (rockets, mines, drums): the share of their reach over tanks
/// that they reach over a cottage's parts.
pub const STRUCTURE_BLAST_REACH: f64 = 0.5;
/// How high (m) a blast on the ground meets a cottage's walls.
pub const BLAST_HEIGHT: f64 = 1.0;
/// A part shielded from a blast by another standing part takes this share of it.
const SHIELDED: f64 = 0.35;
/// A blast shatters the parts within this far (m) of it; parts farther off that it
/// breaks come loose whole and fall, so splinters fly only where it went off.
const BLAST_CORE: f64 = 0.5;
/// A cottage counts as levelled (destroyed, for the score and the levels that rebuild
/// it) once its roof is down or less than this share of its parts stands.
const LEVELLED: f64 = 0.45;
/// A falling body's colliders sit this far (m) inside its parts' faces, clear of the
/// standing parts they were built against.
const SHRINK: f64 = 0.03;
/// Standing parts' colliders reach this far (m) past their faces into their
/// neighbours', so a shell or a sight line along the seam between two parts meets one.
const SEAM: f64 = 0.01;
/// The chimney's brick, and how big (m) a burst of its bricks reads as when it lands.
const CHIMNEY_BRICK: u32 = 0x9c4a32;
const CHIMNEY_BRICKS_SIZE: f64 = 2.5;
/// A fallen chimney's stack bursts on the first knock this hard (m/s change in a
/// tick): mortared brick does not land standing.
const CHIMNEY_LANDING: f64 = 1.0;
/// Mass per cubic metre of a loose part, as other wooden scenery pieces (the chimney's
/// brick heavier).
const TIMBER_DENSITY: f64 = 0.35;
const BRICK_DENSITY: f64 = 0.8;
/// A part giving way slumps out from the cottage's middle at this speed (m/s) and
/// tips at this rate (rad/s), so nothing hangs level in the air where it stood.
const SLUMP_SPEED: f64 = 0.6;
const SLUMP_TIP: f64 = 0.6;
/// A loose part resting (slower than this, m/s) with its middle this high (m) up on its
/// cottage's standing parts, this long (s) after it came loose, is wedged there.
const WEDGE_SPEED: f64 = 0.2;
const WEDGE_HEIGHT: f64 = 1.0;
const WEDGE_AFTER: f64 = 0.4;
/// How fast (m/s) a wedged part is shoved off what it lies on, and how long (s) it is
/// shoved before it drops through instead.
const WEDGE_PUSH: f64 = 1.2;
const WEDGE_GIVE_UP: f64 = 1.0;
/// The damage each unit of impulse a falling part strikes a standing one with does.
const CRUSH_DAMAGE: f64 = 6.0;
/// Contact force per unit of a loose part's mass that counts as striking what it lands
/// on rather than resting its weight there.
const CRUSH_FORCE_PER_MASS: f64 = 40.0;
/// How fast (m/s) a blast throws the parts it tears loose away from it.
const BLAST_THROW: f64 = 2.5;
/// Two sheets whose spans along the ridge overlap by this share of the first are one
/// column of rafters.
const COLUMN_OVERLAP: f64 = 0.5;
/// A falling body breaks up into its parts when its velocity changes by this much
/// (m/s) in one tick: a hard landing.
const SHATTER_SPEED: f64 = 2.5;
/// Seconds a loose part lies before it sinks away (plus the cleanup fade), and its
/// hard deadline.
const PART_LIFE: f64 = 30.0;
const PART_DEADLINE: f64 = 40.0;
/// A loose part's linear and angular damping while it falls, and after this long (s)
/// the damping toward rest that settles a pile rather than letting it creep.
const FALLING_DAMPING: (f32, f32) = (0.1, 0.3);
const SETTLE_AFTER: f64 = 2.0;
const SETTLED_DAMPING: (f32, f32) = (2.0, 3.0);

/// The linear and angular damping of a loose part `age` seconds after it came loose.
pub fn house_debris_damping(age: f64) -> (f32, f32) {
    if age > SETTLE_AFTER {
        SETTLED_DAMPING
    } else {
        FALLING_DAMPING
    }
}
/// The most splinter events one break sends: a blast breaking many parts throws them
/// from its heaviest few.
const SHATTER_EVENTS: usize = 6;

/// A cottage's structure as it stands: its layout, the parts broken or fallen from it,
/// the damage its standing parts have taken and their colliders.
#[derive(Clone, Debug)]
pub struct StructureState {
    pub layout: Arc<HouseStructure>,
    pub gone: PartSet,
    pub damage: Vec<f64>,
    pub colliders: Vec<Option<ColliderHandle>>,
    /// Counted as levelled ([`LEVELLED`]).
    pub levelled: bool,
}

impl StructureState {
    pub fn new(layout: Arc<HouseStructure>) -> Self {
        let count = layout.parts.len();
        Self {
            layout,
            gone: PartSet::EMPTY,
            damage: vec![0.0; count],
            colliders: vec![None; count],
            levelled: false,
        }
    }

    pub fn standing(&self, part: usize) -> bool {
        !self.gone.contains(part)
    }

    /// The standing parts that have taken damage.
    pub fn cracked(&self) -> PartSet {
        PartSet::from_indices(
            (0..self.layout.parts.len()).filter(|&index| self.standing(index) && self.damage[index] > 0.0),
        )
    }

    /// The share of the cottage's parts still standing.
    pub fn standing_share(&self) -> f64 {
        let count = self.layout.parts.len();
        (count - self.gone.len()) as f64 / count as f64
    }
}

/// The structure a cottage cover box stands for.
pub fn house_structure(x: f64, z: f64, w: f64, d: f64, h: f64) -> HouseStructure {
    HouseStructure::new(&HouseDesign::of(x, z, w, d, h))
}

/// A part's collider relative to its cottage's ground centre (world axes), shrunk by
/// `inset` metres, marked with the part's index.
pub fn part_collider(layout: &HouseStructure, index: usize, inset: f64) -> ColliderBuilder {
    let part = &layout.parts[index];
    let design = &layout.design;
    let builder = if part.cuboid {
        // A wall's lowest parts and the posts stand on their footing, which goes with
        // them: their colliders reach the ground.
        let footed = part.grounded && (part.wall.is_some() || part.kind == PartKind::Post);
        let bottom = if footed { part.min.with_y(0.0) } else { part.min };
        let (a, b) = (design.to_world(bottom), design.to_world(part.max));
        let (min, max) = (a.min(b), a.max(b));
        let half = ((max - min) / 2.0 - DVec3::splat(inset)).max(DVec3::splat(0.01));
        let center = (min + max) / 2.0;
        ColliderBuilder::cuboid(half.x as f32, half.y as f32, half.z as f32)
            .translation(vector(center.x, center.y, center.z))
    } else {
        let center = part.points.iter().copied().sum::<DVec3>() / part.points.len() as f64;
        let points: Vec<_> = part
            .points
            .iter()
            .map(|&p| {
                let p = p + (center - p).normalize_or_zero() * inset;
                let p = design.to_world(p);
                vector(p.x, p.y, p.z)
            })
            .collect();
        ColliderBuilder::convex_hull(&points).unwrap_or_else(|| {
            let half = ((part.max - part.min) / 2.0).max(DVec3::splat(0.02));
            let center = design.to_world(part.center());
            ColliderBuilder::cuboid(half.x as f32, half.y as f32, half.z as f32)
                .translation(vector(center.x, center.y, center.z))
        })
    };
    builder.user_data(index as u128)
}

/// The colliders of a body of a cottage's loose `parts`, relative to the cottage's ground
/// centre where it fell from: the host's, and a client's for its predicted hull.
pub fn house_debris_colliders(layout: &HouseStructure, parts: &PartSet) -> Vec<ColliderBuilder> {
    let surface = debris_material(DebrisMaterial::Wood);
    parts
        .iter()
        .map(|index| {
            let density = if layout.parts[index].kind == PartKind::Chimney {
                BRICK_DENSITY
            } else {
                TIMBER_DENSITY
            };
            part_collider(layout, index, SHRINK)
                .collision_groups(interaction_groups(group::HOUSE_SHARD))
                .density(density as f32)
                .friction(surface.friction as f32)
                .restitution(surface.restitution as f32)
        })
        .collect()
}

/// The colliders of a cottage's standing parts (those not in `gone`), in part order,
/// on a fixed body at its ground centre.
pub fn standing_colliders(layout: &HouseStructure, gone: &PartSet) -> Vec<ColliderBuilder> {
    (0..layout.parts.len())
        .filter(|&index| !gone.contains(index))
        .map(|index| {
            part_collider(layout, index, -SEAM)
                .collision_groups(interaction_groups(group::COVER))
                .friction(0.4)
        })
        .collect()
}

/// The world boxes of a cottage's standing parts that block tanks on the ground, or
/// its whole box while none of them is gone (a sealed cottage's rooms are no way in).
pub fn ground_footprints(cover: &Cover, out: &mut Vec<Footprint>) {
    let Some(state) = &cover.structure else {
        return;
    };
    let layout = &state.layout;
    let open = (0..layout.parts.len())
        .any(|index| state.gone.contains(index) && layout.parts[index].blocks_ground());
    if !open {
        if state.gone.len() < layout.parts.len() {
            out.push(Footprint::from(cover));
        }
        return;
    }
    for (index, part) in layout.parts.iter().enumerate() {
        if state.gone.contains(index) || !part.blocks_ground() {
            continue;
        }
        let (a, b) = (
            layout.design.to_world(part.min),
            layout.design.to_world(part.max),
        );
        let (min, max) = (a.min(b), a.max(b));
        out.push(Footprint {
            x: cover.x + (min.x + max.x) / 2.0,
            z: cover.z + (min.z + max.z) / 2.0,
            w: max.x - min.x,
            d: max.z - min.z,
        });
    }
}

/// The paint a part's splinters and dust take: the roof's for the roof, the walls'
/// otherwise.
fn part_paint(layout: &HouseStructure, index: usize) -> u32 {
    let style = layout.design.style;
    match layout.parts[index].kind {
        PartKind::Sheet | PartKind::Ridge => style.roof,
        PartKind::Chimney => CHIMNEY_BRICK,
        PartKind::Floor | PartKind::Partition => 0xc9b48f,
        _ => style.siding,
    }
}

impl Simulation {
    /// The part of a cottage behind `collider` (from its user data), if it is one.
    pub fn house_part(&self, collider: ColliderHandle) -> Option<(usize, usize)> {
        let &cover = self.cover_by_collider.get(&collider)?;
        self.covers[cover].structure.as_ref()?;
        let part = self.world.colliders.get(collider)?.user_data as usize;
        Some((cover, part))
    }

    /// Hit cottage `cover`'s `part` for `amount` at `point`, from `along` (the way the
    /// shell flew); `force` breaks it whatever its strength (an armour-piercing round
    /// through it). Returns whether the part broke.
    #[allow(clippy::too_many_arguments)]
    pub fn strike_house(
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
        if !state.standing(part) {
            return false;
        }
        state.damage[part] += amount;
        if !force && state.damage[part] < state.layout.parts[part].strength {
            return false;
        }
        let from = Point3::new(point.x - along.x, point.y, point.z - along.z);
        self.break_house_parts(cover, PartSet::from_indices([part]), Some(from), by);
        true
    }

    /// A burst at `point` damaging every cottage part within `radius`, falling off to
    /// nothing at its edge and shielded by standing parts between, and throwing the
    /// loose debris round it.
    pub fn house_blast(&mut self, point: Point3, radius: f64, damage: f64, by: Option<(u32, Team)>) {
        blast_debris(self, point.planar(), radius, damage);
        for cover in 0..self.covers.len() {
            self.house_blast_on(cover, point, radius, damage, by);
        }
    }

    /// [`Self::house_blast`] on one cottage, without the debris.
    pub fn house_blast_on(
        &mut self,
        cover: usize,
        point: Point3,
        radius: f64,
        damage: f64,
        by: Option<(u32, Team)>,
    ) {
        let c = &self.covers[cover];
        let Some(state) = &c.structure else {
            return;
        };
        // The cottage's box in reach at all.
        let reach = Vec2::new(
            ((point.x - c.x).abs() - c.w / 2.0).max(0.0),
            ((point.z - c.z).abs() - c.d / 2.0).max(0.0),
        );
        if reach.x.hypot(reach.z) >= radius {
            return;
        }
        let layout = state.layout.clone();
        let local = layout.design.to_local(DVec3::new(point.x - c.x, point.y, point.z - c.z));
        let mut broken = PartSet::EMPTY;
        let mut loose = PartSet::EMPTY;
        let mut hits: Vec<(usize, f64, f64)> = Vec::new();
        for (index, part) in layout.parts.iter().enumerate() {
            if !state.standing(index) {
                continue;
            }
            // How far the burst is from the part itself: a sloping sheet's box reaches
            // far below its eave, so a burst by a gable's window met the roof over it.
            let gap = match state.colliders[index].and_then(|handle| self.world.colliders.get(handle)) {
                Some(collider) if part.distance(local) < radius => f64::from(
                    collider
                        .shape()
                        .distance_to_point(collider.position(), vector(point.x, point.y, point.z), true),
                ),
                _ => part.distance(local),
            };
            if gap >= radius {
                continue;
            }
            let mut share = 1.0 - gap / radius;
            if gap > 0.05 && self.shielded(cover, index, point, local, gap) {
                share *= SHIELDED;
            }
            hits.push((index, damage * share, gap));
        }
        let state = self.covers[cover]
            .structure
            .as_mut()
            .expect("checked above");
        for (index, amount, gap) in hits {
            state.damage[index] += amount;
            if state.damage[index] >= layout.parts[index].strength {
                if gap < BLAST_CORE {
                    broken.insert(index);
                } else {
                    loose.insert(index);
                }
            }
        }
        if !broken.is_empty() || !loose.is_empty() {
            self.break_house_parts_loosening(cover, broken, loose, Some(point), by);
        }
    }

    /// Stand cottage `cover` as a scene shows it, with exactly the parts not in `gone`
    /// (a model of a room refilled from what a client sees): their colliders come and
    /// go, no part falls and nothing is scored.
    pub fn set_house_gone(&mut self, cover: usize, gone: PartSet) {
        let body = self.covers[cover].body;
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        if state.gone == gone || !self.world.bodies.contains(body) {
            return;
        }
        let layout = state.layout.clone();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for index in 0..layout.parts.len() {
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
        for handle in removed {
            self.cover_by_collider.remove(&handle);
            self.world.remove_collider(handle);
        }
        for index in added {
            let collider = part_collider(&layout, index, -SEAM)
                .collision_groups(interaction_groups(group::COVER))
                .friction(0.4);
            let handle = self.world.insert_collider(collider, Some(body));
            self.cover_by_collider.insert(handle, cover);
            if let Some(state) = &mut self.covers[cover].structure {
                state.colliders[index] = Some(handle);
            }
        }
        let region = Footprint::from(&self.covers[cover]);
        self.nav.rebuild(&self.covers, Some(region));
    }

    /// Every standing part of cottage `cover` takes `amount`. The walls on the ground
    /// that it breaks shatter; what it breaks above them comes loose whole and falls.
    pub fn damage_every_house_part(&mut self, cover: usize, amount: f64, by: Option<(u32, Team)>) {
        let Some(state) = &mut self.covers[cover].structure else {
            return;
        };
        let (mut broken, mut loose) = (PartSet::EMPTY, PartSet::EMPTY);
        for index in 0..state.layout.parts.len() {
            if state.standing(index) {
                state.damage[index] += amount;
                let part = &state.layout.parts[index];
                if state.damage[index] >= part.strength {
                    if part.grounded {
                        broken.insert(index);
                    } else {
                        loose.insert(index);
                    }
                }
            }
        }
        if !broken.is_empty() || !loose.is_empty() {
            self.break_house_parts_loosening(cover, broken, loose, None, by);
        }
    }

    /// Whether another standing part lies between a blast at `point` (`local` in the
    /// cottage's build frame) and the nearest point of `part`, `gap` away.
    fn shielded(&self, cover: usize, part: usize, point: Point3, local: DVec3, gap: f64) -> bool {
        let c = &self.covers[cover];
        let Some(state) = &c.structure else {
            return false;
        };
        let target = state.layout.parts[part].min.max(local.min(state.layout.parts[part].max));
        let target = state.layout.design.to_world(target);
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
            .is_some_and(|(collider, _)| {
                self.house_part(collider) != Some((cover, part))
            })
    }

    /// Break `broken` out of cottage `cover` (each throws its splinters away from
    /// `from`), then let fall whatever the frame no longer holds up.
    pub fn break_house_parts(
        &mut self,
        cover: usize,
        broken: PartSet,
        from: Option<Point3>,
        by: Option<(u32, Team)>,
    ) {
        self.break_house_parts_loosening(cover, broken, PartSet::EMPTY, from, by);
    }

    /// [`Self::break_house_parts`], with the parts in `loose` torn loose whole: they
    /// fall with what they took down rather than shattering.
    pub fn break_house_parts_loosening(
        &mut self,
        cover: usize,
        broken: PartSet,
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
        let broken = PartSet::from_indices(broken.iter().filter(|&index| state.standing(index)));
        let loose = PartSet::from_indices(
            loose
                .iter()
                .filter(|&index| state.standing(index) && !broken.contains(index)),
        );
        if broken.is_empty() && loose.is_empty() {
            return;
        }
        // Splinters from the heaviest broken parts.
        let mut thrown: Vec<usize> = broken.iter().collect();
        thrown.sort_by(|&a, &b| {
            let volume = |i: usize| (layout.parts[i].max - layout.parts[i].min).element_product();
            volume(b).total_cmp(&volume(a)).then(a.cmp(&b))
        });
        for &index in thrown.iter().take(SHATTER_EVENTS) {
            let part = &layout.parts[index];
            let center = layout.design.to_world(part.center());
            let size = (part.max - part.min).max_element();
            let mut event = SimEvent::at(SimEventType::Shatter, x + center.x, z + center.z);
            event.height = Some(center.y);
            event.size = Some(size);
            event.color = Some(part_paint(&layout, index));
            event.cover_kind = Some(CoverKind::House);
            event.id = Some(self.covers[cover].id);
            event.from = from;
            self.events.push(event);
        }
        self.remove_house_parts(cover, &broken);
        // What the frame no longer holds comes down, in groups still joined.
        let gone = self.covers[cover]
            .structure
            .as_ref()
            .map_or(PartSet::EMPTY, |state| state.gone);
        let falling = layout.unsupported(&gone.union(&loose)).union(&loose);
        if !falling.is_empty() {
            self.remove_house_parts(cover, &falling);
            for cluster in layout.clusters(&falling) {
                // A blast throws what it tore loose away from it.
                let thrown = (cluster.iter().any(|index| loose.contains(index)))
                    .then_some(from)
                    .flatten();
                self.drop_house_cluster(cover, cluster, thrown);
            }
        }
        self.after_house_break(cover, &layout, broken.union(&falling), by);
    }

    /// Take `parts` out of the cottage's standing structure and the world.
    fn remove_house_parts(&mut self, cover: usize, parts: &PartSet) {
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

    /// The health, navigation and score after parts left cottage `cover`.
    fn after_house_break(
        &mut self,
        cover: usize,
        layout: &HouseStructure,
        left: PartSet,
        by: Option<(u32, Team)>,
    ) {
        let c = &mut self.covers[cover];
        let Some(state) = &mut c.structure else {
            return;
        };
        c.hp = c.max_hp * state.standing_share();
        let roof_down = (0..layout.parts.len())
            .filter(|&index| layout.parts[index].kind == PartKind::Sheet)
            .all(|index| state.gone.contains(index));
        let levelled = !state.levelled && (roof_down || state.standing_share() < LEVELLED);
        if levelled {
            state.levelled = true;
            c.alive = false;
            let (x, z, h, id, color) = (c.x, c.z, c.h, c.id, c.color);
            self.destroyed += 1;
            if let Some((owner, team)) = by
                && self
                    .tanks
                    .iter()
                    .any(|tank| self.records(tank) && tank.id == owner && tank.team == team)
            {
                self.combat_record.cover_destroyed += 1;
            }
            let mut destroy = SimEvent::at(SimEventType::Destroy, x, z);
            destroy.cover_kind = Some(CoverKind::House);
            destroy.height = Some(h);
            destroy.id = Some(id);
            destroy.size = Some(7.0);
            destroy.color = Some(color);
            self.events.push(destroy);
        }
        if left
            .iter()
            .any(|index| layout.parts[index].blocks_ground())
        {
            let region = Footprint::from(&self.covers[cover]);
            self.nav.rebuild(&self.covers, Some(region));
        }
    }

    /// `cluster` of cottage `cover`'s parts coming down as one body, slumping out from
    /// the cottage's middle.
    fn drop_house_cluster(&mut self, cover: usize, cluster: PartSet, blast: Option<Point3>) {
        let c = &self.covers[cover];
        let Some(state) = &c.structure else {
            return;
        };
        let layout = state.layout.clone();
        let (x, z, id) = (c.x, c.z, c.id);
        let body = self.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, 0.0, z))
                .linear_damping(FALLING_DAMPING.0)
                .angular_damping(FALLING_DAMPING.1)
                .can_sleep(true),
        );
        let colliders = self.insert_house_debris(&layout, &cluster, body);
        // Out from the cottage's middle and tipping over that way.
        let center = cluster
            .iter()
            .map(|index| layout.design.to_world(layout.parts[index].center()))
            .sum::<DVec3>()
            / cluster.len() as f64;
        let out = DVec3::new(center.x, 0.0, center.z).normalize_or_zero();
        // A wall tips out over its foot as it goes; a roof section or floor, tipped
        // the same way about its middle, would swing its high edge up like a lid, so
        // it is left to fall.
        let wall = cluster.iter().any(|index| {
            matches!(
                layout.parts[index].kind,
                PartKind::Panel
                    | PartKind::Window
                    | PartKind::Door
                    | PartKind::Gable
                    | PartKind::Partition
                    | PartKind::Post
            )
        });
        let mut velocity = DVec3::new(out.x * SLUMP_SPEED, -SLUMP_SPEED, out.z * SLUMP_SPEED);
        if let Some(blast) = blast {
            let away = DVec3::new(x + center.x - blast.x, 0.0, z + center.z - blast.z);
            velocity += away.normalize_or_zero() * BLAST_THROW;
        }
        let rigid_body = &mut self.world.bodies[body];
        rigid_body.set_linvel(vector(velocity.x, velocity.y, velocity.z), true);
        if wall {
            let tip = DVec3::Y.cross(out) * SLUMP_TIP;
            rigid_body.set_angvel(vector(tip.x, tip.y, tip.z), true);
        }
        self.add_house_fragment(cover, id, layout.design.style.siding, body, colliders, cluster);
    }

    /// A falling body's colliders, one for each of `parts`.
    fn insert_house_debris(
        &mut self,
        layout: &HouseStructure,
        parts: &PartSet,
        body: RigidBodyHandle,
    ) -> Vec<ColliderHandle> {
        let colliders: Vec<ColliderHandle> = house_debris_colliders(layout, parts)
            .into_iter()
            .map(|collider| self.world.insert_collider(collider, Some(body)))
            .collect();
        // Every part reports what it lands on hard ([`crush_house_parts`]).
        let threshold = (f64::from(self.world.bodies[body].mass()) * CRUSH_FORCE_PER_MASS) as f32;
        for &handle in &colliders {
            let collider = &mut self.world.colliders[handle];
            collider.set_active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
            collider.set_contact_force_event_threshold(threshold);
        }
        colliders
    }

    fn add_house_fragment(
        &mut self,
        cover: usize,
        id: u32,
        color: u32,
        body: RigidBodyHandle,
        colliders: Vec<ColliderHandle>,
        parts: PartSet,
    ) {
        self.reserve_fragments(1);
        let fragment_id = self.allocate_id();
        if let Some(&first) = colliders.first() {
            track_debris_contacts(self, body, first, fragment_id, DebrisMaterial::Wood);
        }
        let _ = cover;
        let mut fragment = Fragment::new(
            fragment_id,
            body,
            PART_LIFE + DEBRIS_CLEANUP_SECONDS,
            1.0,
            color,
        );
        fragment.material = Some(DebrisMaterial::Wood);
        fragment.source_kind = Some(CoverKind::House);
        fragment.house_id = Some(id);
        fragment.house_parts = Some(parts);
        fragment.created_at = Some(self.elapsed);
        fragment.expires_at = Some(self.elapsed + PART_DEADLINE);
        fragment.house_velocity = Some(from_vector(self.world.bodies[body].linvel()));
        self.fragments.push(fragment);
    }

    /// A fallen chimney landing: its stack bursts into bricks where it struck.
    fn chimney_to_bricks(&mut self, index: usize) {
        let fragment = self.fragments.remove(index);
        let body = &self.world.bodies[fragment.body];
        let at = from_vector(body.center_of_mass());
        let mut event = SimEvent::at(SimEventType::Shatter, at.x, at.z);
        event.height = Some(at.y);
        event.size = Some(CHIMNEY_BRICKS_SIZE);
        event.color = Some(CHIMNEY_BRICK);
        event.cover_kind = Some(CoverKind::House);
        event.id = fragment.house_id;
        self.events.push(event);
        self.remove_body(fragment.body);
    }

    /// Break a landed body up into its parts, each a body of its own moving as it was.
    fn shatter_house_fragment(&mut self, index: usize) {
        let fragment = self.fragments.remove(index);
        let (Some(parts), Some(house)) = (fragment.house_parts, fragment.house_id) else {
            self.fragments.insert(index, fragment);
            return;
        };
        let Some(layout) = self
            .covers
            .iter()
            .find(|cover| cover.id == house)
            .and_then(|cover| cover.structure.as_ref())
            .map(|state| state.layout.clone())
        else {
            self.fragments.insert(index, fragment);
            return;
        };
        let old = &self.world.bodies[fragment.body];
        let (position, rotation) = (old.translation(), *old.rotation());
        let (linvel, angvel) = (old.linvel(), old.angvel());
        let center = old.center_of_mass();
        self.remove_body(fragment.body);
        for section in landing_sections(&layout, &parts) {
            let body = self.world.insert_body(
                RigidBodyBuilder::dynamic()
                    .translation(position)
                    .rotation(rotation.to_scaled_axis())
                    .linear_damping(FALLING_DAMPING.0)
                    .angular_damping(FALLING_DAMPING.1)
                    .can_sleep(true),
            );
            let colliders = self.insert_house_debris(&layout, &section, body);
            let rigid_body = &mut self.world.bodies[body];
            let r = rigid_body.center_of_mass() - center;
            rigid_body.set_linvel(linvel + angvel.cross(r), true);
            rigid_body.set_angvel(angvel, true);
            self.reserve_fragments(1);
            let fragment_id = self.allocate_id();
            if let Some(&first) = colliders.first() {
                track_debris_contacts(self, body, first, fragment_id, DebrisMaterial::Wood);
            }
            let mut piece = fragment.clone();
            piece.id = fragment_id;
            piece.body = body;
            piece.house_parts = Some(section);
            piece.house_wedged = None;
            piece.house_velocity = Some(from_vector(self.world.bodies[body].linvel()));
            self.fragments.push(piece);
        }
    }
}

/// The sections a landing body breaks into: a roof slope's sheets column by column
/// (its rafters split along their length: the sheet under the ridge with the one
/// below it), every other part on its own, the chimney among them.
fn landing_sections(layout: &HouseStructure, parts: &PartSet) -> Vec<PartSet> {
    let mut sections: Vec<PartSet> = Vec::new();
    for index in parts.iter() {
        let part = &layout.parts[index];
        let column = |other: &StructurePart| {
            part.kind == PartKind::Sheet
                && other.kind == PartKind::Sheet
                && other.slope.map(|(slope, _)| slope) == part.slope.map(|(slope, _)| slope)
                && part.max.z.min(other.max.z) - part.min.z.max(other.min.z)
                    > COLUMN_OVERLAP * (part.max.z - part.min.z)
        };
        match sections
            .iter_mut()
            .find(|section| section.iter().any(|other| column(&layout.parts[other])))
        {
            Some(section) => section.insert(index),
            None => sections.push(PartSet::from_indices([index])),
        }
    }
    sections
}

/// Each tick: loose cottage parts that land hard break up, and those lying a while
/// are damped to rest.
pub fn settle_house_debris(simulation: &mut Simulation) {
    unwedge_house_debris(simulation);
    let mut index = 0;
    while index < simulation.fragments.len() {
        let fragment = &simulation.fragments[index];
        let (Some(parts), Some(before)) = (fragment.house_parts, fragment.house_velocity) else {
            index += 1;
            continue;
        };
        let body = &simulation.world.bodies[fragment.body];
        let now = from_vector(body.linvel());
        let change = (now.x - before.x)
            .hypot(now.y - before.y)
            .hypot(now.z - before.z);
        let age = simulation.elapsed - fragment.created_at.unwrap_or(simulation.elapsed);
        let room = simulation.fragments.len() + parts.len() <= simulation.max_fragments;
        // The chimney alone lands as bricks.
        if parts == PartSet::from_indices([CHIMNEY_PART])
            && change > CHIMNEY_LANDING
            && fragment.life > DEBRIS_CLEANUP_SECONDS
        {
            simulation.chimney_to_bricks(index);
            continue;
        }
        if parts.len() > 1 && change > SHATTER_SPEED && room && fragment.life > DEBRIS_CLEANUP_SECONDS {
            simulation.shatter_house_fragment(index);
            continue;
        }
        let body_handle = fragment.body;
        simulation.fragments[index].house_velocity = Some(now);
        // A body being shoved off what it lies on keeps falling's damping.
        let (linear, angular) = if simulation.fragments[index].house_wedged.is_some() {
            FALLING_DAMPING
        } else {
            house_debris_damping(age)
        };
        let body = &mut simulation.world.bodies[body_handle];
        if body.linear_damping() != linear {
            body.set_linear_damping(linear);
            body.set_angular_damping(angular);
        }
        index += 1;
    }
}

/// Loose cottage parts landing hard on standing ones break them: a standing part takes
/// [`CRUSH_DAMAGE`] for every unit of impulse falling parts struck it with this tick,
/// from the contact forces past their weight that the step reported (a roof slab
/// coming down on a wall stub, not one resting on it). Parts are taken in cottage, then
/// part order: no random draw.
pub fn crush_house_parts(simulation: &mut Simulation, contacts: &[ContactForce]) {
    let mut struck: Vec<((usize, usize), f64, Point3)> = Vec::new();
    for contact in contacts {
        for (falling, standing) in [
            (contact.collider1, contact.collider2),
            (contact.collider2, contact.collider1),
        ] {
            let Some(part) = simulation.house_part(standing) else {
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
            if !simulation
                .fragments
                .iter()
                .any(|fragment| fragment.body == body && fragment.house_parts.is_some())
            {
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
        let Some(center) = part_center(&simulation.covers[cover], part) else {
            continue;
        };
        let along = Vec2::new(center.x - from.x, center.z - from.z);
        simulation.strike_house(cover, part, impulse * CRUSH_DAMAGE, false, center, along, None);
    }
}

/// A loose part come to rest up on what still stands of its cottage (a roof slab
/// lying across two gables like a lid, a floor strip on wall stubs) slides off it: it
/// is shoved out from the cottage's middle each tick it lies there, and if it is still
/// there after [`WEDGE_GIVE_UP`] it drops through the remains. What it lay on stands:
/// nothing breaks that nothing struck. No random draw.
fn unwedge_house_debris(simulation: &mut Simulation) {
    let elapsed = simulation.elapsed;
    for index in 0..simulation.fragments.len() {
        let fragment = &simulation.fragments[index];
        let (Some(_), Some(created), Some(house)) =
            (fragment.house_parts, fragment.created_at, fragment.house_id)
        else {
            continue;
        };
        if elapsed - created < WEDGE_AFTER || fragment.life <= DEBRIS_CLEANUP_SECONDS {
            continue;
        }
        let body = &simulation.world.bodies[fragment.body];
        let resting_up = f64::from(body.center_of_mass().y) >= WEDGE_HEIGHT
            && f64::from(body.linvel().length()) <= WEDGE_SPEED;
        let on_remains = resting_up
            && body.colliders().iter().any(|&collider| {
                simulation.world.contact_pairs_with(collider).any(|pair| {
                    let other = if pair.collider1 == collider {
                        pair.collider2
                    } else {
                        pair.collider1
                    };
                    pair.has_any_active_contact() && simulation.house_part(other).is_some()
                })
            });
        if !on_remains {
            if fragment.house_wedged.is_some() && !resting_up {
                simulation.fragments[index].house_wedged = None;
            }
            continue;
        }
        let Some(cover) = simulation.covers.iter().find(|cover| cover.id == house) else {
            continue;
        };
        let at = from_vector(body.center_of_mass());
        let out = Vec2::new(at.x - cover.x, at.z - cover.z);
        let length = out.x.hypot(out.z).max(1e-6);
        let since = *simulation.fragments[index].house_wedged.get_or_insert(elapsed);
        let handle = simulation.fragments[index].body;
        if elapsed - since > WEDGE_GIVE_UP {
            // Still up there: it comes down through what holds it.
            let colliders: Vec<ColliderHandle> =
                simulation.world.bodies[handle].colliders().to_vec();
            for collider in colliders {
                simulation.world.colliders[collider]
                    .set_collision_groups(interaction_groups(group::HOUSE_SHARD_THROUGH));
            }
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

/// Whether `fragment` is a loose piece of a cottage's structure.
pub fn house_debris(fragment: &Fragment) -> bool {
    fragment.house_parts.is_some()
}

/// The fragment whose body holds `collider`, for a cottage's loose parts any of its
/// colliders, for other debris its first.
pub fn fragment_with_collider(simulation: &Simulation, collider: ColliderHandle) -> Option<usize> {
    let body = simulation.world.colliders.get(collider)?.parent()?;
    simulation.fragments.iter().position(|fragment| {
        fragment.body == body
            && (fragment.house_parts.is_some()
                || simulation.world.bodies[body].colliders().first() == Some(&collider))
    })
}

/// The world position of cottage `cover`'s part `index`'s middle.
pub fn part_center(cover: &Cover, index: usize) -> Option<Point3> {
    let state = cover.structure.as_ref()?;
    let center = state.layout.design.to_world(state.layout.parts.get(index)?.center());
    Some(Point3::new(cover.x + center.x, center.y, cover.z + center.z))
}

/// Cottage `cover`'s part `index` as a world box (min, max).
pub fn part_box(cover: &Cover, index: usize) -> Option<(Point3, Point3)> {
    let state = cover.structure.as_ref()?;
    let part = state.layout.parts.get(index)?;
    let (a, b) = (
        state.layout.design.to_world(part.min),
        state.layout.design.to_world(part.max),
    );
    let (min, max) = (a.min(b), a.max(b));
    Some((
        Point3::new(cover.x + min.x, min.y, cover.z + min.z),
        Point3::new(cover.x + max.x, max.y, cover.z + max.z),
    ))
}
