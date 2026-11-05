//! A collapsed cottage's shards drawn merged once they come to rest.
//!
//! Each falling shard is its own model (a draw and, for the roof, a shadow), which a
//! collapse can afford for the seconds it moves but not for the many seconds its pile
//! lies on the heap. Shards that have kept still a moment are baked at their resting
//! poses into merged chunks of a few shards each, and their own views hidden; a shard
//! that moves again (a tank shoving it) breaks up its chunk, whose other shards are
//! merged again a moment later, and one starting its cleanup fade does too, so it
//! sinks as it would alone. A pile is merged a chunk a frame: merging and uploading a
//! whole cottage's shards (a quarter of a million vertices) at once, and again each
//! time the pile changed, cost a slow device a 60 ms frame every 0.4 s while it
//! settled. Presentation only: the shards' bodies are untouched.
//!
//! When the pile's shards begin to sink away, the walls' and the mound's poses they
//! last rested at stay behind pressed flat ([`SettledShards::remains`]): splintered
//! panels, slabs and boards strewn low over the floor for the rest of the round, so
//! the collapse never leaves a swept foundation. Of the roof's slabs only a few
//! crumpled scraps of sheet stay, torn from where each lay ([`Lasting::Scraps`]), so
//! the remains keep the cottage's roofing among their boards. They are drawn without
//! a shadow, lie too low for a hull to clip them visibly, and are pressed a chunk a
//! frame too.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::geometry::math::hex_to_linear;
use sloppy_core::models::{is_paintable, vertex_material};
use sloppy_core::scene::Node;

use crate::model::{MergedPart, MeshData, Vertex};

/// How much of its height a pile keeps once pressed into its lasting remains, and
/// the least height a pressed piece stands above the ground (m).
const REMAINS_SQUASH: f32 = 0.32;

const REMAINS_LIFT: f32 = 0.015;

/// What of a shard stays in a pile's lasting remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lasting {
    /// Nothing: the chimney's stack, whose bricks the heap keeps of its own.
    No,
    /// All of it, pressed flat: a wall's shard or the mound.
    Whole,
    /// A few torn scraps of it, crumpled: a roof slab, which whole and pressed
    /// flat lay over the floor like a green floor.
    Scraps,
}

/// A roof slab's scraps in the remains: how many, their size (m, in the slab's own
/// frame), how far from its middle they may lie, and how high (m, before the
/// remains are pressed) their crumples rise and fall, over what length (m).
const SCRAPS: usize = 3;
const SCRAP_SIZE: (f32, f32) = (0.45, 1.0);
const SCRAP_SPREAD: f32 = 1.6;
const CRUMPLE: f32 = 0.09;
const CRUMPLE_LENGTH: f32 = 0.37;

/// Seconds a shard keeps still before it may join a chunk, and how long a cottage's
/// pile must go without another shard coming to rest before fewer than
/// [`CHUNK_SHARDS`] of them are merged.
const STILL_SECONDS: f64 = 0.12;
const REBUILD_GAP: f64 = 0.4;
/// Shards merged into one chunk: a cottage's pile takes about eight chunks, a few
/// draws each, rather than one draw per shard, and a frame merges at most one chunk
/// (about a megabyte and a half of vertices).
const CHUNK_SHARDS: usize = 4;
/// Movement (m, and quaternion difference) a still shard may show between frames.
const STILL_DISTANCE: f32 = 0.006;
const STILL_TURN: f32 = 0.004;
/// How far (m, and quaternion difference) a merged shard may creep from the pose
/// it was baked at before it leaves its chunk: a piece settling by millimetres on
/// the pile stays drawn as part of it.
const DRIFT_DISTANCE: f32 = 0.05;
const DRIFT_TURN: f32 = 0.0015;

type Pose = (Vec3, Quat);

struct Shard {
    house: u32,
    node: Node,
    /// Its pose last frame, none before the first.
    pose: Option<Pose>,
    /// The chunk drawing it and the pose it was baked at there, while merged.
    baked: Option<(u64, Pose)>,
    /// The pose it last rested at, kept for the remains.
    rested: Option<Pose>,
    /// What of it stays in the remains.
    lasting: Lasting,
    still_since: f64,
    fading: bool,
}

