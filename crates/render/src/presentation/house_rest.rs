//! A cottage's loose parts drawn merged once they come to rest.
//!
//! Each falling body of a cottage's parts is its own model, a draw per finish and a
//! shadow, which a collapse can afford for the seconds it moves but not for the many
//! seconds its pile lies. Bodies that have kept still a moment are baked at their
//! resting poses into merged chunks of a few bodies each, and their own views hidden;
//! one that moves again (a tank shoving it, a shell throwing it) breaks up its chunk,
//! whose other bodies are merged again a moment later, and one starting its cleanup
//! fade does too, so it sinks as it would alone. A pile is merged a chunk a frame, so
//! a slow device never pays for a whole cottage's pile in one frame. Presentation only:
//! the bodies are untouched.
//!
//! A body's model tree is kept only until its first merge: its own model and its chunk
//! hold what the GPU draws, and a pile lies for many seconds. A body merged again after
//! its chunk broke up, or one whose look is shared (and so was never its own), is drawn
//! again from its source.

use crate::fixed_hash::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::geometry::math::hex_to_linear;
use sloppy_core::models::{is_paintable, vertex_material};
use sloppy_core::scene::Node;

use crate::model::{MergedPart, MeshData};

/// Seconds a body keeps still before it may join a chunk, and how long a cottage's
/// pile must go without another body coming to rest before fewer than
/// [`CHUNK_PARTS`] of them are merged.
const STILL_SECONDS: f64 = 0.12;
const REBUILD_GAP: f64 = 0.4;
/// Bodies merged into one chunk: a cottage's pile takes a handful of chunks, a few
/// draws each, rather than a few draws per body, and a frame merges at most one.
const CHUNK_PARTS: usize = 10;
/// Movement (m, and quaternion difference) a still body may show between frames.
const STILL_DISTANCE: f32 = 0.006;
const STILL_TURN: f32 = 0.004;
/// How far (m, and quaternion difference) a merged body may creep from the pose it
/// was baked at before it leaves its chunk: one settling by millimetres stays drawn
/// as part of it.
const DRIFT_DISTANCE: f32 = 0.05;
const DRIFT_TURN: f32 = 0.0015;

type Pose = (Vec3, Quat);

struct Resting<S> {
    house: u32,
    /// What its body is drawn from ([`HousePiles::plan`]'s `draw`).
    source: S,
    /// Its body's model tree until it is first merged.
    node: Option<Node>,
    /// Its pose last frame, none before the first.
    pose: Option<Pose>,
    /// The chunk drawing it and the pose it was baked at there, while merged.
    baked: Option<(u64, Pose)>,
    still_since: f64,
    fading: bool,
}

struct Chunk<H> {
    members: Vec<u32>,
    /// None while its model is being added.
    handle: Option<H>,
}

/// One frame's merging: the chunks to free, and the chunk to draw now (hand its
/// handle back with [`HousePiles::built`]).
pub struct Plan<H> {
    pub freed: Vec<H>,
    pub build: Option<(u64, Vec<MergedPart>)>,
}

pub struct HousePiles<H, S> {
    bodies: HashMap<u32, Resting<S>>,
    chunks: HashMap<u64, Chunk<H>>,
    next_chunk: u64,
    /// Scratch for [`Self::plan`]: the bodies resting on their own, by house and id.
    resting: Vec<(u32, u32)>,
    /// Scratch: chunks breaking up this frame.
    broken: Vec<u64>,
}

impl<H, S> Default for HousePiles<H, S> {
    fn default() -> Self {
        Self {
            bodies: HashMap::default(),
            chunks: HashMap::default(),
            next_chunk: 0,
            resting: Vec::new(),
            broken: Vec::new(),
        }
    }
}

impl<H, S> HousePiles<H, S> {
    /// Follow the loose body of fragment `id` from cottage `house`, drawn from
    /// `source`; `node` is its model tree when it has one of its own already built.
    pub fn track(&mut self, id: u32, house: u32, source: S, node: Option<Node>, now: f64) {
        self.bodies.insert(
            id,
            Resting {
                house,
                source,
                node,
                pose: None,
                baked: None,
                still_since: now,
                fading: false,
            },
        );
    }

