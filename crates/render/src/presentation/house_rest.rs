//! A collapsed cottage's shards drawn as one model once they come to rest.
//!
//! Each falling shard is its own model (a draw and, for the roof, a shadow), which
//! a collapse can afford for the seconds it moves but not for the many seconds its
//! pile lies on the heap. Shards that have kept still a moment are baked at their
//! resting poses into one merged model per cottage and their own views hidden; a
//! shard that moves again (a tank shoving it) leaves the merge and shows on its own,
//! and one starting its cleanup fade leaves it too, so it sinks as it would alone.
//! Presentation only: the shards' bodies are untouched.

use std::collections::{HashMap, HashSet};

use glam::{DQuat, DVec3, Quat, Vec3};
use sloppy_core::models::batch;
use sloppy_core::scene::Node;

/// Seconds a shard keeps still before it joins its cottage's merge, and the least
/// time between rebuilding a merge to take in more.
const STILL_SECONDS: f64 = 0.35;
const REBUILD_GAP: f64 = 0.6;
/// Movement (m, and quaternion difference) a still shard may show between frames.
const STILL_DISTANCE: f32 = 0.004;
const STILL_TURN: f32 = 0.004;

struct Shard {
    house: u32,
    node: Node,
    /// Its pose last frame, none before the first.
    pose: Option<(Vec3, Quat)>,
    still_since: f64,
    fading: bool,
}

struct Merge<H> {
    members: Vec<u32>,
    built_at: f64,
    handle: Option<H>,
}

/// A merge to (re)build: free `old`, then draw `node` (if any) and hand back its
/// handle with [`SettledShards::built`].
pub struct Rebuild<H> {
    pub house: u32,
    pub old: Option<H>,
    pub node: Option<Node>,
}

pub struct SettledShards<H> {
    shards: HashMap<u32, Shard>,
    merges: HashMap<u32, Merge<H>>,
}

impl<H> Default for SettledShards<H> {
    fn default() -> Self {
        Self {
            shards: HashMap::new(),
            merges: HashMap::new(),
        }
    }
}

impl<H> SettledShards<H> {
    /// Follow the shard of fragment `id` from cottage `house`, drawn as `node`.
    pub fn track(&mut self, id: u32, house: u32, node: Node, now: f64) {
        self.shards.insert(
            id,
            Shard {
                house,
                node,
                pose: None,
                still_since: now,
                fading: false,
            },
        );
    }

    pub fn tracks(&self, id: u32) -> bool {
        self.shards.contains_key(&id)
    }

    /// A tracked shard's pose this frame, and whether it has begun to fade.
    pub fn observe(&mut self, id: u32, pose: (Vec3, Quat), fading: bool, now: f64) {
        let Some(shard) = self.shards.get_mut(&id) else {
            return;
        };
        let moved = shard.pose.is_none_or(|(position, rotation)| {
            position.distance(pose.0) > STILL_DISTANCE
                || rotation.dot(pose.1).abs() < 1.0 - STILL_TURN
        });
        if moved {
            shard.still_since = now;
        }
        shard.pose = Some(pose);
        shard.fading = fading;
    }

    /// Whether the shard is drawn in its cottage's merge (so its own view hides).
    pub fn merged(&self, id: u32) -> bool {
        self.shards.get(&id).is_some_and(|shard| {
            self.merges
                .get(&shard.house)
                .is_some_and(|merge| merge.handle.is_some() && merge.members.contains(&id))
        })
    }

    /// Forget shards whose fragments are gone; the merges left with none to free.
    pub fn retain(&mut self, live: &HashSet<u32>) -> Vec<H> {
        self.shards.retain(|id, _| live.contains(id));
        let houses: HashSet<u32> = self.shards.values().map(|shard| shard.house).collect();
        let gone: Vec<u32> = self
            .merges
            .keys()
            .copied()
            .filter(|house| !houses.contains(house))
            .collect();
        gone.into_iter()
            .filter_map(|house| self.merges.remove(&house)?.handle)
            .collect()
    }