struct Chunk<H> {
    members: Vec<u32>,
    /// None while its model is being added.
    handle: Option<H>,
}

/// One frame's merging: the chunks to free, and the chunk to draw now (hand its
/// handle back with [`SettledShards::built`]).
pub struct Plan<H> {
    pub freed: Vec<H>,
    pub build: Option<(u64, Vec<MergedPart>)>,
}

/// A collapsed cottage's lasting remains: the shards to press, a chunk a frame, and
/// the chunks drawn so far.
struct Remains<H> {
    pending: Vec<(Node, Pose, Option<u32>)>,
    drawn: Vec<H>,
}

pub struct SettledShards<H> {
    shards: HashMap<u32, Shard>,
    chunks: HashMap<u64, Chunk<H>>,
    next_chunk: u64,
    remains: HashMap<u32, Remains<H>>,
}

impl<H> Default for SettledShards<H> {
    fn default() -> Self {
        Self {
            shards: HashMap::new(),
            chunks: HashMap::new(),
            next_chunk: 0,
            remains: HashMap::new(),
        }
    }
}

impl<H> SettledShards<H> {
    /// Follow the shard of fragment `id` from cottage `house`, drawn as `node`.
    pub fn track(&mut self, id: u32, house: u32, node: Node, now: f64, lasting: Lasting) {
        self.shards.insert(
            id,
            Shard {
                house,
                node,
                pose: None,
                baked: None,
                rested: None,
                lasting,
                still_since: now,
                fading: false,
            },
        );
    }

    pub fn tracks(&self, id: u32) -> bool {
        self.shards.contains_key(&id)
    }

    /// A tracked shard's pose this frame, and whether it has begun to fade.
    pub fn observe(&mut self, id: u32, pose: Pose, fading: bool, now: f64) {
        let Some(shard) = self.shards.get_mut(&id) else {
            return;
        };
        let moved = match shard.baked {
            // Merged: it stays while it keeps near the pose it was baked at.
            Some((_, (position, rotation))) => {
                position.distance(pose.0) > DRIFT_DISTANCE
                    || rotation.dot(pose.1).abs() < 1.0 - DRIFT_TURN
            }
            None => shard.pose.is_none_or(|(position, rotation)| {
                position.distance(pose.0) > STILL_DISTANCE
                    || rotation.dot(pose.1).abs() < 1.0 - STILL_TURN
            }),
        };
        if moved {
            shard.still_since = now;
        }
        shard.pose = Some(pose);
        shard.fading = fading;
    }

    /// Whether the shard is drawn in a chunk (so its own view hides).
    pub fn merged(&self, id: u32) -> bool {
        self.shards
            .get(&id)
            .and_then(|shard| shard.baked)
            .and_then(|(chunk, _)| self.chunks.get(&chunk))
            .is_some_and(|chunk| chunk.handle.is_some())
    }

    /// Forget shards whose fragments are gone; their chunks break up.
    pub fn retain(&mut self, live: &HashSet<u32>) -> Vec<H> {
        self.shards.retain(|id, _| live.contains(id));
        let broken: Vec<u64> = self
            .chunks
            .iter()
            .filter(|(_, chunk)| chunk.members.iter().any(|id| !self.shards.contains_key(id)))
            .map(|(&id, _)| id)
            .collect();
        broken
            .into_iter()
            .filter_map(|chunk| self.break_up(chunk))
            .collect()
    }

    /// Free chunk `id`, returning its members to their own views.
    fn break_up(&mut self, id: u64) -> Option<H> {
        let chunk = self.chunks.remove(&id)?;
        for member in &chunk.members {
            if let Some(shard) = self.shards.get_mut(member) {
                shard.baked = None;
            }
        }
        chunk.handle
    }