    pub fn tracks(&self, id: u32) -> bool {
        self.bodies.contains_key(&id)
    }

    /// A tracked body's pose this frame, and whether it has begun to fade.
    pub fn observe(&mut self, id: u32, pose: Pose, fading: bool, now: f64) {
        let Some(body) = self.bodies.get_mut(&id) else {
            return;
        };
        let moved = match body.baked {
            // Merged: it stays while it keeps near the pose it was baked at.
            Some((_, (position, rotation))) => {
                position.distance(pose.0) > DRIFT_DISTANCE
                    || rotation.dot(pose.1).abs() < 1.0 - DRIFT_TURN
            }
            None => body.pose.is_none_or(|(position, rotation)| {
                position.distance(pose.0) > STILL_DISTANCE
                    || rotation.dot(pose.1).abs() < 1.0 - STILL_TURN
            }),
        };
        if moved {
            body.still_since = now;
        }
        body.pose = Some(pose);
        body.fading = fading;
    }

    /// Whether the body is drawn in a chunk (so its own view hides).
    pub fn merged(&self, id: u32) -> bool {
        self.bodies
            .get(&id)
            .and_then(|body| body.baked)
            .and_then(|(chunk, _)| self.chunks.get(&chunk))
            .is_some_and(|chunk| chunk.handle.is_some())
    }

    /// Forget bodies whose fragments are gone; their chunks break up.
    pub fn retain(&mut self, live: &HashSet<u32>) -> Vec<H> {
        let before = self.bodies.len();
        self.bodies.retain(|id, _| live.contains(id));
        if self.bodies.len() == before {
            return Vec::new();
        }
        let mut broken = std::mem::take(&mut self.broken);
        broken.clear();
        broken.extend(
            self.chunks
                .iter()
                .filter(|(_, chunk)| chunk.members.iter().any(|id| !self.bodies.contains_key(id)))
                .map(|(&id, _)| id),
        );
        let freed = broken
            .drain(..)
            .filter_map(|chunk| self.break_up(chunk))
            .collect();
        self.broken = broken;
        freed
    }

    /// Free chunk `id`, returning its members to their own views.
    fn break_up(&mut self, id: u64) -> Option<H> {
        let chunk = self.chunks.remove(&id)?;
        for member in &chunk.members {
            if let Some(body) = self.bodies.get_mut(member) {
                body.baked = None;
            }
        }
        chunk.handle
    }

    /// This frame's merging: chunks a body moved out of or began to fade in break up,
    /// and at most one new chunk is merged from bodies resting on their own, once a
    /// pile has [`CHUNK_PARTS`] of them or has gone [`REBUILD_GAP`] without another
    /// coming to rest. A merged body no longer holding its tree is drawn by `draw`
    /// from its source.
    pub fn plan(&mut self, now: f64, mut draw: impl FnMut(&S) -> Node) -> Plan<H> {
        let mut broken = std::mem::take(&mut self.broken);
        broken.clear();
        broken.extend(
            self.chunks
                .iter()
                .filter(|(id, chunk)| {
                    chunk.members.iter().any(|member| {
                        self.bodies.get(member).is_none_or(|body| {
                            body.fading
                                || now - body.still_since < STILL_SECONDS
                                || body.baked.is_none_or(|(chunk, _)| chunk != **id)
                        })
                    })
                })
                .map(|(&id, _)| id),
        );
        let freed = broken
            .drain(..)
            .filter_map(|chunk| self.break_up(chunk))
            .collect();
        self.broken = broken;
        let ready = self.ready_members(now);
        let build = ready.map(|members| {
            let id = self.next_chunk;
            self.next_chunk += 1;
            let mut bodies = Vec::with_capacity(members.len());
            for member in &members {
                let body = self.bodies.get_mut(member).expect("resting body");
                let pose = body.pose.expect("a resting body has a pose");
                body.baked = Some((id, pose));
                let node = body.node.take().unwrap_or_else(|| draw(&body.source));
                bodies.push((node, pose));
            }
            let parts = merged(bodies.iter().map(|(node, pose)| (node, *pose)));
            self.chunks.insert(
                id,
                Chunk {
                    members,
                    handle: None,
                },
            );
            (id, parts)
        });
        Plan { freed, build }
    }