    /// The merges whose resting shards changed: one that lost a shard (moving again
    /// or fading) rebuilds at once, one that could take in newly settled shards at
    /// most every [`REBUILD_GAP`].
    pub fn plan(&mut self, now: f64) -> Vec<Rebuild<H>> {
        let mut settled: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut houses: Vec<u32> = Vec::new();
        for (&id, shard) in &self.shards {
            if !houses.contains(&shard.house) {
                houses.push(shard.house);
            }
            if !shard.fading && now - shard.still_since >= STILL_SECONDS {
                settled.entry(shard.house).or_default().push(id);
            }
        }
        houses.sort_unstable();
        let mut rebuilds = Vec::new();
        for house in houses {
            let mut members = settled.remove(&house).unwrap_or_default();
            members.sort_unstable();
            let merge = self.merges.entry(house).or_insert(Merge {
                members: Vec::new(),
                built_at: f64::NEG_INFINITY,
                handle: None,
            });
            if members == merge.members {
                continue;
            }
            let lost = merge.members.iter().any(|id| !members.contains(id));
            if !lost && now - merge.built_at < REBUILD_GAP {
                continue;
            }
            merge.built_at = now;
            let node = (members.len() >= 2).then(|| merged_node(&self.shards, &members));
            if node.is_none() {
                members.clear();
            }
            merge.members = members;
            rebuilds.push(Rebuild {
                house,
                old: merge.handle.take(),
                node,
            });
        }
        rebuilds
    }

    pub fn built(&mut self, house: u32, handle: H) {
        if let Some(merge) = self.merges.get_mut(&house) {
            merge.handle = Some(handle);
        }
    }

    /// Every merge's handle, to free on a reset.
    pub fn clear(&mut self) -> Vec<H> {
        self.shards.clear();
        self.merges
            .drain()
            .filter_map(|(_, merge)| merge.handle)
            .collect()
    }
}

/// The shards' meshes baked at their poses into one batched model.
fn merged_node(shards: &HashMap<u32, Shard>, members: &[u32]) -> Node {
    let mut root = Node::group("house-rest");
    for id in members {
        let shard = &shards[id];
        let Some((position, rotation)) = shard.pose else {
            continue;
        };
        let position = DVec3::new(
            f64::from(position.x),
            f64::from(position.y),
            f64::from(position.z),
        );
        let rotation = DQuat::from_xyzw(
            f64::from(rotation.x),
            f64::from(rotation.y),
            f64::from(rotation.z),
            f64::from(rotation.w),
        );
        collect(&shard.node, position, rotation, &mut root);
    }
    batch(&mut root);
    root
}

/// The drawables under `node` (whose own parts carry no further transforms past
/// one level) carried into the pose.
fn collect(node: &Node, position: DVec3, rotation: DQuat, root: &mut Node) {
    let local_position = position + rotation * node.position;
    let local_rotation = rotation * node.rotation;
    if let Some(drawable) = &node.drawable {
        root.children.push(Node {
            drawable: Some(drawable.clone()),
            position: local_position,
            rotation: local_rotation,
            scale: node.scale,
            ..Node::default()
        });
    }
    for child in &node.children {
        collect(child, local_position, local_rotation, root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_shards_merge_and_a_moving_one_leaves_at_once() {
        let mut settled: SettledShards<u8> = SettledShards::default();
        for id in [1, 2, 3] {
            settled.track(id, 9, Node::group(""), 0.0);
        }
        let pose = |x: f32| (Vec3::new(x, 0.0, 0.0), Quat::IDENTITY);
        let mut now = 0.0;
        for _ in 0..30 {
            now += 1.0 / 60.0;
            for id in [1, 2, 3] {
                settled.observe(id, pose(id as f32), false, now);
            }
            for rebuild in settled.plan(now) {
                settled.built(rebuild.house, 7);
            }
        }
        assert!([1, 2, 3].iter().all(|&id| settled.merged(id)));
        now += 1.0 / 60.0;
        settled.observe(2, pose(5.0), false, now);
        let rebuilds = settled.plan(now);
        assert_eq!(rebuilds.len(), 1);
        assert_eq!(rebuilds[0].old, Some(7));
        settled.built(9, 8);
        assert!(!settled.merged(2) && settled.merged(1));
        let live: HashSet<u32> = HashSet::new();
        assert_eq!(settled.retain(&live), vec![8]);
    }
}