    /// This frame's merging: chunks a shard moved out of or began to fade in break up,
    /// and at most one new chunk is merged from shards resting on their own, once a
    /// pile has [`CHUNK_SHARDS`] of them or has gone [`REBUILD_GAP`] without another
    /// coming to rest.
    pub fn plan(&mut self, now: f64) -> Plan<H> {
        let broken: Vec<u64> = self
            .chunks
            .iter()
            .filter(|(id, chunk)| {
                chunk.members.iter().any(|member| {
                    self.shards.get(member).is_none_or(|shard| {
                        shard.fading
                            || now - shard.still_since < STILL_SECONDS
                            || shard.baked.is_none_or(|(chunk, _)| chunk != **id)
                    })
                })
            })
            .map(|(&id, _)| id)
            .collect();
        let freed = broken
            .into_iter()
            .filter_map(|chunk| self.break_up(chunk))
            .collect();
        // Shards resting on their own, by cottage; the remains keep where each last lay.
        let mut resting: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&id, shard) in &mut self.shards {
            if !shard.fading && now - shard.still_since >= STILL_SECONDS {
                shard.rested = shard.pose;
                if shard.baked.is_none() {
                    resting.entry(shard.house).or_default().push(id);
                }
            }
        }
        let mut houses: Vec<u32> = resting.keys().copied().collect();
        houses.sort_unstable();
        let ready = houses.into_iter().find_map(|house| {
            let mut members = resting.remove(&house)?;
            let latest = members
                .iter()
                .map(|id| self.shards[id].still_since + STILL_SECONDS)
                .fold(f64::NEG_INFINITY, f64::max);
            let full = members.len() >= CHUNK_SHARDS;
            (full || (members.len() >= 2 && now - latest >= REBUILD_GAP)).then(|| {
                members.sort_unstable();
                members.truncate(CHUNK_SHARDS);
                members
            })
        });
        let build = ready.map(|members| {
            let id = self.next_chunk;
            self.next_chunk += 1;
            for member in &members {
                let shard = self.shards.get_mut(member).expect("resting shard");
                let pose = shard.pose.expect("a resting shard has a pose");
                shard.baked = Some((id, pose));
            }
            let parts = merged(
                members.iter().map(|id| {
                    let shard = &self.shards[id];
                    (&shard.node, shard.pose.expect("posed"))
                }),
                true,
                false,
            );
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

    pub fn built(&mut self, chunk: u64, handle: H) {
        if let Some(chunk) = self.chunks.get_mut(&chunk) {
            chunk.handle = Some(handle);
        }
    }

    /// The next chunk of remains to draw, one a frame: once a cottage's pile has begun
    /// to sink, each shard that ever rested, pressed flat at the pose it rested at.
    /// Hand each one's handle back with [`SettledShards::remains_built`].
    pub fn remains(&mut self) -> Option<(u32, Vec<MergedPart>)> {
        let sinking: HashSet<u32> = self
            .shards
            .values()
            .filter(|shard| shard.fading)
            .map(|shard| shard.house)
            .collect();
        for house in sinking {
            if self.remains.contains_key(&house) {
                continue;
            }
            let mut rested: Vec<(u32, Node, Pose, Lasting)> = self
                .shards
                .iter()
                .filter(|(_, shard)| shard.house == house && shard.lasting != Lasting::No)
                .filter_map(|(&id, shard)| {
                    Some((id, shard.node.clone(), shard.rested?, shard.lasting))
                })
                .collect();
            rested.sort_unstable_by_key(|(id, ..)| *id);
            let pending = rested
                .into_iter()
                .map(|(id, node, pose, lasting)| {
                    (node, pose, (lasting == Lasting::Scraps).then_some(id))
                })
                .collect();
            self.remains.insert(
                house,
                Remains {
                    pending,
                    drawn: Vec::new(),
                },
            );
        }
        let mut houses: Vec<u32> = self
            .remains
            .iter()
            .filter(|(_, remains)| !remains.pending.is_empty())
            .map(|(&house, _)| house)
            .collect();
        houses.sort_unstable();
        let house = *houses.first()?;
        let remains = self.remains.get_mut(&house).expect("listed");
        let take = remains.pending.len().min(CHUNK_SHARDS);
        let chunk: Vec<(Node, Pose, Option<u32>)> = remains.pending.drain(..take).collect();
        let mut parts = merged(
            chunk
                .iter()
                .filter(|(.., scraps)| scraps.is_none())
                .map(|(node, pose, _)| (node, *pose)),
            false,
            true,
        );
        for (node, pose, scraps) in &chunk {
            if let Some(id) = scraps {
                add_scraps(&mut parts, node, *pose, *id);
            }
        }
        Some((house, parts))
    }

    pub fn remains_built(&mut self, house: u32, handle: H) {
        if let Some(remains) = self.remains.get_mut(&house) {
            remains.drawn.push(handle);
        }
    }

    /// Remains of cottages standing again (rebuilt), to free.
    pub fn rebuilt(&mut self, standing: impl Fn(u32) -> bool) -> Vec<H> {
        let gone: Vec<u32> = self
            .remains
            .keys()
            .copied()
            .filter(|&house| standing(house))
            .collect();
        gone.into_iter()
            .filter_map(|house| self.remains.remove(&house))
            .flat_map(|remains| remains.drawn)
            .collect()
    }

    /// Every chunk's and remains' handle, to free on a reset.
    pub fn clear(&mut self) -> Vec<H> {
        self.shards.clear();
        let mut handles: Vec<H> = self
            .chunks
            .drain()
            .filter_map(|(_, chunk)| chunk.handle)
            .collect();
        handles.extend(self.remains.drain().flat_map(|(_, remains)| remains.drawn));
        handles
    }
}

/// Shards' meshes baked at their poses straight into the upload layout, one part per
/// draw material (as batching them would): no scene graph, batched copy or second
/// conversion, each a full copy of a quarter of a million vertices for a whole pile,
/// which the Wasm heap keeps for good once it has grown to hold them. A chunk of the
/// pile shades the ground and itself as one caster (`casts`): one shadow draw grounds
/// its shards, walls included. Remains (`pressed`) lie squashed toward the ground,
/// heights scaled down and normals tilted to match.
fn merged<'a>(
    shards: impl Iterator<Item = (&'a Node, Pose)>,
    casts: bool,
    pressed: bool,
) -> Vec<MergedPart> {
    let mut parts = Vec::new();
    let mut slots = HashMap::new();
    for (node, (position, rotation)) in shards {
        let pose = Mat4::from_rotation_translation(rotation, position);
        gather(node, pose, casts, pressed, &mut parts, &mut slots);
    }
    parts
}

fn gather(
    node: &Node,
    parent: Mat4,
    casts: bool,
    pressed: bool,
    parts: &mut Vec<MergedPart>,
    slots: &mut HashMap<usize, usize>,
) {
    let world = parent * node.local_matrix().as_mat4();
    if let Some(drawable) = node.drawable.as_ref().filter(|d| d.instances.is_none()) {
        // Opaque standard paint goes into vertex colours under one shared material,
        // as `batch` paints it.
        let source = &drawable.material;
        let (material, paint) = if is_paintable(source) {
            let [r, g, b] = hex_to_linear(source.color.0);
            (
                vertex_material(source),
                Some([r as f32, g as f32, b as f32]),
            )
        } else {
            (source.clone(), None)
        };
        let slot = *slots
            .entry(Arc::as_ptr(&material) as usize)
            .or_insert_with(|| {
                parts.push(MergedPart {
                    material,
                    data: MeshData::default(),
                    cast_shadow: casts,
                });
                parts.len() - 1
            });
        let data = &mut parts[slot].data;
        let start = data.vertices.len();
        data.append(&drawable.mesh, Some(&world), paint);
        if pressed {
            for vertex in &mut data.vertices[start..] {
                let y = &mut vertex.position[1];
                *y = (*y * REMAINS_SQUASH).max(REMAINS_LIFT);
                let [x, y, z] = vertex.normal;
                vertex.normal = Vec3::new(x, y / REMAINS_SQUASH, z)
                    .normalize_or(Vec3::Y)
                    .into();
            }
        }
    }
    for child in &node.children {
        gather(child, world, casts, pressed, parts, slots);
    }
}

/// A roof slab's lasting scraps, added to `parts` (the remains' chunk): its meshes
/// at `pose`, cut to [`SCRAPS`] torn patches of its own frame placed by its
/// fragment `id`, crumpled and pressed like the rest of the remains.
fn add_scraps(parts: &mut Vec<MergedPart>, node: &Node, pose: Pose, id: u32) {
    let mut whole = merged(std::iter::once((node, pose)), false, false);
    let (position, rotation) = pose;
    let inverse = rotation.inverse();
    let patches = scrap_patches(id);
    for part in &mut whole {
        let source = std::mem::take(&mut part.data);
        let mut cut = MeshData::default();
        for triangle in source.indices.as_chunks::<3>().0 {
            let corners = triangle.map(|i| source.vertices[i as usize]);
            let local = corners.map(|v| inverse * (Vec3::from(v.position) - position));
            for patch in &patches {
                let mut polygon: Vec<(Vertex, Vec3)> = corners.iter().copied().zip(local).collect();
                for edge in &patch.edges {
                    polygon = clip_polygon(&polygon, *edge);
                    if polygon.len() < 3 {
                        break;
                    }
                }
                if polygon.len() < 3 {
                    continue;
                }
                let base = cut.vertices.len() as u32;
                for (vertex, local) in &polygon {
                    cut.vertices.push(crumple(*vertex, *local, rotation, patch));
                }
                for k in 1..polygon.len() as u32 - 1 {
                    cut.indices.extend([base, base + k, base + k + 1]);
                }
            }
        }
        part.data = cut;
    }
    for part in whole
        .into_iter()
        .filter(|part| !part.data.indices.is_empty())
    {
        match parts
            .iter_mut()
            .find(|other| Arc::ptr_eq(&other.material, &part.material))
        {
            Some(other) => {
                let base = other.data.vertices.len() as u32;
                other.data.vertices.extend(part.data.vertices);
                other
                    .data
                    .indices
                    .extend(part.data.indices.iter().map(|i| base + i));
            }
            None => parts.push(part),
        }
    }
}

/// A torn patch of a slab's frame (x, z): the half-planes that bound it, each
/// `(normal, offset)` keeping `normal · p <= offset`, and its crumple's phase.
struct Patch {
    edges: Vec<(glam::Vec2, f32)>,
    phase: f32,
}

/// [`SCRAPS`] patches for the slab of fragment `id`: skewed, unevenly cut
/// quadrilaterals strewn over its middle, each its own size.
fn scrap_patches(id: u32) -> Vec<Patch> {
    let mut seed = id.wrapping_mul(0x9e37_79b9) ^ 0x5bd1_e995;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 10_000) as f32 / 10_000.0
    };
    (0..SCRAPS)
        .map(|_| {
            let center = glam::Vec2::new(
                (next() * 2.0 - 1.0) * SCRAP_SPREAD,
                (next() * 2.0 - 1.0) * SCRAP_SPREAD,
            );
            let size = SCRAP_SIZE.0 + (SCRAP_SIZE.1 - SCRAP_SIZE.0) * next();
            let turn = next() * std::f32::consts::PI;
            // Four torn sides, each its own reach and slant from a square's.
            let edges = (0..4)
                .map(|k| {
                    let angle =
                        turn + k as f32 * std::f32::consts::FRAC_PI_2 + (next() - 0.5) * 0.6;
                    let normal = glam::Vec2::new(angle.cos(), angle.sin());
                    let reach = size * (0.35 + 0.3 * next());
                    (normal, normal.dot(center) + reach)
                })
                .collect();
            Patch {
                edges,
                phase: next() * std::f32::consts::TAU,
            }
        })
        .collect()
}