    /// The bodies of the lowest-numbered house whose pile is ready to merge (up to
    /// [`CHUNK_PARTS`] of them, lowest ids first), if any.
    fn ready_members(&mut self, now: f64) -> Option<Vec<u32>> {
        let mut resting = std::mem::take(&mut self.resting);
        resting.clear();
        resting.extend(self.bodies.iter().filter_map(|(&id, body)| {
            (!body.fading && body.baked.is_none() && now - body.still_since >= STILL_SECONDS)
                .then_some((body.house, id))
        }));
        resting.sort_unstable();
        let ready = resting
            .chunk_by(|a, b| a.0 == b.0)
            .find(|pile| {
                let latest = pile
                    .iter()
                    .map(|(_, id)| self.bodies[id].still_since + STILL_SECONDS)
                    .fold(f64::NEG_INFINITY, f64::max);
                pile.len() >= CHUNK_PARTS || (pile.len() >= 2 && now - latest >= REBUILD_GAP)
            })
            .map(|pile| pile.iter().take(CHUNK_PARTS).map(|&(_, id)| id).collect());
        self.resting = resting;
        ready
    }

    pub fn built(&mut self, chunk: u64, handle: H) {
        if let Some(chunk) = self.chunks.get_mut(&chunk) {
            chunk.handle = Some(handle);
        }
    }

    /// Every chunk's handle, to free on a reset.
    pub fn clear(&mut self) -> Vec<H> {
        self.bodies.clear();
        self.chunks
            .drain()
            .filter_map(|(_, chunk)| chunk.handle)
            .collect()
    }
}

/// Bodies' meshes baked at their poses straight into the upload layout, one part per
/// draw material (as batching them would): no scene graph or batched copy. A chunk
/// shades the ground as one caster: one shadow draw per finish grounds its bodies.
fn merged<'a>(bodies: impl Iterator<Item = (&'a Node, Pose)>) -> Vec<MergedPart> {
    let mut slots = Slots::default();
    for (node, (position, rotation)) in bodies {
        let pose = Mat4::from_rotation_translation(rotation, position);
        gather(node, pose, &mut slots);
    }
    slots.parts
}

/// The merged parts by draw material, and which part each source material's drawables
/// join: looking a source up by its address spares finding its vertex-coloured
/// material again for every drawable that wears it.
#[derive(Default)]
struct Slots {
    parts: Vec<MergedPart>,
    /// Draw material address to part.
    by_material: HashMap<usize, usize>,
    /// Source material address to part.
    by_source: HashMap<usize, usize>,
}

