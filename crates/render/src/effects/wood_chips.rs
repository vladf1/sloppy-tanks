//! Wood a shell tears out of a cargo crate: splinters, board shards and chips that
//! burst from the struck face or from the bursting crate, tumble, bounce, tip over
//! flat on the ground, lie a few seconds and shrink away, plus the pale excelsior
//! packing a burst crate spills, which flutters down slowly. Lit by the sun like the
//! falling leaves. Two bounded instanced draws, wood and straw; a full pool drops its
//! oldest piece. The crate's big panels, lid halves and skid are simulation
//! fragments; everything here is cosmetic and draws from `CosmeticRandom`.

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::f64::consts::{FRAC_PI_2, TAU};

use glam::{DVec3, Mat3, Mat4, Quat, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::sim::{CoverKind, SimEvent, SimEventType};

use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const MAX_WOOD_CHIPS: usize = 512;
pub const MAX_STRAW: usize = 256;
const GRAVITY: f64 = 9.8;
/// The share of its fall speed a piece keeps bouncing off the ground, and of its
/// slide and tumble a ground contact leaves it.
const RESTITUTION: f64 = 0.25;
const GROUND_FRICTION: f64 = 0.55;
/// A piece bouncing up slower than this, or on its third contact, settles.
const SETTLE_SPEED: f64 = 0.9;
const MAX_BOUNCES: u8 = 3;
/// Seconds a settling piece takes to tip over flat.
const SETTLE_SECONDS: f32 = 0.16;
/// Grounded pieces slide to a stop at this rate (1/s), and count as stopped below
/// this speed (m/s).
const SLIDE_DRAG: f64 = 7.0;
const STOPPED: f64 = 0.02;
/// Landed pieces lie this far above the ground, clear of its decals and the harbor
/// paving.
const CLEARANCE: f64 = 0.03;
/// Seconds a landed piece lies, [minimum, random span], then shrinks away.
const WOOD_REST: [f64; 2] = [6.0, 4.0];
const STRAW_REST: [f64; 2] = [4.0, 3.0];
const SHRINK: f64 = 0.8;
/// Physics never steps further than this at once (a hidden tab's catch-up frame).
const MAX_STEP: f64 = 0.05;
/// A landed piece props up on debris by at most this tilt (radians).
const MAX_LEAN: f64 = 0.14;

/// A shell hit tears [minimum, random span] pieces from the face, the first few long
/// slivers, in a cone this wide (radians) around the face normal; this share skids
/// out along the face instead.
const HIT_PIECES: [usize; 2] = [10, 7];
const HIT_LONG_SLIVERS: usize = 2;
const HIT_CONE: f64 = 1.22;
const HIT_FACE_SPRAY: f64 = 0.22;
/// A shell hit without its height strikes about here.
pub(super) const HIT_HEIGHT: f64 = 1.1;
/// A burst crate throws [minimum, random span] pieces from inside this half width
/// of its footprint (footprints are 2.4-3.1 m square).
const BURST_PIECES: [usize; 2] = [56, 12];
const BURST_HALF_WIDTH: f64 = 1.1;
/// The first pieces of a burst are broken-off lengths of board.
const BURST_BOARDS: usize = 6;
/// A loose board a shell bursts throws this many pieces per metre of its length,
/// within [fewest, most].
const SHATTER_PIECES_PER_METRE: f64 = 7.0;
const SHATTER_PIECES: [usize; 2] = [8, 22];
/// A destroyed crate without its height stood about this tall.
pub(super) const CRATE_HEIGHT: f64 = 2.4;
/// Of a burst, this share only tumbles out around the crate's foot, this share is
/// spall thrown back out of the struck face toward the gun, and of the rest flung
/// on, this share goes up in high arcs.
const SLUMP: f64 = 0.3;
const SPALL: f64 = 0.12;
const HIGH_ARC: f64 = 0.2;
/// Excelsior tufts a burst spills, each of [minimum, random span] fibres.
const STRAW_TUFTS: usize = 6;
const STRAW_FIBRES: [usize; 2] = [7, 4];

/// Wood colors: split-open fresh wood, its darker heart and end grain, and a crate's
/// weathered boards when the event names none.
const FRESH_WOOD: [u32; 2] = [0xe2c596, 0xd2ad78];
const INNER_WOOD: u32 = 0x8a6440;
const CRATE_WOOD: u32 = 0xb47a49;
const STRAW_COLORS: [u32; 2] = [0xd8b45a, 0xe6c97a];

/// A piece's size in metres, each [minimum, random span]: length along the grain,
/// width and thickness.
#[derive(Clone, Copy, Debug)]
struct Cut {
    length: [f64; 2],
    width: [f64; 2],
    thickness: [f64; 2],
}

const LONG_SLIVER: Cut = Cut {
    length: [0.45, 0.3],
    width: [0.045, 0.03],
    thickness: [0.022, 0.012],
};
const SLIVER: Cut = Cut {
    length: [0.22, 0.3],
    width: [0.035, 0.035],
    thickness: [0.018, 0.012],
};
const SHARD: Cut = Cut {
    length: [0.14, 0.16],
    width: [0.08, 0.1],
    thickness: [0.028, 0.02],
};
const BOARD: Cut = Cut {
    length: [0.4, 0.35],
    width: [0.08, 0.06],
    thickness: [0.035, 0.015],
};
const CHIP: Cut = Cut {
    length: [0.06, 0.06],
    width: [0.05, 0.05],
    thickness: [0.02, 0.014],
};

/// Which pieces a hit or burst is made of: slivers below `slivers`, shards below
/// `shards`, chips above; and its colors: fresh wood below `fresh`, the crate's
/// boards below `boards`, inner wood above.
#[derive(Clone, Copy, Debug)]
struct Mix {
    slivers: f64,
    shards: f64,
    fresh: f64,
    boards: f64,
}

/// A struck face mostly shows freshly split wood.
const HIT_MIX: Mix = Mix {
    slivers: 0.35,
    shards: 0.6,
    fresh: 0.6,
    boards: 0.9,
};
const BURST_MIX: Mix = Mix {
    slivers: 0.45,
    shards: 0.8,
    fresh: 0.4,
    boards: 0.8,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Flying {
        bounces: u8,
    },
    Settling {
        from: Quat,
        to: Quat,
        progress: f32,
    },
    /// Seconds left lying; negative while shrinking away.
    Lying {
        left: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub position: DVec3,
    pub velocity: DVec3,
    rotation: Quat,
    /// The tumble's world axis and rate (rad/s).
    spin_axis: Vec3,
    spin: f32,
    /// The instance scale, and the half extents that meet the ground.
    scale: Vec3,
    half: Vec3,
    /// Linear air drag per second: none for wood, a lot for straw.
    drag: f64,
    /// Straw's sideways flutter: speed (m/s), rate (rad/s) and phase, along a fixed
    /// horizontal direction.
    flutter: [f64; 3],
    flutter_direction: [f64; 2],
    age: f64,
    /// Seconds it lies once landed.
    rest: f64,
    /// The tilt it lands with.
    lean: f32,
    stage: Stage,
    /// Linear RGB.
    color: [f32; 3],
}

impl Piece {
    pub fn landed(&self) -> bool {
        matches!(self.stage, Stage::Lying { .. })
    }

    /// The share of its size left while shrinking away.
    fn size(&self) -> f32 {
        match self.stage {
            Stage::Lying { left } if left < 0.0 => (1.0 + left / SHRINK).max(0.0) as f32,
            _ => 1.0,
        }
    }

    fn world(&self) -> Mat4 {
        let size = self.size();
        let mut position = self.position.as_vec3();
        if size < 1.0 {
            // It sinks as it shrinks, its underside staying on the ground.
            position.y = CLEARANCE as f32 + extent(self.rotation, self.half) as f32 * size;
        }
        Mat4::from_scale_rotation_translation(self.scale * size, self.rotation, position)
    }
}

/// How far below its centre a box with these half extents reaches at `rotation`.
fn extent(rotation: Quat, half: Vec3) -> f64 {
    let axes = Mat3::from_quat(rotation);
    f64::from(
        axes.x_axis.y.abs() * half.x + axes.y_axis.y.abs() * half.y + axes.z_axis.y.abs() * half.z,
    )
}

/// The resting pose nearest `rotation`: lying on its broad face (local Y up or
/// down) along the same heading, propped up by `lean`.
fn lying(rotation: Quat, lean: f32) -> Quat {
    let grain = rotation * Vec3::X;
    let yaw = (-grain.z).atan2(grain.x);
    let roll = if (rotation * Vec3::Y).y < 0.0 {
        PI + lean
    } else {
        lean
    };
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(roll)
}

fn unit_vector(random: &mut CosmeticRandom) -> Vec3 {
    let z = random.next_f64() * 2.0 - 1.0;
    let angle = random.next_f64() * TAU;
    let ring = (1.0 - z * z).sqrt();
    Vec3::new(
        (ring * angle.cos()) as f32,
        (ring * angle.sin()) as f32,
        z as f32,
    )
}

fn heading(random: &mut CosmeticRandom) -> [f64; 2] {
    let angle = random.next_f64() * TAU;
    [angle.cos(), angle.sin()]
}

/// The unit horizontal direction of (x, z), if it has one.
fn horizontal(x: f64, z: f64) -> Option<[f64; 2]> {
    let length = x.hypot(z);
    (length > 1e-6).then(|| [x / length, z / length])
}

/// `direction` turned by `angle` about the vertical.
fn turned([x, z]: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [x * cos - z * sin, x * sin + z * cos]
}

fn between(random: &mut CosmeticRandom, [minimum, span]: [f64; 2]) -> f64 {
    minimum + random.next_f64() * span
}

fn shade(color: [f32; 3], light: f64) -> [f32; 3] {
    color.map(|channel| channel * light as f32)
}

/// A splinter along local X, inside the unit cube: a full board section that
/// breaks off at a slant at one end and tapers to an off-centre point at the other.
pub fn splinter_mesh() -> Mesh {
    // Rings around the grain, counter-clockwise seen from +X: (y-, z-), (y+, z-),
    // (y+, z+), (y-, z+).
    let ring = |x: [f64; 2], y: f64, z: [f64; 2]| {
        [
            [x[0], -y, z[0]],
            [x[0], y, z[0]],
            [x[1], y, z[1]],
            [x[1], -y, z[1]],
        ]
    };
    let rings = [
        ring([-0.5, -0.32], 0.5, [-0.5, 0.5]),
        ring([0.1, 0.22], 0.5, [-0.5, 0.5]),
        ring([0.5, 0.4], 0.28, [-0.36, 0.02]),
    ];
    let mut positions = Vec::new();
    let mut triangle = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        positions.extend(a.into_iter().chain(b).chain(c));
    };
    for pair in rings.windows(2) {
        let (back, front) = (pair[0], pair[1]);
        for i in 0..4 {
            let j = (i + 1) % 4;
            triangle(back[i], back[j], front[j]);
            triangle(back[i], front[j], front[i]);
        }
    }
    let [back, _, tip] = rings;
    triangle(back[3], back[2], back[1]);
    triangle(back[3], back[1], back[0]);
    triangle(tip[0], tip[1], tip[2]);
    triangle(tip[0], tip[2], tip[3]);
    let mut mesh = Mesh::from_f64(&positions, &[], &[], None);
    mesh.compute_vertex_normals();
    mesh
}

/// Turns of a straw fibre's curl, its radius and the ribbon's half width, per
/// unit of fibre length (fibres scale uniformly).
const STRAW_TURNS: f64 = 1.3;
const STRAW_CURL: f64 = 0.08;
const STRAW_HALF_WIDTH: f64 = 0.05;
const STRAW_SEGMENTS: usize = 10;

/// An excelsior fibre: a thin ribbon one unit long along X, curling around it.
pub fn straw_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=STRAW_SEGMENTS {
        let t = i as f64 / STRAW_SEGMENTS as f64;
        let angle = TAU * STRAW_TURNS * t;
        let (sin, cos) = angle.sin_cos();
        // The ribbon spans the curl's radius, so it twists like a shaving.
        for side in [-1.0, 1.0] {
            let reach = STRAW_CURL + side * STRAW_HALF_WIDTH;
            positions.extend([t - 0.5, sin * reach, cos * reach]);
        }
        if i > 0 {
            let first = (i as u32 - 1) * 2;
            indices.extend([first, first + 1, first + 3, first, first + 3, first + 2]);
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
    mesh.compute_vertex_normals();
    mesh
}

/// Shells chipping and bursting cargo crates; `wood_records` and `straw_records`
/// are rebuilt whenever a piece moves.
#[derive(Clone, Debug)]
pub struct WoodChips {
    wood: VecDeque<Piece>,
    straw: VecDeque<Piece>,
    pub wood_records: PoolBuffer,
    pub straw_records: PoolBuffer,
    /// Pieces were added or removed since the records were written.
    changed: bool,
}

impl Default for WoodChips {
    fn default() -> Self {
        Self {
            wood: VecDeque::with_capacity(MAX_WOOD_CHIPS),
            straw: VecDeque::with_capacity(MAX_STRAW),
            wood_records: PoolBuffer::new(MAX_WOOD_CHIPS),
            straw_records: PoolBuffer::new(MAX_STRAW),
            changed: false,
        }
    }
}

impl WoodChips {
    pub fn reset(&mut self) {
        self.wood.clear();
        self.straw.clear();
        self.wood_records.clear();
        self.straw_records.clear();
        self.changed = false;
    }

    /// Live wood pieces and straw fibres.
    pub fn len(&self) -> usize {
        self.wood.len() + self.straw.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn wood(&self) -> impl Iterator<Item = &Piece> {
        self.wood.iter()
    }

    pub fn straw(&self) -> impl Iterator<Item = &Piece> {
        self.straw.iter()
    }

    /// Splinter a crate's struck face, burst a destroyed crate, or shatter a loose
    /// crate or watchtower board a shell flew through; other events throw nothing.
    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        match (event.kind, event.cover_kind) {
            (SimEventType::Impact, Some(CoverKind::Cargo)) => self.splinter_face(event, random),
            (SimEventType::Destroy, Some(CoverKind::Cargo)) => self.burst(event, random),
            (SimEventType::Shatter, Some(CoverKind::Cargo | CoverKind::Tower)) => {
                self.shatter(event, random)
            }
            _ => {}
        }
    }

    /// A loose board a shell flew through bursts into splinters and chips across its
    /// length, most carried on with the shell, a few dropping where it stood.
    fn shatter(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let size = event.size.unwrap_or(1.5);
        let height = event.height.unwrap_or(HIT_HEIGHT);
        let along = event
            .from
            .and_then(|from| horizontal(event.x - from.x, event.z - from.z));
        let boards = event.color.unwrap_or(CRATE_WOOD);
        let count = ((size * SHATTER_PIECES_PER_METRE).round() as usize)
            .clamp(SHATTER_PIECES[0], SHATTER_PIECES[1]);
        for i in 0..count {
            let reach = size * 0.45;
            let offset = heading(random).map(|v| v * reach * random.next_f64());
            let position = DVec3::new(
                event.x + offset[0],
                (height + (random.next_f64() - 0.5) * size * 0.4).max(0.1),
                event.z + offset[1],
            );
            let speed = 1.5 + random.next_f64() * 4.5;
            let spread = turned(
                along.unwrap_or_else(|| heading(random)),
                (random.next_f64() - 0.5) * 1.6,
            );
            let velocity = DVec3::new(
                spread[0] * speed,
                0.8 + random.next_f64() * 3.2,
                spread[1] * speed,
            );
            let cut = if i < 2 {
                BOARD
            } else {
                pick_cut(random, BURST_MIX)
            };
            let color = wood_color(random, BURST_MIX, boards);
            let piece = wood_piece(random, cut, position, velocity, color);
            push(&mut self.wood, MAX_WOOD_CHIPS, piece);
        }
        self.changed = true;
    }

    /// A shell hit: pieces burst out of the face back toward the gun, `from`.
    fn splinter_face(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let height = event.height.unwrap_or(HIT_HEIGHT);
        // Without the shell's direction any side may be the face.
        let out = event
            .from
            .and_then(|from| horizontal(from.x - event.x, from.z - event.z))
            .unwrap_or_else(|| heading(random));
        let along = turned(out, FRAC_PI_2);
        let boards = event.color.unwrap_or(CRATE_WOOD);
        let count = HIT_PIECES[0] + (random.next_f64() * HIT_PIECES[1] as f64) as usize;
        for i in 0..count {
            let cut = if i < HIT_LONG_SLIVERS {
                LONG_SLIVER
            } else {
                pick_cut(random, HIT_MIX)
            };
            let turn = if random.next_f64() < HIT_FACE_SPRAY {
                let side = if random.next_f64() < 0.5 { -1.0 } else { 1.0 };
                side * (1.05 + random.next_f64() * 0.4)
            } else {
                (random.next_f64() - 0.5) * HIT_CONE
            };
            let direction = turned(out, turn);
            let elevation = 0.15 + random.next_f64() * 0.75;
            // Most drop at the crate's foot; a few fly a few metres.
            let speed = 1.0 + random.next_f64().powi(2) * 4.0;
            let offset = (random.next_f64() - 0.5) * 0.3;
            let position = DVec3::new(
                event.x + out[0] * 0.06 + along[0] * offset,
                height + (random.next_f64() - 0.5) * 0.18,
                event.z + out[1] * 0.06 + along[1] * offset,
            );
            let velocity = DVec3::new(
                direction[0] * speed * elevation.cos(),
                speed * elevation.sin(),
                direction[1] * speed * elevation.cos(),
            );
            let color = wood_color(random, HIT_MIX, boards);
            let piece = wood_piece(random, cut, position, velocity, color);
            push(&mut self.wood, MAX_WOOD_CHIPS, piece);
        }
        self.changed = true;
    }

    /// A destroyed crate: its boards splinter throughout and fly on along the
    /// killing shot (when known), some spalling back out of the struck face, and
    /// its packing spills out in tufts.
    fn burst(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let height = event.height.unwrap_or(CRATE_HEIGHT);
        let shot = event
            .from
            .and_then(|from| horizontal(event.x - from.x, event.z - from.z));
        let boards = event.color.unwrap_or(CRATE_WOOD);
        let count = BURST_PIECES[0] + (random.next_f64() * BURST_PIECES[1] as f64) as usize;
        for i in 0..count {
            let dx = (random.next_f64() * 2.0 - 1.0) * BURST_HALF_WIDTH;
            let dz = (random.next_f64() * 2.0 - 1.0) * BURST_HALF_WIDTH;
            let y = 0.3 + random.next_f64() * (height - 0.3);
            let outward = horizontal(dx, dz).unwrap_or_else(|| heading(random));
            let roll = random.next_f64();
            let slump = roll < SLUMP;
            let [vx, vz] = if slump {
                let speed = 0.4 + random.next_f64() * 1.8;
                outward.map(|v| v * speed)
            } else {
                match shot {
                    Some(along) if roll < SLUMP + SPALL => {
                        let back = [-along[0], -along[1]];
                        let away = turned(back, (random.next_f64() - 0.5) * 1.4);
                        let speed = 2.5 + random.next_f64() * 3.0;
                        away.map(|v| v * speed)
                    }
                    Some(along) => {
                        let push = 2.5 + random.next_f64() * 3.5;
                        let spread = 0.8 + random.next_f64() * 1.4;
                        [0, 1].map(|i| along[i] * push + outward[i] * spread)
                    }
                    None => {
                        let speed = 3.0 + random.next_f64() * 4.0;
                        outward.map(|v| v * speed)
                    }
                }
            };
            let vy = if slump {
                0.5 + random.next_f64() * 2.0
            } else if random.next_f64() < HIGH_ARC {
                6.0 + random.next_f64() * 3.5
            } else {
                1.5 + random.next_f64() * 4.0
            };
            let position = DVec3::new(event.x + dx, y, event.z + dz);
            let (cut, color) = if i < BURST_BOARDS {
                // Weathered outside, a board's faces show the crate's color.
                let light = 0.85 + random.next_f64() * 0.25;
                (BOARD, shade(hex_to_linear(boards), light))
            } else {
                (
                    pick_cut(random, BURST_MIX),
                    wood_color(random, BURST_MIX, boards),
                )
            };
            let piece = wood_piece(random, cut, position, DVec3::new(vx, vy, vz), color);
            push(&mut self.wood, MAX_WOOD_CHIPS, piece);
        }
        for _ in 0..STRAW_TUFTS {
            let dx = (random.next_f64() * 2.0 - 1.0) * BURST_HALF_WIDTH * 0.8;
            let dz = (random.next_f64() * 2.0 - 1.0) * BURST_HALF_WIDTH * 0.8;
            let y = 0.6 + random.next_f64() * (height * 0.8 - 0.6).max(0.0);
            let outward = horizontal(dx, dz).unwrap_or_else(|| heading(random));
            let [vx, vz] = match shot {
                Some(along) => {
                    let push = 3.0 + random.next_f64() * 3.0;
                    let spread = 1.2 + random.next_f64() * 1.8;
                    [0, 1].map(|i| along[i] * push + outward[i] * spread)
                }
                None => {
                    let speed = 3.5 + random.next_f64() * 3.0;
                    outward.map(|v| v * speed)
                }
            };
            let vy = 3.0 + random.next_f64() * 3.5;
            let fibres = STRAW_FIBRES[0] + (random.next_f64() * STRAW_FIBRES[1] as f64) as usize;
            for _ in 0..fibres {
                let mut jitter = || (random.next_f64() - 0.5) * 2.0;
                let position = DVec3::new(
                    event.x + dx + jitter() * 0.2,
                    y + jitter() * 0.15,
                    event.z + dz + jitter() * 0.2,
                );
                let velocity = DVec3::new(vx + jitter(), vy + jitter() * 0.8, vz + jitter());
                let piece = straw_piece(random, position, velocity);
                push(&mut self.straw, MAX_STRAW, piece);
            }
        }
        self.changed = true;
    }

    /// Advance every piece and rewrite the records when anything moved.
    pub fn update(&mut self, dt: f64, random: &mut CosmeticRandom) {
        let step = dt.min(MAX_STEP);
        let mut moved = self.changed;
        for pieces in [&mut self.wood, &mut self.straw] {
            let before = pieces.len();
            pieces.retain_mut(|piece| {
                let (live, moving) = advance(piece, dt, step, random);
                moved |= moving;
                live
            });
            moved |= pieces.len() != before;
        }
        if moved {
            write(&mut self.wood_records, &self.wood);
            write(&mut self.straw_records, &self.straw);
        }
        self.changed = false;
    }
}

fn push(pieces: &mut VecDeque<Piece>, capacity: usize, piece: Piece) {
    if pieces.len() >= capacity {
        pieces.pop_front();
    }
    pieces.push_back(piece);
}

fn write(records: &mut PoolBuffer, pieces: &VecDeque<Piece>) {
    records.clear();
    for piece in pieces {
        let [r, g, b] = piece.color;
        records.push(record(piece.world(), [r, g, b, 1.0], [0.0; 4]));
    }
}

fn pick_cut(random: &mut CosmeticRandom, mix: Mix) -> Cut {
    let roll = random.next_f64();
    if roll < mix.slivers {
        SLIVER
    } else if roll < mix.shards {
        SHARD
    } else {
        CHIP
    }
}

fn wood_color(random: &mut CosmeticRandom, mix: Mix, boards: u32) -> [f32; 3] {
    let roll = random.next_f64();
    let light = 0.85 + random.next_f64() * 0.25;
    let base = if roll < mix.fresh {
        let [pale, warm] = FRESH_WOOD.map(hex_to_linear);
        let k = random.next_f64() as f32;
        [0, 1, 2].map(|i| pale[i] + (warm[i] - pale[i]) * k)
    } else if roll < mix.boards {
        hex_to_linear(boards)
    } else {
        hex_to_linear(INNER_WOOD)
    };
    shade(base, light)
}

fn wood_piece(
    random: &mut CosmeticRandom,
    cut: Cut,
    position: DVec3,
    velocity: DVec3,
    color: [f32; 3],
) -> Piece {
    let length = between(random, cut.length) as f32;
    let width = between(random, cut.width) as f32;
    let thickness = between(random, cut.thickness) as f32;
    let scale = Vec3::new(length, thickness, width);
    Piece {
        position,
        velocity,
        rotation: Quat::from_axis_angle(unit_vector(random), (random.next_f64() * TAU) as f32),
        spin_axis: unit_vector(random),
        // Longer pieces tumble slower.
        spin: ((8.0 + random.next_f64() * 17.0) / (1.0 + 3.0 * f64::from(length))) as f32,
        scale,
        half: scale * 0.5,
        drag: 0.0,
        flutter: [0.0; 3],
        flutter_direction: [1.0, 0.0],
        age: 0.0,
        rest: between(random, WOOD_REST),
        lean: ((random.next_f64() - 0.5) * 2.0 * MAX_LEAN) as f32,
        stage: Stage::Flying { bounces: 0 },
        color,
    }
}

fn straw_piece(random: &mut CosmeticRandom, position: DVec3, velocity: DVec3) -> Piece {
    let length = (0.2 + random.next_f64() * 0.2) as f32;
    let [pale, gold] = STRAW_COLORS.map(hex_to_linear);
    let k = random.next_f64() as f32;
    let color = shade(
        [0, 1, 2].map(|i| gold[i] + (pale[i] - gold[i]) * k),
        0.85 + random.next_f64() * 0.25,
    );
    let reach = (STRAW_CURL + STRAW_HALF_WIDTH) as f32;
    Piece {
        position,
        velocity,
        rotation: Quat::from_axis_angle(unit_vector(random), (random.next_f64() * TAU) as f32),
        spin_axis: unit_vector(random),
        spin: (3.0 + random.next_f64() * 5.0) as f32,
        scale: Vec3::splat(length),
        half: Vec3::new(0.5, reach, reach) * length,
        drag: 3.5 + random.next_f64() * 1.5,
        flutter: [
            0.4 + random.next_f64() * 0.5,
            5.0 + random.next_f64() * 5.0,
            random.next_f64() * TAU,
        ],
        flutter_direction: heading(random),
        age: 0.0,
        rest: between(random, STRAW_REST),
        lean: 0.0,
        stage: Stage::Flying { bounces: 0 },
        color,
    }
}

/// Advance one piece by `dt` (its physics by `step`); returns whether it remains
/// and whether its pose changed.
fn advance(piece: &mut Piece, dt: f64, step: f64, random: &mut CosmeticRandom) -> (bool, bool) {
    piece.age += dt;
    match piece.stage {
        Stage::Flying { bounces } => {
            fly(piece, step);
            let floor = CLEARANCE + extent(piece.rotation, piece.half);
            if piece.position.y < floor && piece.velocity.y < 0.0 {
                piece.position.y = floor;
                let rebound = -piece.velocity.y * RESTITUTION;
                piece.velocity.x *= GROUND_FRICTION;
                piece.velocity.z *= GROUND_FRICTION;
                // A knock on the ground changes how it tumbles.
                piece.spin *= GROUND_FRICTION as f32;
                piece.spin_axis = (piece.spin_axis + unit_vector(random)).normalize_or(Vec3::Y);
                let bounces = bounces + 1;
                if piece.drag > 0.0 || rebound < SETTLE_SPEED || bounces >= MAX_BOUNCES {
                    piece.velocity.y = 0.0;
                    piece.stage = Stage::Settling {
                        from: piece.rotation,
                        to: lying(piece.rotation, piece.lean),
                        progress: 0.0,
                    };
                } else {
                    piece.velocity.y = rebound;
                    piece.stage = Stage::Flying { bounces };
                }
            }
            (true, true)
        }
        Stage::Settling { from, to, progress } => {
            slide(piece, step);
            let progress = (progress + dt as f32 / SETTLE_SECONDS).min(1.0);
            // Ease over onto its face.
            let eased = progress * progress * (3.0 - 2.0 * progress);
            piece.rotation = from.slerp(to, eased);
            piece.position.y = CLEARANCE + extent(piece.rotation, piece.half);
            piece.stage = if progress >= 1.0 {
                Stage::Lying { left: piece.rest }
            } else {
                Stage::Settling { from, to, progress }
            };
            (true, true)
        }
        Stage::Lying { left } => {
            let sliding = slide(piece, step);
            let left = left - dt;
            piece.stage = Stage::Lying { left };
            (left > -SHRINK, sliding || left < 0.0)
        }
    }
}

/// Fall under gravity (and air drag), flutter and tumble.
fn fly(piece: &mut Piece, step: f64) {
    let velocity = &mut piece.velocity;
    if piece.drag > 0.0 {
        // Exact linear-drag step toward the terminal fall speed.
        let keep = (-piece.drag * step).exp();
        let terminal = -GRAVITY / piece.drag;
        velocity.x *= keep;
        velocity.z *= keep;
        velocity.y = terminal + (velocity.y - terminal) * keep;
    } else {
        velocity.y -= GRAVITY * step;
    }
    let [speed, rate, phase] = piece.flutter;
    let sway = speed * (piece.age * rate + phase).cos();
    let [fx, fz] = piece.flutter_direction;
    piece.position += DVec3::new(velocity.x + fx * sway, velocity.y, velocity.z + fz * sway) * step;
    let turn = Quat::from_axis_angle(piece.spin_axis, piece.spin * step as f32);
    piece.rotation = (turn * piece.rotation).normalize();
}

/// Slide along the ground to a stop; returns whether it moved.
fn slide(piece: &mut Piece, step: f64) -> bool {
    let velocity = &mut piece.velocity;
    if velocity.x.hypot(velocity.z) < STOPPED {
        velocity.x = 0.0;
        velocity.z = 0.0;
        return false;
    }
    piece.position.x += velocity.x * step;
    piece.position.z += velocity.z * step;
    let keep = (-SLIDE_DRAG * step).exp();
    velocity.x *= keep;
    velocity.z *= keep;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::Point3;

    const DT: f64 = 1.0 / 60.0;

    fn crate_event(kind: SimEventType, from: Option<Point3>) -> SimEvent {
        SimEvent {
            cover_kind: Some(CoverKind::Cargo),
            color: Some(0xb47a49),
            height: Some(if kind == SimEventType::Destroy {
                2.4
            } else {
                1.1
            }),
            from,
            ..SimEvent::at(kind, 2.0, -1.0)
        }
    }

    fn run(chips: &mut WoodChips, random: &mut CosmeticRandom, seconds: f64) {
        for _ in 0..(seconds / DT).round() as usize {
            chips.update(DT, random);
        }
    }

    #[test]
    fn meshes_face_outward_and_fit_their_extents() {
        let mesh = splinter_mesh();
        let centroid = mesh
            .positions
            .iter()
            .fold(Vec3::ZERO, |sum, p| sum + Vec3::from(*p))
            / mesh.positions.len() as f32;
        for triangle in mesh.positions.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(triangle[i]));
            let normal = (b - a).cross(c - a);
            assert!(normal.length() > 1e-4, "degenerate triangle");
            assert!(
                normal.dot((a + b + c) / 3.0 - centroid) > 0.0,
                "inward face"
            );
        }
        for mesh in [splinter_mesh(), straw_mesh()] {
            assert!(mesh.positions.iter().flatten().all(|v| v.abs() <= 0.5));
        }
        let reach = (STRAW_CURL + STRAW_HALF_WIDTH) as f32 + 1e-6;
        assert!(
            straw_mesh()
                .positions
                .iter()
                .all(|p| p[1].abs() <= reach && p[2].abs() <= reach)
        );
    }

    #[test]
    fn only_crate_hits_and_bursts_throw_wood() {
        let mut random = CosmeticRandom::seeded(5);
        let mut chips = WoodChips::default();
        for cover in [CoverKind::Timber, CoverKind::Tree, CoverKind::Drum] {
            let mut event = crate_event(SimEventType::Destroy, None);
            event.cover_kind = Some(cover);
            chips.event(&event, &mut random);
        }
        chips.event(&crate_event(SimEventType::Hurt, None), &mut random);
        assert!(chips.is_empty());
        chips.event(&crate_event(SimEventType::Impact, None), &mut random);
        let hit = chips.len();
        assert!((HIT_PIECES[0]..HIT_PIECES[0] + HIT_PIECES[1]).contains(&hit));
        assert_eq!(chips.straw().count(), 0, "a hit spills no packing");
        chips.event(&crate_event(SimEventType::Destroy, None), &mut random);
        let wood = chips.wood().count() - hit;
        assert!((BURST_PIECES[0]..BURST_PIECES[0] + BURST_PIECES[1]).contains(&wood));
        let straw = chips.straw().count();
        let most = STRAW_TUFTS * (STRAW_FIBRES[0] + STRAW_FIBRES[1] - 1);
        assert!((STRAW_TUFTS * STRAW_FIBRES[0]..=most).contains(&straw));
    }

    #[test]
    fn hit_splinters_burst_back_toward_the_gun() {
        let mut random = CosmeticRandom::seeded(11);
        let mut chips = WoodChips::default();
        // The shell came from -x, so the struck face looks toward -x.
        let from = Point3::new(1.0, 1.1, -1.0);
        chips.event(&crate_event(SimEventType::Impact, Some(from)), &mut random);
        let toward = chips.wood().filter(|piece| piece.velocity.x < 0.0).count();
        assert!(
            toward * 10 >= chips.len() * 7,
            "{toward} of {}",
            chips.len()
        );
        assert!(chips.wood().all(|piece| piece.velocity.y > 0.0));
        run(&mut chips, &mut random, 2.0);
        let landed = chips.wood().filter(|piece| piece.position.x < 2.0).count();
        assert!(
            landed * 10 >= chips.len() * 7,
            "they land at the face's foot"
        );
    }

    #[test]
    fn bursts_fly_on_along_the_killing_shot() {
        let mut random = CosmeticRandom::seeded(13);
        let mut chips = WoodChips::default();
        // A metre back toward +z: the shell flew toward -z.
        let from = Point3::new(2.0, 1.2, 0.0);
        chips.event(&crate_event(SimEventType::Destroy, Some(from)), &mut random);
        let on = chips.wood().filter(|piece| piece.velocity.z < -2.0).count();
        let back = chips.wood().filter(|piece| piece.velocity.z > 2.0).count();
        assert!(on > 3 * back, "{on} fly on, {back} back");
        assert!(
            chips.wood().any(|piece| piece.velocity.z > 0.0),
            "some spall flies back out of the struck face"
        );
    }

    #[test]
    fn pieces_bounce_settle_flat_above_the_ground_and_go() {
        let mut random = CosmeticRandom::seeded(17);
        let mut chips = WoodChips::default();
        let from = Point3::new(2.0, 1.2, 0.0);
        chips.event(&crate_event(SimEventType::Destroy, Some(from)), &mut random);
        chips.event(&crate_event(SimEventType::Impact, Some(from)), &mut random);
        let count = chips.len();
        let mut lowest = f32::MAX;
        for _ in 0..(3.4 / DT) as usize {
            chips.update(DT, &mut random);
            for record in chips.wood_records.records() {
                lowest = lowest.min(record.translation().y);
            }
        }
        assert!(lowest > 0.0, "no piece sinks below the ground in flight");
        assert_eq!(chips.len(), count, "nothing is gone before it rests");
        assert!(chips.wood().chain(chips.straw()).all(Piece::landed));
        for piece in chips.wood().chain(chips.straw()) {
            // Flat on its face (up to its lean), resting on its half thickness.
            let up = piece.rotation * Vec3::Y;
            assert!(up.y.abs() > (MAX_LEAN as f32).cos() - 1e-3);
            let rest = CLEARANCE + extent(piece.rotation, piece.half);
            assert!((piece.position.y - rest).abs() < 1e-6);
            assert!(piece.position.y < 0.2);
        }
        assert_eq!(chips.wood_records.len(), chips.wood().count());
        assert_eq!(chips.straw_records.len(), chips.straw().count());
        run(
            &mut chips,
            &mut random,
            WOOD_REST.iter().sum::<f64>() + SHRINK,
        );
        assert_eq!(chips.wood().count(), 0);
        run(&mut chips, &mut random, STRAW_REST.iter().sum::<f64>());
        assert!(chips.is_empty());
        assert!(chips.wood_records.is_empty() && chips.straw_records.is_empty());
    }

    #[test]
    fn pieces_lying_still_upload_nothing() {
        let mut random = CosmeticRandom::seeded(23);
        let mut chips = WoodChips::default();
        let from = Point3::new(1.0, 1.1, -1.0);
        chips.event(&crate_event(SimEventType::Impact, Some(from)), &mut random);
        run(&mut chips, &mut random, 2.6);
        assert!(
            chips
                .wood()
                .all(|piece| piece.landed() && piece.velocity == DVec3::ZERO)
        );
        chips.wood_records.take_dirty(|_, _| {});
        chips.update(DT, &mut random);
        assert!(chips.wood_records.dirty().is_empty());
        assert_eq!(chips.wood_records.len(), chips.len());
    }

    #[test]
    fn pools_stay_bounded_and_reset_clears_them() {
        let mut random = CosmeticRandom::seeded(19);
        let mut chips = WoodChips::default();
        for i in 0..40 {
            let kind = if i % 2 == 0 {
                SimEventType::Destroy
            } else {
                SimEventType::Impact
            };
            chips.event(&crate_event(kind, None), &mut random);
            chips.update(DT, &mut random);
        }
        assert_eq!(chips.wood().count(), MAX_WOOD_CHIPS);
        assert_eq!(chips.straw().count(), MAX_STRAW);
        assert_eq!(chips.wood_records.len(), MAX_WOOD_CHIPS);
        assert!(chips.wood_records.records().iter().all(|r| {
            r.world_rows.as_flattened().iter().all(|v| v.is_finite()) && r.translation().y > 0.0
        }));
        chips.reset();
        assert!(chips.is_empty());
        assert!(chips.wood_records.is_empty() && chips.straw_records.is_empty());
        chips.update(DT, &mut random);
        assert!(chips.wood_records.is_empty());
    }
}