/// The part of convex `polygon` (vertices with their points in the slab's frame)
/// on the kept side of `edge`, in the slab's x, z.
fn clip_polygon(
    polygon: &[(Vertex, Vec3)],
    (normal, offset): (glam::Vec2, f32),
) -> Vec<(Vertex, Vec3)> {
    let side = |p: Vec3| normal.dot(glam::Vec2::new(p.x, p.z)) - offset;
    let mut kept = Vec::with_capacity(polygon.len() + 1);
    for (k, &(a, pa)) in polygon.iter().enumerate() {
        let (b, pb) = polygon[(k + 1) % polygon.len()];
        let (da, db) = (side(pa), side(pb));
        if da <= 0.0 {
            kept.push((a, pa));
        }
        if (da <= 0.0) != (db <= 0.0) {
            let t = da / (da - db);
            kept.push((mix_vertex(&a, &b, t), pa.lerp(pb, t)));
        }
    }
    kept
}

fn mix_vertex(a: &Vertex, b: &Vertex, t: f32) -> Vertex {
    let lerp3 = |x: [f32; 3], y: [f32; 3]| Vec3::from(x).lerp(Vec3::from(y), t).into();
    Vertex {
        position: lerp3(a.position, b.position),
        normal: Vec3::from(a.normal)
            .lerp(Vec3::from(b.normal), t)
            .normalize_or(Vec3::Y)
            .into(),
        uv: glam::Vec2::from(a.uv)
            .lerp(glam::Vec2::from(b.uv), t)
            .into(),
        color: glam::Vec4::from(a.color)
            .lerp(glam::Vec4::from(b.color), t)
            .into(),
    }
}