fn gather(node: &Node, parent: Mat4, slots: &mut Slots) {
    if !node.visible {
        return;
    }
    let world = parent * node.local_matrix().as_mat4();
    if let Some(drawable) = node.drawable.as_ref().filter(|d| d.instances.is_none()) {
        // Opaque standard paint goes into vertex colours under one shared material,
        // as `batch` paints it.
        let source = &drawable.material;
        let paint = is_paintable(source).then(|| {
            let [r, g, b] = hex_to_linear(source.color.0);
            [r as f32, g as f32, b as f32]
        });
        let Slots {
            parts,
            by_material,
            by_source,
        } = slots;
        let slot = *by_source
            .entry(Arc::as_ptr(source) as usize)
            .or_insert_with(|| {
                let material = if paint.is_some() {
                    vertex_material(source)
                } else {
                    source.clone()
                };
                *by_material
                    .entry(Arc::as_ptr(&material) as usize)
                    .or_insert_with(|| {
                        parts.push(MergedPart {
                            material,
                            data: MeshData::default(),
                            cast_shadow: true,
                        });
                        parts.len() - 1
                    })
            });
        parts[slot].data.append(&drawable.mesh, Some(&world), paint);
    }
    for child in &node.children {
        gather(child, world, slots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still<S>(piles: &mut HousePiles<u32, S>, ids: &[u32], from: f64, to: f64) {
        let mut now = from;
        while now <= to {
            for &id in ids {
                piles.observe(
                    id,
                    (Vec3::new(id as f32, 0.1, 0.0), Quat::IDENTITY),
                    false,
                    now,
                );
            }
            now += 0.05;
        }
    }

    fn draw(_: &()) -> Node {
        Node::group("")
    }

    #[test]
    fn a_piece_broken_out_of_a_merged_pile_frees_its_chunk_and_the_rest_merge_again() {
        let mut piles: HousePiles<u32, ()> = HousePiles::default();
        let ids = [1, 2, 3];
        for &id in &ids {
            piles.track(id, 7, (), Some(Node::group("")), 0.0);
        }
        still(&mut piles, &ids, 0.0, 1.0);
        let plan = piles.plan(1.0, draw);
        let (chunk, _) = plan.build.expect("the resting pile merges");
        piles.built(chunk, 40);
        assert!(ids.iter().all(|&id| piles.merged(id)));
        // A shell breaks body 2: its fragment is gone, replaced by two new pieces.
        let live: HashSet<u32> = [1, 3, 4, 5].into_iter().collect();
        assert_eq!(piles.retain(&live), vec![40], "the chunk is freed");
        assert!(
            !piles.merged(1) && !piles.merged(3),
            "its other bodies show again"
        );
        for id in [4, 5] {
            piles.track(id, 7, (), None, 1.1);
        }
        // Once all have come to rest they merge again.
        still(&mut piles, &[1, 3, 4, 5], 1.1, 2.0);
        let plan = piles.plan(2.0, draw);
        let (_, _) = plan.build.expect("merged again");
    }

    #[test]
    fn a_merged_body_thrown_by_a_blast_leaves_its_chunk() {
        let mut piles: HousePiles<u32, ()> = HousePiles::default();
        for id in [1, 2] {
            piles.track(id, 7, (), Some(Node::group("")), 0.0);
        }
        still(&mut piles, &[1, 2], 0.0, 1.0);
        let (chunk, _) = piles.plan(1.0, draw).build.expect("merged");
        piles.built(chunk, 9);
        piles.observe(1, (Vec3::new(1.0, 0.8, 0.3), Quat::IDENTITY), false, 1.05);
        let plan = piles.plan(1.05, draw);
        assert_eq!(plan.freed, vec![9]);
        assert!(!piles.merged(1));
    }

    #[test]
    fn a_body_keeps_its_tree_only_until_first_merged_and_is_drawn_again_after() {
        let mut piles: HousePiles<u32, u32> = HousePiles::default();
        for id in [1, 2] {
            piles.track(id, 7, id, Some(Node::group("")), 0.0);
        }
        still(&mut piles, &[1, 2], 0.0, 1.0);
        let mut drawn = Vec::new();
        let (chunk, _) = piles
            .plan(1.0, |&source| {
                drawn.push(source);
                Node::group("")
            })
            .build
            .expect("merged");
        assert!(drawn.is_empty(), "both bodies merge from their own trees");
        assert!(piles.bodies.values().all(|body| body.node.is_none()));
        piles.built(chunk, 9);
        // Body 1 is shoved, then both come to rest again.
        piles.observe(1, (Vec3::new(1.0, 0.8, 0.3), Quat::IDENTITY), false, 1.05);
        assert_eq!(piles.plan(1.05, |_| unreachable!()).freed, vec![9]);
        still(&mut piles, &[1, 2], 1.1, 2.0);
        piles
            .plan(2.0, |&source| {
                drawn.push(source);
                Node::group("")
            })
            .build
            .expect("merged again");
        drawn.sort_unstable();
        assert_eq!(drawn, vec![1, 2], "drawn again from their sources");
    }
}