/// A scrap's vertex bent up and down across the sheet, the way a torn sheet
/// crumples, then pressed into the remains like the rest.
fn crumple(mut vertex: Vertex, local: Vec3, rotation: Quat, patch: &Patch) -> Vertex {
    let wave = (local.x / CRUMPLE_LENGTH + patch.phase).sin()
        * (local.z / (CRUMPLE_LENGTH * 1.6) - patch.phase).cos();
    let lift = rotation * Vec3::Y * (CRUMPLE * wave);
    let position = Vec3::from(vertex.position) + lift;
    let y = (position.y * REMAINS_SQUASH).max(REMAINS_LIFT);
    vertex.position = [position.x, y, position.z];
    let [x, ny, z] = vertex.normal;
    vertex.normal = Vec3::new(x, ny / REMAINS_SQUASH, z)
        .normalize_or(Vec3::Y)
        .into();
    vertex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32) -> Pose {
        (Vec3::new(x, 0.0, 0.0), Quat::IDENTITY)
    }

    /// A shard of one painted triangle two metres up.
    fn shard() -> Node {
        let mesh = sloppy_core::geometry::Mesh {
            positions: vec![[0.0, 2.0, 0.0], [1.0, 2.0, 0.0], [0.0, 3.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0; 2]; 3],
            ..Default::default()
        };
        let mut root = Node::default();
        root.children.push(Node::mesh(
            Arc::new(mesh),
            Arc::new(sloppy_core::scene::Material::default()),
        ));
        root
    }

    /// One frame as presentation runs it: every shard observed at `at(id)`, chunks
    /// broken up and at most one merged (given the next handle). Returns the freed
    /// handles and whether a chunk was merged.
    fn frame(
        settled: &mut SettledShards<u32>,
        now: &mut f64,
        at: impl Fn(u32) -> f32,
        fading: bool,
        next: &mut u32,
    ) -> (Vec<u32>, bool) {
        *now += 1.0 / 60.0;
        for id in 1..=10 {
            settled.observe(id, pose(at(id)), fading, *now);
        }
        let plan = settled.plan(*now);
        let merged = plan.build.is_some();
        if let Some((chunk, parts)) = plan.build {
            // One draw material: each shard's triangle, carried to where it rests.
            assert_eq!(parts.len(), 1);
            assert!(parts[0].cast_shadow);
            let vertices = &parts[0].data.vertices;
            assert!(vertices.len() % 3 == 0 && vertices.len() <= 3 * CHUNK_SHARDS);
            assert!(vertices.iter().all(|v| v.position[1] >= 2.0));
            settled.built(chunk, *next);
            *next += 1;
        }
        (plan.freed, merged)
    }

    #[test]
    fn resting_shards_merge_a_chunk_a_frame_and_a_moving_one_breaks_only_its_own() {
        let mut settled: SettledShards<u32> = SettledShards::default();
        for id in 1..=10 {
            settled.track(id, 9, shard(), 0.0, Lasting::Whole);
        }
        let (mut now, mut next) = (0.0, 0);
        let rest = |id: u32| id as f32;
        let mut merges = 0;
        for _ in 0..60 {
            let (freed, merged) = frame(&mut settled, &mut now, rest, false, &mut next);
            assert!(freed.is_empty());
            merges += usize::from(merged);
        }
        // Ten shards: two full chunks, then the last two once the pile goes quiet.
        assert_eq!(merges, 3);
        assert!((1..=10).all(|id| settled.merged(id)));
        // A shard shoved off the pile frees its own chunk, whose other shards merge
        // again at once; the shoved one rests on its own.
        let shoved = |id: u32| if id == 2 { 5.0 } else { id as f32 };
        let (freed, merged) = frame(&mut settled, &mut now, shoved, false, &mut next);
        assert_eq!((freed, merged), (vec![0], true));
        assert!(settled.merged(1) && !settled.merged(2) && settled.merged(5));
        for _ in 0..40 {
            let (freed, merged) = frame(&mut settled, &mut now, shoved, false, &mut next);
            assert!(freed.is_empty() && !merged);
        }
        // The pile sinks: every chunk breaks up, and the remains of all ten shards are
        // pressed a chunk a frame.
        let (mut freed, _) = frame(&mut settled, &mut now, shoved, true, &mut next);
        freed.sort_unstable();
        assert_eq!(freed, vec![1, 2, 3]);
        let mut remains = 0;
        while let Some((house, parts)) = settled.remains() {
            assert_eq!(house, 9);
            // Pressed flat and casting no shadow.
            assert!(!parts[0].cast_shadow);
            for vertex in &parts[0].data.vertices {
                assert!(vertex.position[1] <= 3.0 * REMAINS_SQUASH + 1e-5);
            }
            settled.remains_built(house, 30 + remains);
            remains += 1;
        }
        assert_eq!(remains, 3);
        let live: HashSet<u32> = HashSet::new();
        assert!(settled.retain(&live).is_empty());
        assert_eq!(settled.rebuilt(|house| house == 9), vec![30, 31, 32]);
    }

    /// A roof slab four metres square, two triangles, a metre and a half up.
    fn slab() -> Node {
        let mesh = sloppy_core::geometry::Mesh {
            positions: vec![
                [-2.0, 1.5, -2.0],
                [2.0, 1.5, -2.0],
                [2.0, 1.5, 2.0],
                [-2.0, 1.5, 2.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0; 2]; 4],
            indices: Some(vec![0, 2, 1, 0, 3, 2]),
            ..Default::default()
        };
        let mut root = Node::default();
        root.children.push(Node::mesh(
            Arc::new(mesh),
            Arc::new(sloppy_core::scene::Material::default()),
        ));
        root
    }

    #[test]
    fn a_roof_slab_leaves_a_few_crumpled_scraps_of_itself_in_the_remains() {
        let pose = (Vec3::new(10.0, 0.0, -4.0), Quat::from_rotation_y(0.7));
        for id in [3, 41, 977] {
            let mut parts = Vec::new();
            add_scraps(&mut parts, &slab(), pose, id);
            assert_eq!(parts.len(), 1);
            let data = &parts[0].data;
            assert!(!data.indices.is_empty(), "slab {id} keeps scraps");
            let at = |i: u32| Vec3::from(data.vertices[i as usize].position);
            let area: f32 = data
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&[a, b, c]| (at(b) - at(a)).cross(at(c) - at(a)).length() / 2.0)
                .sum();
            // Scraps, not the slab: a few small pieces of its sixteen square metres.
            assert!(area > 0.2 && area < 4.0, "slab {id}: {area} m² of scraps");
            for vertex in &data.vertices {
                let p = Vec3::from(vertex.position);
                // Pressed low with the remains, crumples and all, and torn from
                // where the slab lay.
                assert!(p.y <= (1.5 + CRUMPLE) * REMAINS_SQUASH + 1e-4);
                let local = pose.1.inverse() * (p - pose.0);
                assert!(local.x.abs() <= 2.0 + 1e-3 && local.z.abs() <= 2.0 + 1e-3);
            }
        }
    }
}
