//! The village cottage's plan, shared by the simulation and its model: the design a
//! cottage is built to (roof form, finishes and paints, chosen from where it stands
//! and turned to front its street), the measurements of its walls and roof, the
//! pieces its collapse breaks it into and the rubble heap it leaves behind.
//!
//! A cottage is built in its own frame: ridge along z, gable walls (or hip ends) at
//! ±z, eave walls at ±x, ground at y = 0. Its cover box (`w` along world x, `d` along
//! world z, `h` to the ridge) is the collision footprint; whole quarter turns carry
//! the frame into the world, swapping the box's width and depth on odd turns.
//!
//! The pieces are measured here, in the build frame, so the collider a piece falls
//! with matches the part of the model it is drawn as.

use std::f64::consts::PI;

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

use super::house_fracture::{Crack, EdgeKind, PartPlane, ShardRegion, Side};
use super::math::{Random, js_round};

/// Hit points of a cottage: fifteen standard shells, more than three watchtowers,
/// so levelling one is a deliberate effort rather than the work of stray fire (in
/// seeded bot battles every cottage stands after a minute and most after five).
pub const HOUSE_HEALTH: f64 = 600.0;
/// Health fractions below which a cottage shows heavier damage: broken windows and
/// more holes, then a sagging, holed roof and a toppled chimney.
const DAMAGED: f64 = 0.7;
const WRECKED: f64 = 0.35;

/// Height of the fieldstone plinth (two 21 cm steps up to the door).
pub const PLINTH: f64 = 0.42;
/// Horizontal roof overhang past the eave walls and past the gable walls.
pub const EAVE_OVERHANG: f64 = 0.4;
pub const RAKE_OVERHANG: f64 = 0.3;
/// Roof deck thickness, measured square to the slope.
pub const ROOF_THICKNESS: f64 = 0.1;
/// How far a wall piece reaches in behind its outside face.
pub const WALL_DEPTH: f64 = 0.2;
/// Chimney: plan size of the stack, height of the brick above the ridge, and the
/// pot's mouth above the ridge, where the smoke leaves.
pub const CHIMNEY: f64 = 0.62;
pub const CHIMNEY_TOP: f64 = 0.3;
pub const POT_MOUTH: f64 = 0.7;
/// Clearance between neighbouring pieces' colliders.
const GAP: f64 = 0.01;
/// The collapse's rubble mound: how far its foot stays in from the walls' outside
/// faces, how far further in its crown starts, and the crown's height. Its slopes
/// are gentle enough (about 35 degrees) for a shard to rest on them.
pub const MOUND_INSET: f64 = 0.45;
pub const MOUND_SLOPE_RUN: f64 = 1.45;
pub const MOUND_HEIGHT: f64 = 1.0;
/// The least half width of the mound's crown.
const MOUND_CROWN: f64 = 0.3;

/// The rubble a collapsed cottage leaves: one heap along its back wall, inset from
/// the walls, low enough to shoot over from a hull but high enough to hide one. The
/// rest of the footprint opens for driving.
pub struct HouseHeap {
    pub inset: f64,
    pub depth: f64,
    pub height: f64,
}

pub const HOUSE_HEAP: HouseHeap = HouseHeap {
    inset: 0.2,
    depth: 1.6,
    height: 1.15,
};

/// Whether a rubble cover is a cottage's heap rather than a watchtower footing: the
/// heaps are the only rubble this high.
pub fn is_house_heap(h: f64) -> bool {
    (h - HOUSE_HEAP.height).abs() < 1e-6
}

/// Damage look of a standing cottage: 0 intact, 1 damaged, 2 wrecked.
pub fn house_damage_stage(hp: f64, max_hp: f64) -> u32 {
    if hp > max_hp * DAMAGED {
        0
    } else if hp > max_hp * WRECKED {
        1
    } else {
        2
    }
}

/// Quarter turns about +y that put a cottage's door toward its street: the row
/// cottages by the outer roads face them, the corner ones face into their block, the
/// end-of-street pair face the green and the village-square houses face the square.
pub fn cottage_turns(x: f64, z: f64) -> i32 {
    if x.abs() > 40.0 && z.abs() < 26.0 {
        // Door toward the outer road at x = ±52.
        if x > 0.0 { 1 } else { -1 }
    } else if z.abs() > 30.0 {
        // Door toward the cross road nearer the middle.
        if z > 0.0 { 2 } else { 0 }
    } else if x > 0.0 {
        -1
    } else {
        1
    }
}

/// The world direction a cottage's door faces (`turns` from [`cottage_turns`]).
pub fn door_direction(turns: i32) -> DVec2 {
    let yaw = f64::from(turns) * PI / 2.0;
    DVec2::new(yaw.sin(), yaw.cos())
}

/// Stable per-cottage variation from its position.
pub fn house_variant(x: f64, z: f64) -> u32 {
    (js_round(x * 7.0).abs() as u32)
        .wrapping_mul(31)
        .wrapping_add(js_round(z * 13.0).abs() as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoofForm {
    /// Two slopes meeting at a ridge, closed by gable walls.
    Gable,
    /// Four slopes: hips at both ends of a shorter ridge, no gables.
    Hip,
}

/// The wall a cottage's door is in: a gable end (front-gable) or an eave wall
/// (side-gable, the ridge running along the street).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorWall {
    Gable,
    Eave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoofCover {
    /// Stepped shingle courses.
    Shingles,
    /// Painted standing-seam steel.
    Seams,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shutters {
    None,
    /// Louvred shutters with angled slats.
    Louvred,
    /// Plank shutters on a Z brace.
    Board,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallFinish {
    /// Painted clapboard over corner boards.
    Clapboard,
    /// Red brick laid in bond.
    Brick,
}

/// One of the village's house types: form, finishes and paints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseStyle {
    pub form: RoofForm,
    /// Gable roofs only; a hip roof turns its ridge along the longer side.
    pub door: DoorWall,
    pub cover: RoofCover,
    pub walls: WallFinish,
    /// Wall paint (a tint over brick).
    pub siding: u32,
    pub trim: u32,
    /// Door and shutter paint.
    pub accent: u32,
    pub roof: u32,
    /// Dormer plan on a gable roof: 0 none, 1 a pair on one slope, 2 one each side,
    /// 3 one on the back slope.
    pub dormers: u32,
    /// A cross gable over the door of a side-gable cottage: its wall rises past the
    /// eave into a gable of its own whose roof runs back into the main slope, so
    /// the roof reads as a T from above.
    pub wing: bool,
    pub shutters: Shutters,
    /// A front-gable cottage's door wall carries a full shed porch roof on
    /// brackets instead of the small pedimented canopy.
    pub porch: bool,
    /// Wall top (the eave line) as a fraction of the ridge height: lower walls
    /// under a steep attic roof, higher ones under a shallow one.
    pub eave: f64,
}

#[allow(clippy::too_many_arguments)]
const fn style(
    form: RoofForm,
    door: DoorWall,
    cover: RoofCover,
    walls: WallFinish,
    [siding, trim, accent, roof]: [u32; 4],
    dormers: u32,
    wing: bool,
    shutters: Shutters,
    porch: bool,
    eave: f64,
) -> HouseStyle {
    HouseStyle {
        form,
        door,
        cover,
        walls,
        siding,
        trim,
        accent,
        roof,
        dormers,
        wing,
        shutters,
        porch,
        eave,
    }
}

/// The village's house types, ordered so neighbours in the table differ in form,
/// finish, colour and roof pitch; [`HouseDesign::facing`] picks one per cottage.
/// The paints keep to one weathered village palette held apart from the teams'
/// red and blue and from the grass and dirt: muted cream, sage, tan, ochre, slate
/// and white walls with one accent each and trim a soft off-white that takes its
/// shading, under roofs a clear step darker that carry an earthy colour of their
/// own (moss, slate, cedar, teal, terracotta, ochre, barn-red steel and weathered
/// silver-brown), so from overhead every roof stands off the ground by value and
/// hue, no two neighbours match, and none reads as a team marker. Shingle paints
/// sit over the shingle texture and its weathering, which darken them to about
/// two fifths, so they are painted that much brighter than they read.
#[rustfmt::skip]
pub const HOUSE_STYLES: [HouseStyle; 8] = {
    use DoorWall::{Eave, Gable as Front};
    use RoofCover::{Seams, Shingles};
    use RoofForm::{Gable, Hip};
    use Shutters::{Board, Louvred, None};
    use WallFinish::{Brick, Clapboard};
    [
        // Cream clapboard, bottle-green plank shutters, a steep moss-green shingle
        // roof and dormers.
        style(Gable, Front, Shingles, Clapboard, [0xe8dcc0, 0xe6e0d4, 0x2f5040, 0x82996a], 1, false, Board, false, 0.6),
        // Soft red brick under a low slate-blue hip roof, navy door and louvres.
        style(Hip, Front, Shingles, Brick, [0xd9d0c6, 0xdfd7c6, 0x24385a, 0x8f98a2], 0, false, Louvred, false, 0.72),
        // Sage side-gable with an oxblood door under a cross gable and weathered
        // cedar shingles, a dormer lighting the back of the attic.
        style(Gable, Eave, Shingles, Clapboard, [0xa3ae8e, 0xe3dbc8, 0x6a2a24, 0xc48b60], 3, true, Board, false, 0.6),
        // Weathered tan front gable with a shed porch over the door, under
        // weathered teal standing-seam steel.
        style(Gable, Front, Seams, Clapboard, [0xc4a882, 0xe2dacb, 0x2e2c2a, 0x4a706a], 0, false, None, true, 0.68),
        // Ochre hip cottage under faded terracotta shingles.
        style(Hip, Front, Shingles, Clapboard, [0xd2b26e, 0xe6e0d4, 0x5c3c22, 0xb87a62], 0, false, Board, false, 0.7),
        // Slate-grey side-gable with black louvres and a cross gable under
        // lichen-ochre shingles.
        style(Gable, Eave, Shingles, Clapboard, [0x9ea9b2, 0xe6e0d4, 0x232a28, 0xae9a6e], 0, true, Louvred, false, 0.62),
        // Chalk-white front gable with a porch, under weathered barn-red
        // standing-seam steel and black shutters.
        style(Gable, Front, Seams, Clapboard, [0xe6e2d8, 0xeae6dd, 0x1d2622, 0x7a4c40], 2, false, Board, true, 0.6),
        // Sand clapboard side-gable with silver-brown shingles, green louvres and
        // a cross gable.
        style(Gable, Eave, Shingles, Clapboard, [0xd8c8a2, 0xe5dfd2, 0x2f4b3a, 0xb09c8a], 0, true, Louvred, false, 0.64),
    ]
};

/// Roof measurements shared by the deck, shingles, gutters, chimney and the
/// collapse pieces, in the build frame.
#[derive(Clone, Copy, Debug)]
pub struct Roof {
    pub form: RoofForm,
    /// Wall top (the eave line on the walls) and ridge height under the deck.
    pub wall: f64,
    pub ridge: f64,
    /// Slope rise per metre across.
    pub pitch: f64,
    /// Half span of the eaves along x.
    pub eave_x: f64,
    /// Half span along z: to the rakes of a gable, to the end eaves of a hip.
    pub rake_z: f64,
    /// Underside height at the eave edge and vertical deck thickness.
    pub eave_y: f64,
    pub thick: f64,
    /// Half length of the ridge: the whole roof on a gable, shortened by the hips.
    pub ridge_half: f64,
}

impl Roof {
    pub fn new(form: RoofForm, w: f64, d: f64, h: f64, eave: f64) -> Self {
        let wall = h * eave;
        let pitch = (h - wall) / (w / 2.0);
        let eave_x = w / 2.0 + EAVE_OVERHANG;
        let (rake_z, ridge_half) = match form {
            RoofForm::Gable => (d / 2.0 + RAKE_OVERHANG, d / 2.0 + RAKE_OVERHANG),
            // Hips take the side slopes' pitch, so the ridge is as much shorter than
            // the roof as the roof is wide.
            RoofForm::Hip => {
                let end = d / 2.0 + EAVE_OVERHANG;
                (end, (end - eave_x).max(0.0))
            }
        };
        Self {
            form,
            wall,
            ridge: h,
            pitch,
            eave_x,
            rake_z,
            eave_y: wall - pitch * EAVE_OVERHANG,
            thick: ROOF_THICKNESS * (1.0 + pitch * pitch).sqrt(),
            ridge_half,
        }
    }

    /// Horizontal run from the ridge line to `(x, z)` down the slope it lies on.
    pub fn run(&self, x: f64, z: f64) -> f64 {
        match self.form {
            RoofForm::Gable => x.abs(),
            RoofForm::Hip => x.abs().max(z.abs() - self.ridge_half),
        }
    }

    /// Height of the deck's top surface at `(x, z)`.
    pub fn top(&self, x: f64, z: f64) -> f64 {
        self.ridge + self.thick - self.pitch * self.run(x, z)
    }
}

/// The parts a cottage is built of, in the build frame: front is +z (the gable or
/// hip end the ridge points at), right is +x. Hip roofs add the two end slopes; the
/// plinth stays as the heap's footing. A collapse breaks each wall and side slope
/// into shards ([`HousePiece`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HousePart {
    FrontWall,
    BackWall,
    RightWall,
    LeftWall,
    RightRoof,
    LeftRoof,
    FrontRoof,
    BackRoof,
    Chimney,
    /// The rubble the walls and floors crush into, heaped in the footprint: the
    /// pile the falling walls and roof come to rest on. It never moves, and only
    /// the cottage's own shards rest on it.
    Mound,
}

impl HousePart {
    pub const ALL: [Self; 10] = [
        Self::FrontWall,
        Self::BackWall,
        Self::RightWall,
        Self::LeftWall,
        Self::RightRoof,
        Self::LeftRoof,
        Self::FrontRoof,
        Self::BackRoof,
        Self::Chimney,
        Self::Mound,
    ];

    /// The piece of the wall at `index` in [`wall_frames`] order.
    pub fn wall(index: usize) -> Self {
        [
            Self::FrontWall,
            Self::BackWall,
            Self::RightWall,
            Self::LeftWall,
        ][index]
    }

    /// The roof piece over the slope at `(x, z)` in the build frame.
    pub fn roof(roof: &Roof, x: f64, z: f64) -> Self {
        if roof.form == RoofForm::Hip && z.abs() - roof.ridge_half > x.abs() {
            if z > 0.0 {
                Self::FrontRoof
            } else {
                Self::BackRoof
            }
        } else if x >= 0.0 {
            Self::RightRoof
        } else {
            Self::LeftRoof
        }
    }

    pub fn is_roof(self) -> bool {
        matches!(
            self,
            Self::RightRoof | Self::LeftRoof | Self::FrontRoof | Self::BackRoof
        )
    }

    /// The wall's index in [`wall_frames`] order.
    pub fn wall_index(self) -> Option<usize> {
        match self {
            Self::FrontWall => Some(0),
            Self::BackWall => Some(1),
            Self::RightWall => Some(2),
            Self::LeftWall => Some(3),
            _ => None,
        }
    }

    /// The most shards a collapse breaks the part into: a wall's two or three
    /// strips each in a lower and an upper shard, a slope's two strips each at the
    /// ridge (or the hip's peak) and at the eave, and the chimney's stack and top.
    pub const fn max_shards(self) -> u8 {
        match self {
            Self::FrontWall | Self::BackWall | Self::RightWall | Self::LeftWall => 6,
            Self::RightRoof | Self::LeftRoof | Self::FrontRoof | Self::BackRoof => 4,
            Self::Chimney => 2,
            Self::Mound => 1,
        }
    }
}

/// One shard a collapsing cottage breaks into: a part and which of its shards,
/// counted along the part. It is named like its part with the shard's number
/// (`front-wall-1`), the chimney plainly, as the scene codec names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct HousePiece {
    pub part: HousePart,
    pub shard: u8,
}

impl From<HousePiece> for String {
    fn from(piece: HousePiece) -> Self {
        let part = serde_json::to_value(piece.part)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        if piece.part.max_shards() == 1 {
            part
        } else {
            format!("{part}-{}", piece.shard)
        }
    }
}

impl TryFrom<String> for HousePiece {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|piece| String::from(*piece) == name)
            .ok_or_else(|| format!("unknown house piece {name}"))
    }
}

impl HousePiece {
    pub const fn new(part: HousePart, shard: u8) -> Self {
        Self { part, shard }
    }

    /// Every shard any cottage can break into, part by part: a wall's strip `k`
    /// gives shards `2k` (below its split) and `2k + 1` (above it), a slope's strip
    /// `k` shards `2k` (at the ridge or peak) and `2k + 1` (at the eave).
    pub const ALL: [Self; 43] = {
        let mut all = [Self::new(HousePart::FrontWall, 0); 43];
        let mut next = 0;
        let mut part = 0;
        while part < HousePart::ALL.len() {
            let mut shard = 0;
            while shard < HousePart::ALL[part].max_shards() {
                all[next] = Self::new(HousePart::ALL[part], shard);
                next += 1;
                shard += 1;
            }
            part += 1;
        }
        assert!(next == all.len(), "every part's shards");
        all
    };
}

/// A collapse shard: where it lies in its part (the part's plane, its outline there
/// and the shard's jagged region) and its collider, hull points in the build frame
/// and their bounds. The collider is the shard's straight-edged core, so
/// neighbouring shards start clear of each other however their cracks wander.
#[derive(Clone, Debug)]
pub struct PieceShape {
    pub piece: HousePiece,
    pub plane: PartPlane,
    /// The part's outline in its plane (convex), and the shard's share of it.
    pub outline: Vec<DVec2>,
    pub region: ShardRegion,
    pub hull: Vec<DVec3>,
    pub center: DVec3,
    pub size: DVec3,
    /// A side slope's eave slab's whole reach out to its eave (hull points in
    /// the build frame), which it rests on once the roof has given way, propped
    /// on the stubs of wall: none on other shards. While the roof hangs over the
    /// buckling walls it stays out of the way of the walls folding in under it.
    pub overhang: Vec<DVec3>,
}

impl PieceShape {
    fn new(
        piece: HousePiece,
        plane: PartPlane,
        outline: Vec<DVec2>,
        region: ShardRegion,
        hull: Vec<DVec3>,
    ) -> Self {
        let min = hull
            .iter()
            .copied()
            .reduce(DVec3::min)
            .expect("hull points");
        let max = hull
            .iter()
            .copied()
            .reduce(DVec3::max)
            .expect("hull points");
        Self {
            piece,
            plane,
            outline,
            region,
            hull,
            center: (min + max) / 2.0,
            size: max - min,
            overhang: Vec::new(),
        }
    }
}

/// Fractions of a wall's height (plinth to wall top) where its cracks change
/// course, and of a slope's run (ridge to eave) where a roof crack does.
const WALL_KNOTS: [f64; 3] = [0.3, 0.55, 0.8];
const ROOF_KNOTS: [f64; 3] = [0.3, 0.6, 0.85];
/// How far (m) a crack wanders either side of its line at each knot.
const WALL_WANDER: f64 = 0.24;
const ROOF_WANDER: f64 = 0.32;
/// Walls this long or longer break into three shards, shorter ones into two.
const LONG_WALL: f64 = 6.5;
/// Where the back wall breaks off above the stub it leaves standing in the heap:
/// between these heights above the plinth.
const STUB: (f64, f64) = (0.3, 0.72);
/// Clearance between neighbouring shards' colliders across a crack.
const CRACK_GAP: f64 = 0.02;
/// Where a wall's upper shards break off its lower ones: the split's ends lie
/// between these fractions of the wall's height above the plinth, and it bends
/// once, at a knee up to this far (m) off the straight line, so the break reads
/// as torn rather than sawn.
const WALL_SPLIT: (f64, f64) = (0.44, 0.58);
const SPLIT_KNEE: f64 = 0.2;
/// Where a side slope's ridge slabs break off its eave slabs, along a rafter
/// line: between these fractions of the run from the ridge to the eave.
const ROOF_SPLIT: (f64, f64) = (0.42, 0.56);
/// Where the chimney stack snaps, as a fraction of its height above the roof,
/// and how far (m) below the roof its stump's broken foot still reaches.
const CHIMNEY_SNAP: (f64, f64) = (0.4, 0.6);
const CHIMNEY_FOOT: f64 = 0.2;
/// Far past any part, for half-planes that must cover it.
const FAR: f64 = 1.0e3;
/// Rafters hang this far under a roof deck; a slab's collider takes them in only
/// inside the wall line, clear of the wall tops.
pub const RAFTER_DEPTH: f64 = 0.16;
const RAFTER_INSET: f64 = 0.45;

/// Widest half width (m) of a cross gable, and how far its ridge stays under the
/// main ridge.
const WING_HALF: f64 = 1.3;
const WING_BELOW_RIDGE: f64 = 0.32;
/// How far a cross gable's roof overhangs its face and its side walls.
pub const WING_OVERHANG: f64 = 0.25;

/// A side-gable cottage's cross gable on its right (+x) slope, in the build frame:
/// its middle along the ridge and half width, its face's plane (x), the height its
/// face starts at, the face's wall height below its gable and the gable's rise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wing {
    pub z: f64,
    pub half: f64,
    pub face: f64,
    pub base: f64,
    pub wall: f64,
    pub peak: f64,
}

impl Wing {
    /// Height of its ridge, under the roof deck's top.
    pub fn crest(&self) -> f64 {
        self.base + self.wall + self.peak
    }

    /// How far in from its face its ridge runs before it meets the main slope.
    pub fn ridge_end(&self, roof: &Roof) -> f64 {
        ((roof.ridge + roof.thick - self.crest()) / roof.pitch).max(0.05)
    }

    /// Hull points of the gable over a roof slab's collider.
    fn hull(&self, roof: &Roof) -> Vec<DVec3> {
        let front = self.face + WING_OVERHANG;
        let back = self.ridge_end(roof);
        let eave = self.base + self.wall;
        let reach = self.half + WING_OVERHANG;
        vec![
            DVec3::new(front, self.crest(), self.z),
            DVec3::new(back, self.crest(), self.z),
            DVec3::new(front, eave, self.z - reach),
            DVec3::new(front, eave, self.z + reach),
            DVec3::new(self.face, self.base, self.z - self.half),
            DVec3::new(self.face, self.base, self.z + self.half),
        ]
    }
}

/// The walls as (outside-face centre, outward normal, along-wall direction, half
/// length): front (+z), back, right (+x) and left, the order the model builds them.
/// The along direction is the wall frame's +x seen from outside.
pub fn wall_frames(w: f64, d: f64) -> [(DVec3, DVec3, DVec3, f64); 4] {
    [
        (DVec3::new(0.0, 0.0, d / 2.0), DVec3::Z, DVec3::X, w / 2.0),
        (
            DVec3::new(0.0, 0.0, -d / 2.0),
            -DVec3::Z,
            -DVec3::X,
            w / 2.0,
        ),
        (DVec3::new(w / 2.0, 0.0, 0.0), DVec3::X, -DVec3::Z, d / 2.0),
        (DVec3::new(-w / 2.0, 0.0, 0.0), -DVec3::X, DVec3::Z, d / 2.0),
    ]
}

/// How one cottage is built: its type, the quarter turns from the build frame to
/// the world and its built measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseDesign {
    pub style: HouseStyle,
    /// Quarter turns about +y from the build frame to the world.
    pub turns: i32,
    /// Built width (x), depth (z, along the ridge) and ridge height.
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub variant: u32,
    /// The wall with the door, in [`wall_frames`] order (0 or 2).
    pub door_wall: usize,
    /// A TV aerial strapped to the chimney.
    pub antenna: bool,
}

impl HouseDesign {
    /// The cottage a village cover box at `x, z` stands for, fronting its street.
    pub fn of(x: f64, z: f64, w: f64, d: f64, h: f64) -> Self {
        Self::facing(x, z, w, d, h, cottage_turns(x, z))
    }

    /// A cottage whose door faces `street` quarter turns (see [`door_direction`]);
    /// scenery outside the arena chooses its own.
    pub fn facing(x: f64, z: f64, w: f64, d: f64, h: f64, street: i32) -> Self {
        let variant = house_variant(x, z);
        // Mirror-image cottages take different types, so the symmetric plan does
        // not read as copies.
        let quadrant = u32::from(x > 0.0) * 2 + u32::from(z > 0.0);
        let pick = (variant / 2).wrapping_add(quadrant * 2 + 3) as usize;
        let style = HOUSE_STYLES[pick % HOUSE_STYLES.len()];
        let built = |turns: i32| {
            if turns.rem_euclid(2) == 0 {
                (w, d)
            } else {
                (d, w)
            }
        };
        let door_wall = match (style.form, style.door) {
            (RoofForm::Gable, DoorWall::Gable) => 0,
            (RoofForm::Gable, DoorWall::Eave) => 2,
            // A hip ridge runs along the longer side, whichever wall the door is in.
            (RoofForm::Hip, _) => {
                let (bw, bd) = built(street);
                if bd >= bw { 0 } else { 2 }
            }
        };
        // A door in the right (+x) wall faces the street a quarter turn earlier.
        let turns = if door_wall == 2 { street - 1 } else { street };
        let (bw, bd) = built(turns);
        Self {
            style,
            turns,
            w: bw,
            d: bd,
            h,
            variant,
            door_wall,
            antenna: (variant / 5).is_multiple_of(3),
        }
    }

    pub fn yaw(&self) -> f64 {
        f64::from(self.turns) * PI / 2.0
    }

    pub fn roof(&self) -> Roof {
        Roof::new(self.style.form, self.w, self.d, self.h, self.style.eave)
    }

    /// A build-frame offset carried into the world (about the cottage's centre).
    pub fn to_world(&self, local: DVec3) -> DVec3 {
        let (sin, cos) = self.yaw().sin_cos();
        DVec3::new(
            local.x * cos + local.z * sin,
            local.y,
            -local.x * sin + local.z * cos,
        )
    }

    /// A world offset from the cottage's centre in the build frame.
    pub fn to_local(&self, world: DVec3) -> DVec3 {
        let (sin, cos) = self.yaw().sin_cos();
        DVec3::new(
            world.x * cos - world.z * sin,
            world.y,
            world.x * sin + world.z * cos,
        )
    }

    /// The chimney pot's mouth in the build frame. It keeps its world place
    /// ([`chimney_flue`]) however the cottage turns, where the village's smoke rises.
    pub fn flue(&self) -> DVec3 {
        let (world_w, world_d) = if self.turns.rem_euclid(2) == 0 {
            (self.w, self.d)
        } else {
            (self.d, self.w)
        };
        self.to_local(chimney_flue(world_w, world_d, self.h))
    }

    /// The chimney stack's bottom (below where it meets the roof) and its top (under
    /// the corbel), in the build frame.
    pub fn chimney_span(&self) -> (f64, f64) {
        let roof = self.roof();
        let flue = self.flue();
        let low = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(sx, sz)| roof.top(flue.x + sx * CHIMNEY / 2.0, flue.z + sz * CHIMNEY / 2.0))
            .into_iter()
            .fold(f64::INFINITY, f64::min);
        (low - roof.thick * 2.0, roof.ridge + CHIMNEY_TOP)
    }

    /// Where a gable roof's dormers sit: (slope side, z along the ridge), for its
    /// plan (0 none, 1 a pair on one slope, 2 one on each slope).
    pub fn dormers(&self) -> Vec<(f64, f64)> {
        let roof = self.roof();
        if roof.form != RoofForm::Gable || roof.ridge - roof.wall < 1.3 {
            return Vec::new();
        }
        let d = self.d;
        match self.style.dormers {
            1 => vec![(1.0, -d * 0.2), (1.0, d * 0.2)],
            2 => vec![(1.0, d * 0.08), (-1.0, -d * 0.08)],
            3 => vec![(-1.0, 0.0)],
            _ => Vec::new(),
        }
    }

    /// A side-gable cottage's cross gable over its door: (z along the ridge, half
    /// width), on the right (+x) slope over the door wall. It centres on the door
    /// unless the chimney stands there, when it moves along clear of the stack.
    pub fn wing(&self) -> Option<Wing> {
        let roof = self.roof();
        if !self.style.wing || roof.form != RoofForm::Gable || self.door_wall != 2 {
            return None;
        }
        let half = (self.d * 0.21).clamp(1.0, WING_HALF);
        let flue = self.flue();
        let clear = half + CHIMNEY / 2.0 + 0.25;
        let z = if flue.x > 0.0 && flue.z.abs() < clear {
            flue.z - flue.z.signum() * clear
        } else {
            0.0
        };
        if z.abs() + half > self.d / 2.0 - 0.6 {
            return None;
        }
        let base = roof.wall - 0.02;
        let peak = half * roof.pitch;
        let crest = roof.ridge - WING_BELOW_RIDGE;
        let wall = crest - base - peak;
        (wall > 0.25).then_some(Wing {
            z,
            half,
            face: self.w / 2.0,
            base,
            wall,
            peak,
        })
    }

    /// A part's own crack stream, so each part breaks the same way whatever the
    /// others do.
    fn crack_random(&self, part: HousePart) -> Random {
        let index = HousePart::ALL.iter().position(|&p| p == part).unwrap_or(0);
        Random::new(f64::from(self.variant) * 31.0 + index as f64 * 7.0 + 101.0)
    }

    /// The cracks across the wall at `index` (half length `half`), left to right
    /// along it, and for the back wall the line each shard breaks off along above
    /// the stub it leaves, as (left end, right end) in the wall's plane.
    pub fn wall_breaks(&self, index: usize) -> (Vec<Crack>, Vec<(DVec2, DVec2)>) {
        let roof = self.roof();
        let part = HousePart::wall(index);
        let half = wall_frames(self.w, self.d)[index].3;
        let mut rng = self.crack_random(part);
        let rise = roof.wall - PLINTH;
        let knots = WALL_KNOTS.map(|f| PLINTH + f * rise);
        let shards = if 2.0 * half >= LONG_WALL { 3 } else { 2 };
        let cracks: Vec<Crack> = (1..shards)
            .map(|k| {
                let s = -half
                    + 2.0 * half * f64::from(k) / f64::from(shards)
                    + rng.range(-0.12, 0.12) * half;
                Crack::new(s, &knots, || rng.range(-WALL_WANDER, WALL_WANDER))
            })
            .collect();
        let mut floors = Vec::new();
        if part == HousePart::BackWall {
            let mut from = -half;
            for k in 0..shards as usize {
                let to = cracks.get(k).map_or(half, |crack| crack.points[0].x);
                floors.push((
                    DVec2::new(from, PLINTH + rng.range(STUB.0, STUB.1)),
                    DVec2::new(to, PLINTH + rng.range(STUB.0, STUB.1)),
                ));
                from = to;
            }
        }
        (cracks, floors)
    }

    /// The crack across the right (`side` 1) or left roof slope, splitting it along
    /// the ridge into two slabs: it runs down from the ridge, fanning out toward the
    /// eave of a hip and wandering, clear of the dormers on the slope.
    pub fn roof_cracks(&self, side: f64) -> Vec<Crack> {
        let roof = self.roof();
        let part = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        let mut rng = self.crack_random(part);
        let dormers: Vec<f64> = self
            .dormers()
            .into_iter()
            .filter(|&(s, _)| s == side)
            .map(|(_, z)| z)
            .collect();
        let ridge = roof.ridge_half.max(0.8);
        // Dormers and a cross gable on this slope, as (z, half width).
        let mut keep: Vec<(f64, f64)> = dormers.iter().map(|&z| (z, 0.62)).collect();
        if let Some(wing) = self.wing().filter(|_| side > 0.0) {
            keep.push((wing.z, wing.half + 0.2));
        }
        [rng.range(-0.35, 0.35)]
            .into_iter()
            .map(|offset| {
                let mut z = offset * ridge;
                // Past a dormer or the cross gable rather than through it.
                if let Some(&(d, half)) = keep.iter().find(|(d, half)| (z - d).abs() < half + 0.28)
                {
                    let away = if z >= d { 1.0 } else { -1.0 };
                    z = d + away * (half + 0.33);
                }
                let spread = (roof.rake_z / ridge - 1.0).max(0.0) * 0.6 + 0.15;
                let mut points = vec![DVec2::new(z, -50.0)];
                for f in ROOF_KNOTS {
                    let s = z * (1.0 + spread * f) + rng.range(-ROOF_WANDER, ROOF_WANDER);
                    points.push(DVec2::new(s, f * roof.eave_x));
                }
                let last = points[points.len() - 1].x;
                points.insert(1, DVec2::new(z, 0.0));
                points.push(DVec2::new(last, 50.0));
                Crack { points }
            })
            .collect()
    }

    /// A split's own stream, apart from the cracks', so the cracks keep their course.
    fn split_random(&self, part: HousePart) -> Random {
        let index = HousePart::ALL.iter().position(|&p| p == part).unwrap_or(0);
        Random::new(f64::from(self.variant) * 17.0 + index as f64 * 5.0 + 211.0)
    }

    /// The bent line (left end, knee, right end, in the wall's plane) where the
    /// wall at `index` breaks into lower and upper shards; none on the back wall,
    /// whose shards break off above the stub it leaves.
    pub fn wall_split(&self, index: usize) -> Option<[DVec2; 3]> {
        if index == 1 {
            return None;
        }
        let roof = self.roof();
        let half = wall_frames(self.w, self.d)[index].3;
        let mut rng = self.split_random(HousePart::wall(index));
        let rise = roof.wall - PLINTH;
        let mut height = || PLINTH + rise * rng.range(WALL_SPLIT.0, WALL_SPLIT.1);
        let (left, right) = (
            DVec2::new(-half - 0.5, height()),
            DVec2::new(half + 0.5, height()),
        );
        let s = rng.range(-0.5, 0.5) * half;
        let straight = left.y + (right.y - left.y) * (s - left.x) / (right.x - left.x);
        let knee = DVec2::new(s, straight + rng.range(-SPLIT_KNEE, SPLIT_KNEE));
        Some([left, knee, right])
    }

    /// The rafter line (two points in the slope's plan plane: along the ridge, out
    /// from it) where the right (`side` 1) or left slope's ridge slabs break off its
    /// eave slabs.
    pub fn roof_split(&self, side: f64) -> (DVec2, DVec2) {
        let roof = self.roof();
        let part = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        let mut rng = self.split_random(part);
        let mut run = || roof.eave_x * rng.range(ROOF_SPLIT.0, ROOF_SPLIT.1);
        (
            DVec2::new(-roof.rake_z - 0.5, run()),
            DVec2::new(roof.rake_z + 0.5, run()),
        )
    }

    /// The height where the chimney stack snaps, in the build frame.
    pub fn chimney_snap(&self) -> f64 {
        let (low, top) = self.chimney_collider();
        let mut rng = self.split_random(HousePart::Chimney);
        low + (top - low) * rng.range(CHIMNEY_SNAP.0, CHIMNEY_SNAP.1)
    }

    /// The stack's collider from just clear of the roof it pierced to its top.
    fn chimney_collider(&self) -> (f64, f64) {
        let roof = self.roof();
        let flue = self.flue();
        let (_, top) = self.chimney_span();
        let half = CHIMNEY / 2.0 + 0.05;
        let bottom = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(sx, sz)| roof.top(flue.x + sx * half, flue.z + sz * half))
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
            + 0.1
            + 2.0 * GAP;
        (bottom, top + 0.16)
    }

    /// Every shard the cottage collapses into, with its collider: each wall in two
    /// or three strips, each broken into a lower and an upper shard (the back wall
    /// only above the stub it leaves standing in the heap), each side slope in two
    /// strips broken along a rafter line into ridge and eave slabs, each hip end in
    /// two, and the chimney's stack snapped under its top.
    pub fn pieces(&self) -> Vec<PieceShape> {
        let roof = self.roof();
        let mut pieces = Vec::new();
        for (index, (center, out, along, half)) in
            wall_frames(self.w, self.d).into_iter().enumerate()
        {
            let part = HousePart::wall(index);
            let gable = roof.form == RoofForm::Gable && index < 2;
            // Eave walls stop short of the end walls' inner faces.
            let reach = if index < 2 {
                half
            } else {
                half - WALL_DEPTH - GAP
            };
            let top = roof.wall - GAP;
            let mut outline = vec![
                DVec2::new(-reach, PLINTH),
                DVec2::new(reach, PLINTH),
                DVec2::new(reach, top),
            ];
            if gable {
                outline.push(DVec2::new(0.0, roof.ridge - GAP));
            }
            outline.push(DVec2::new(-reach, top));
            let plane = PartPlane {
                origin: center,
                s: along,
                t: DVec3::Y,
            };
            let (cracks, floors) = self.wall_breaks(index);
            let split = self.wall_split(index);
            for strip in 0..=cracks.len() {
                let (left, right) = (strip.checked_sub(1).map(|k| &cracks[k]), cracks.get(strip));
                let floor = floors.get(strip).map(|&(a, b)| {
                    Side::through(a, b, DVec2::new(a.x, roof.ridge), EdgeKind::Broken)
                });
                let region = ShardRegion::between(left, right, floor);
                // The collider: the shard's straight core between its cracks' extremes.
                let (left_core, right_core) = (
                    left.map(|c| straight(c, true)),
                    right.map(|c| straight(c, false)),
                );
                let span = (
                    left_core.as_ref().map_or(-FAR, |c| c.points[0].x),
                    right_core.as_ref().map_or(FAR, |c| c.points[0].x),
                );
                // Each strip's shards: below and above the split, or (the back
                // wall) the one above its stub.
                let shards: Vec<(u8, ShardRegion, Option<Side>)> = match split {
                    Some(line) => [false, true]
                        .into_iter()
                        .map(|upper| {
                            let (low, high) = split_extent(&line, span);
                            let level = if upper {
                                high + CRACK_GAP
                            } else {
                                low - CRACK_GAP
                            };
                            (
                                u8::from(upper),
                                region.intersect(&split_halves(&line, upper)),
                                Some(level_side(level, upper)),
                            )
                        })
                        .collect(),
                    None => {
                        let level = floors
                            .get(strip)
                            .map(|&(a, b)| level_side(a.y.max(b.y) + CRACK_GAP, true));
                        vec![(1, region, level)]
                    }
                };
                for (level, region, cut) in shards {
                    let core = ShardRegion::between(left_core.as_ref(), right_core.as_ref(), cut);
                    let points = core.outline_points(&outline);
                    if points.len() < 3 {
                        continue;
                    }
                    let hull = points
                        .iter()
                        .flat_map(|q| {
                            let face = center + along * q.x + DVec3::Y * q.y;
                            [face, face - out * WALL_DEPTH]
                        })
                        .collect();
                    pieces.push(PieceShape::new(
                        HousePiece::new(part, strip as u8 * 2 + level),
                        plane,
                        outline.clone(),
                        region,
                        hull,
                    ));
                }
            }
        }
        let (xe, ze, rh) = (roof.eave_x, roof.rake_z, roof.ridge_half);
        // A slab's collider: its deck and shingles inside the walls, and the rafters
        // under it inside the wall line. The eaves and rakes that overhang the walls
        // stay out of it: resting on the wall tops, they would catch each wall shard
        // tipping out from under the roof and hold it up as a post under a slab many
        // times its weight. Clear of the walls, the slab drops between them.
        let (inside_w, inside_d) = (
            self.w / 2.0 - WALL_DEPTH - GAP,
            self.d / 2.0 - WALL_DEPTH - GAP,
        );
        let lift = |x: f64, z: f64, inside: bool| {
            let top = roof.top(x, z);
            let low = top - roof.thick + GAP;
            let low = if inside { low - RAFTER_DEPTH } else { low };
            [DVec3::new(x, top + 0.08, z), DVec3::new(x, low, z)]
        };
        for side in [1.0, -1.0] {
            let part = if side > 0.0 {
                HousePart::RightRoof
            } else {
                HousePart::LeftRoof
            };
            // Plan coordinates: along the ridge, and out from it down the slope.
            let plane = PartPlane {
                origin: DVec3::ZERO,
                s: DVec3::Z,
                t: DVec3::X * side,
            };
            let gutter = xe + 0.17;
            let outline = match roof.form {
                RoofForm::Gable => vec![
                    DVec2::new(-ze, 0.0),
                    DVec2::new(ze, 0.0),
                    DVec2::new(ze, gutter),
                    DVec2::new(-ze, gutter),
                ],
                RoofForm::Hip => vec![
                    DVec2::new(-rh, 0.0),
                    DVec2::new(rh, 0.0),
                    DVec2::new(ze, xe),
                    DVec2::new(ze, gutter),
                    DVec2::new(-ze, gutter),
                    DVec2::new(-ze, xe),
                ],
            };
            let cracks = self.roof_cracks(side);
            let (from, to) = self.roof_split(side);
            let line = [from, to];
            let wall_line = self.w / 2.0 - RAFTER_INSET;
            for strip in 0..=cracks.len() {
                let (left, right) = (strip.checked_sub(1).map(|k| &cracks[k]), cracks.get(strip));
                let (left_core, right_core) = (
                    left.map(|c| straight(c, true)),
                    right.map(|c| straight(c, false)),
                );
                let span = (
                    left_core.as_ref().map_or(-FAR, |c| c.points[0].x),
                    right_core.as_ref().map_or(FAR, |c| c.points[0].x),
                );
                let (near, far) = split_extent(&line, span);
                // The strip's slab at the ridge (level 0), then at the eave.
                for eave in [false, true] {
                    let region = ShardRegion::between(left, right, None)
                        .intersect(&split_halves(&line, eave));
                    let level = if eave {
                        far + CRACK_GAP
                    } else {
                        near - CRACK_GAP
                    };
                    let core = ShardRegion::between(
                        left_core.as_ref(),
                        right_core.as_ref(),
                        Some(level_side(level, eave)),
                    );
                    let points = core.outline_points(&within(&outline, inside_d, inside_w));
                    if points.len() < 3 {
                        continue;
                    }
                    // The rafters' reach, cut off at the wall line.
                    let rafters = core.outline_points(&within(&outline, inside_d, wall_line));
                    let mut hull: Vec<DVec3> = points
                        .iter()
                        .flat_map(|q| lift(side * q.y, q.x, false))
                        .chain(rafters.iter().map(|q| lift(side * q.y, q.x, true)[1]))
                        .collect();
                    // The eave slab carrying the cross gable falls with it on top:
                    // the gable's foot, held inside the wall it fronts and clear of
                    // the ridge slab above the break, joins its collider. Its peak
                    // stays out, or the slab would come to rest on it, upside down.
                    if let Some(wing) = self.wing().filter(|_| side > 0.0 && eave) {
                        let (low, high) = points
                            .iter()
                            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), q| {
                                (l.min(q.x), h.max(q.x))
                            });
                        if (low..high).contains(&wing.z) {
                            let eave_line = wing.base + wing.wall;
                            hull.extend(
                                wing.hull(&roof)
                                    .into_iter()
                                    .filter(|p| p.y <= eave_line + 1e-9)
                                    .map(|p| {
                                        DVec3::new(p.x.clamp(far + CRACK_GAP, inside_w), p.y, p.z)
                                    }),
                            );
                        }
                    }
                    let mut shape = PieceShape::new(
                        HousePiece::new(part, strip as u8 * 2 + u8::from(eave)),
                        plane,
                        outline.clone(),
                        region,
                        hull,
                    );
                    if eave {
                        shape.overhang = core
                            .outline_points(&within(&outline, roof.rake_z, roof.eave_x))
                            .iter()
                            .flat_map(|q| lift(side * q.y, q.x, false))
                            .collect();
                    }
                    pieces.push(shape);
                }
            }
        }
        if roof.form == RoofForm::Hip {
            for end in [1.0, -1.0] {
                let part = if end > 0.0 {
                    HousePart::FrontRoof
                } else {
                    HousePart::BackRoof
                };
                let plane = PartPlane {
                    origin: DVec3::ZERO,
                    s: DVec3::X,
                    t: DVec3::Z * end,
                };
                let outline = vec![
                    DVec2::new(0.0, rh),
                    DVec2::new(xe, ze),
                    DVec2::new(xe, ze + 0.17),
                    DVec2::new(-xe, ze + 0.17),
                    DVec2::new(-xe, ze),
                ];
                // Split from the hip's peak down to the eave.
                let mut rng = self.crack_random(part);
                let foot = rng.range(-0.4, 0.4) * xe;
                let mut points = vec![DVec2::new(0.0, -50.0), DVec2::new(0.0, rh)];
                for f in ROOF_KNOTS {
                    let s = foot * f + rng.range(-ROOF_WANDER, ROOF_WANDER) * f;
                    points.push(DVec2::new(s, rh + f * (ze - rh)));
                }
                let last = points[points.len() - 1].x;
                points.push(DVec2::new(last, 50.0));
                let crack = Crack { points };
                // Broken again along a rafter line across the hip, peak from eave.
                let mut split = self.split_random(part);
                let mut run = || rh + (ze - rh) * split.range(ROOF_SPLIT.0, ROOF_SPLIT.1);
                let line = [DVec2::new(-xe - 0.5, run()), DVec2::new(xe + 0.5, run())];
                for strip in 0..2u8 {
                    let (left, right) = if strip == 0 {
                        (None, Some(&crack))
                    } else {
                        (Some(&crack), None)
                    };
                    let (left_core, right_core) = (
                        left.map(|c| straight(c, true)),
                        right.map(|c| straight(c, false)),
                    );
                    let span = (
                        left_core.as_ref().map_or(-FAR, |c| c.points[0].x),
                        right_core.as_ref().map_or(FAR, |c| c.points[0].x),
                    );
                    let (near, far) = split_extent(&line, span);
                    for eave in [false, true] {
                        let level = if eave {
                            far + CRACK_GAP
                        } else {
                            near - CRACK_GAP
                        };
                        let core = ShardRegion::between(
                            left_core.as_ref(),
                            right_core.as_ref(),
                            Some(level_side(level, eave)),
                        );
                        let hull: Vec<DVec3> = core
                            .outline_points(&within(&outline, inside_w, inside_d))
                            .iter()
                            .flat_map(|q| lift(q.x, end * q.y, false))
                            .collect();
                        if hull.len() < 6 {
                            continue;
                        }
                        pieces.push(PieceShape::new(
                            HousePiece::new(part, strip * 2 + u8::from(eave)),
                            plane,
                            outline.clone(),
                            ShardRegion::between(left, right, None)
                                .intersect(&split_halves(&line, eave)),
                            hull,
                        ));
                    }
                }
            }
        }
        let flue = self.flue();
        let half = CHIMNEY / 2.0 + 0.05;
        let corners = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(sx, sz)| DVec2::new(flue.x + sx * half, flue.z + sz * half));
        // The stack's colliders start clear of the roof it pierced, and part where
        // it snaps: the stump below, the top with its pot above.
        let (bottom, top) = self.chimney_collider();
        let snap = self.chimney_snap();
        // The chimney's plane: across it and up it.
        let plane = PartPlane {
            origin: DVec3::ZERO,
            s: DVec3::X,
            t: DVec3::Y,
        };
        let below = ShardRegion::between(None, None, Some(level_side(snap, false)))
            .intersect(&[vec![level_side(bottom - CHIMNEY_FOOT, true)]]);
        let above = ShardRegion::between(None, None, Some(level_side(snap, true)));
        for (shard, region, (low, high)) in [
            (0, below, (bottom, snap - CRACK_GAP)),
            (1, above, (snap + CRACK_GAP, top)),
        ] {
            let hull = corners
                .into_iter()
                .flat_map(|c| [low, high].map(|y| DVec3::new(c.x, y, c.y)))
                .collect();
            pieces.push(PieceShape::new(
                HousePiece::new(HousePart::Chimney, shard),
                plane,
                Vec::new(),
                region,
                hull,
            ));
        }
        pieces.push(self.mound());
        pieces
    }

    /// The rubble mound the collapse heaps in the footprint ([`HousePart::Mound`]):
    /// a low frustum from its foot inside the walls to its flat crown.
    pub fn mound(&self) -> PieceShape {
        let (foot_x, foot_z) = (self.w / 2.0 - MOUND_INSET, self.d / 2.0 - MOUND_INSET);
        let (crown_x, crown_z) = (
            (foot_x - MOUND_SLOPE_RUN).max(MOUND_CROWN),
            (foot_z - MOUND_SLOPE_RUN).max(MOUND_CROWN),
        );
        let hull = [(foot_x, foot_z, 0.0), (crown_x, crown_z, MOUND_HEIGHT)]
            .into_iter()
            .flat_map(|(x, z, y)| {
                [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
                    .map(|(sx, sz)| DVec3::new(sx * x, y, sz * z))
            })
            .collect();
        PieceShape::new(
            HousePiece::new(HousePart::Mound, 0),
            PartPlane {
                origin: DVec3::ZERO,
                s: DVec3::X,
                t: DVec3::Z,
            },
            Vec::new(),
            ShardRegion::between(None, None, None),
            hull,
        )
    }

    /// The top of the back wall's stub, left in the heap where its shards broke
    /// off: a ragged line in the back wall's plane, from one end to the other.
    pub fn back_stub(&self) -> Vec<DVec2> {
        let (_, floors) = self.wall_breaks(1);
        floors.into_iter().flat_map(|(a, b)| [a, b]).collect()
    }

    /// The heap's centre and size in the build frame.
    pub fn heap(&self) -> (DVec3, DVec3) {
        let HouseHeap {
            inset,
            depth,
            height,
        } = HOUSE_HEAP;
        (
            DVec3::new(0.0, height / 2.0, -self.d / 2.0 + inset + depth / 2.0),
            DVec3::new(self.w - 2.0 * inset, height, depth),
        )
    }

    /// The heap as a world cover box: centre offset from the cottage and its world
    /// width and depth.
    pub fn heap_box(&self) -> (DVec2, DVec2) {
        let (center, size) = self.heap();
        let at = self.to_world(center);
        let size = if self.turns.rem_euclid(2) == 0 {
            DVec2::new(size.x, size.z)
        } else {
            DVec2::new(size.z, size.x)
        };
        (DVec2::new(at.x, at.z), size)
    }

    /// How far the open floor reaches in front of the heap, to the front wall.
    pub fn open_floor(&self) -> f64 {
        self.d - HOUSE_HEAP.inset - HOUSE_HEAP.depth
    }
}

/// The part of a roof slope's plan `outline` (convex; `s` along the slope from
/// its middle, `t` out from the ridge) within `along` of the middle and `across`
/// of the ridge.
fn within(outline: &[DVec2], along: f64, across: f64) -> Vec<DVec2> {
    let side = |a: DVec2, b: DVec2| Side::through(a, b, DVec2::ZERO, EdgeKind::Band);
    let inside = ShardRegion {
        cells: vec![vec![
            side(DVec2::new(-along, 0.0), DVec2::new(-along, 1.0)),
            side(DVec2::new(along, 0.0), DVec2::new(along, 1.0)),
            side(DVec2::new(0.0, across), DVec2::new(1.0, across)),
        ]],
    };
    inside.outline_points(outline)
}

/// The half-plane `t >= level` (`above`) or `t <= level` of a part's plane, cut
/// along a broken line.
fn level_side(level: f64, above: bool) -> Side {
    Side::through(
        DVec2::new(0.0, level),
        DVec2::new(1.0, level),
        DVec2::new(0.0, if above { FAR } else { -FAR }),
        EdgeKind::Broken,
    )
}

/// The lowest and highest `t` a split line (points with `s` increasing) reaches
/// over the run of `s` in `span`.
fn split_extent(line: &[DVec2], (from, to): (f64, f64)) -> (f64, f64) {
    let at = |s: f64| {
        let s = s.clamp(line[0].x, line[line.len() - 1].x);
        line.windows(2)
            .find(|pair| s <= pair[1].x)
            .map_or(line[line.len() - 1].y, |pair| {
                let (a, b) = (pair[0], pair[1]);
                a.y + (b.y - a.y) * (s - a.x) / (b.x - a.x)
            })
    };
    line.iter()
        .filter(|p| p.x > from && p.x < to)
        .map(|p| p.y)
        .chain([at(from), at(to)])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), t| {
            (low.min(t), high.max(t))
        })
}

/// One side of a split line (above it in `t`, or below), as convex cells: each
/// segment's half-plane, held to its own run of `s` by bands at the bends.
fn split_halves(line: &[DVec2], above: bool) -> Vec<Vec<Side>> {
    let keep = if above { FAR } else { -FAR };
    let segments = line.len() - 1;
    (0..segments)
        .map(|i| {
            let (a, b) = (line[i], line[i + 1]);
            let mut cell = vec![Side::through(
                a,
                b,
                DVec2::new((a.x + b.x) / 2.0, keep),
                EdgeKind::Broken,
            )];
            if i > 0 {
                cell.push(Side::through(
                    a,
                    a + DVec2::Y,
                    DVec2::new(a.x + 1.0, a.y),
                    EdgeKind::Band,
                ));
            }
            if i + 1 < segments {
                cell.push(Side::through(
                    b,
                    b + DVec2::Y,
                    DVec2::new(b.x - 1.0, b.y),
                    EdgeKind::Band,
                ));
            }
            cell
        })
        .collect()
}

/// A straight crack at the extreme of `crack` on its far side from the shard it
/// bounds (`left`: the shard lies to its right), moved clear by the crack gap.
fn straight(crack: &Crack, left: bool) -> Crack {
    let knots: Vec<f64> = crack.points[1..crack.points.len() - 1]
        .iter()
        .map(|p| p.y)
        .collect();
    let xs = crack.points.iter().map(|p| p.x);
    let s = if left {
        xs.fold(f64::NEG_INFINITY, f64::max) + CRACK_GAP
    } else {
        xs.fold(f64::INFINITY, f64::min) - CRACK_GAP
    };
    Crack::new(s, &knots, || 0.0)
}

/// The chimney pot's mouth relative to the cottage's ground centre in world axes
/// (`w`, `d` the cover box), where the village's chimney smoke rises.
pub fn chimney_flue(w: f64, d: f64, h: f64) -> DVec3 {
    DVec3::new(-w * 0.25, h + POT_MOUTH, -d * 0.2)
}

/// A heap's debris seed carries the cottage it fell from, so its model can lay the
/// broken floor out ahead of it: the build frame's quarter turns and the open
/// floor's depth in decimetres ride under a random scatter seed.
pub fn heap_seed(random: f64, design: &HouseDesign) -> f64 {
    let scatter = (random * f64::from(1u32 << 20)).floor();
    let floor = js_round(design.open_floor() * 10.0).clamp(0.0, 127.0);
    scatter * 512.0 + f64::from(design.turns.rem_euclid(4)) * 128.0 + floor
}

/// A heap's layout from its debris seed: (quarter turns, open floor depth in
/// metres, scatter seed).
pub fn heap_layout(seed: f64) -> (i32, f64, f64) {
    let seed = seed.max(0.0).floor();
    let floor = seed % 128.0;
    let turns = ((seed / 128.0).floor() % 4.0) as i32;
    (turns, floor / 10.0, (seed / 512.0).floor())
}

/// The cottage a heap at `x, z` (world box `w` by `d`) fell from: its centre and
/// design, recovered from the heap's seed. The ridge height is not kept; `h` stands
/// in for it.
pub fn heap_house(x: f64, z: f64, w: f64, d: f64, seed: f64, h: f64) -> (DVec2, HouseDesign) {
    let (turns, open, _) = heap_layout(seed);
    let across = if turns % 2 == 0 { w } else { d };
    let built_w = across + 2.0 * HOUSE_HEAP.inset;
    let built_d = open + HOUSE_HEAP.inset + HOUSE_HEAP.depth;
    let (world_w, world_d) = if turns % 2 == 0 {
        (built_w, built_d)
    } else {
        (built_d, built_w)
    };
    let (sin, cos) = (f64::from(turns) * PI / 2.0).sin_cos();
    // The heap centre's build-frame offset, carried into the world.
    let back = -built_d / 2.0 + HOUSE_HEAP.inset + HOUSE_HEAP.depth / 2.0;
    let center = DVec2::new(x - back * sin, z - back * cos);
    let design = HouseDesign::of(center.x, center.y, world_w, world_d, h);
    (center, design)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn village() -> Vec<(f64, f64, f64, f64, f64)> {
        let mut houses = Vec::new();
        for s in [-1.0, 1.0] {
            for z in [-39.0, -13.0, 13.0, 39.0] {
                houses.push((s * 45.0, z, 5.0, 6.0, 4.6));
            }
            for z in [-46.0, 46.0] {
                houses.push((s * 17.0, z, 7.0, 5.0, 5.2));
            }
            houses.push((s * 24.0, 0.0, 5.0, 7.0, 4.9));
        }
        houses
    }

    #[test]
    fn every_cottage_fronts_its_street_with_its_footprint_unchanged() {
        for (x, z, w, d, h) in village() {
            let design = HouseDesign::of(x, z, w, d, h);
            let door = design.to_world(if design.door_wall == 0 {
                DVec3::Z
            } else {
                DVec3::X
            });
            let street = door_direction(cottage_turns(x, z));
            assert!((door.x - street.x).abs() < 1e-9 && (door.z - street.y).abs() < 1e-9);
            let corner = design.to_world(DVec3::new(design.w / 2.0, 0.0, design.d / 2.0));
            assert!((corner.x.abs() - w / 2.0).abs() < 1e-9, "{x} {z}");
            assert!((corner.z.abs() - d / 2.0).abs() < 1e-9, "{x} {z}");
            if design.style.form == RoofForm::Hip {
                assert!(
                    design.d >= design.w,
                    "a hip ridge runs along the longer side"
                );
            }
        }
    }

    #[test]
    fn the_village_mixes_roof_forms_finishes_and_paints() {
        let designs: Vec<HouseDesign> = village()
            .into_iter()
            .map(|(x, z, w, d, h)| HouseDesign::of(x, z, w, d, h))
            .collect();
        let styles: std::collections::HashSet<usize> = designs
            .iter()
            .map(|d| HOUSE_STYLES.iter().position(|s| *s == d.style).unwrap())
            .collect();
        assert!(styles.len() >= 6, "{styles:?}");
        assert!(designs.iter().any(|d| d.style.form == RoofForm::Hip));
        assert!(designs.iter().any(|d| d.door_wall == 2));
        assert!(designs.iter().any(|d| d.style.walls == WallFinish::Brick));
        assert!(designs.iter().any(|d| d.style.cover == RoofCover::Seams));
        // Side-gable cottages raise a cross gable over the door, and the roof's
        // crack runs clear of it.
        let wings: Vec<&HouseDesign> = designs.iter().filter(|d| d.wing().is_some()).collect();
        assert!(wings.len() >= 3, "{}", wings.len());
        for design in wings {
            let wing = design.wing().expect("wing");
            assert!(wing.crest() < design.h, "under the main ridge");
            assert!(wing.z.abs() + wing.half < design.d / 2.0);
            for crack in design.roof_cracks(1.0) {
                let z = crack.points[1].x;
                assert!((z - wing.z).abs() > wing.half, "{z} {wing:?}");
            }
        }
    }

    /// No roof slab's collider reaches over a wall: a wall shard tipping out from
    /// under an eave resting on its top would be held standing under the slab.
    #[test]
    fn roof_slab_colliders_stay_inside_the_walls() {
        for (x, z, w, d, h) in village() {
            let design = HouseDesign::of(x, z, w, d, h);
            let (inside_w, inside_d) = (design.w / 2.0 - WALL_DEPTH, design.d / 2.0 - WALL_DEPTH);
            for slab in design.pieces().iter().filter(|p| p.piece.part.is_roof()) {
                for p in &slab.hull {
                    assert!(
                        p.x.abs() < inside_w && p.z.abs() < inside_d,
                        "{x} {z} {:?} {p}",
                        slab.piece
                    );
                }
            }
        }
    }

    #[test]
    fn pieces_stay_inside_the_cottage_and_the_heap_leaves_the_front_open() {
        for (x, z, w, d, h) in village() {
            let design = HouseDesign::of(x, z, w, d, h);
            let pieces = design.pieces();
            let roof = design.roof();
            // Every wall in two or three strips, each split in two but the back
            // wall's above its stub; each side slope in two strips split at the
            // ridge and eave, each hip end likewise, the chimney in two and the
            // rubble mound.
            let walls = |length: f64| if length >= LONG_WALL { 3 } else { 2 };
            let expected = 3 * walls(design.w)
                + 4 * walls(design.d)
                + 8
                + if roof.form == RoofForm::Hip { 8 } else { 0 }
                + 2
                + 1;
            assert_eq!(pieces.len(), expected, "{x} {z}");
            assert!(pieces.len() <= HousePiece::ALL.len());
            for piece in &pieces {
                assert!(HousePiece::ALL.contains(&piece.piece));
                assert!(piece.piece.shard < piece.piece.part.max_shards());
                for p in &piece.hull {
                    assert!(
                        p.x.abs() <= roof.eave_x + 0.2 && p.z.abs() <= roof.rake_z + 0.2,
                        "{:?} {p}",
                        piece.piece
                    );
                    // The mound heaps up from the ground, the rest from the plinth.
                    let floor = if piece.piece.part == HousePart::Mound {
                        0.0
                    } else {
                        PLINTH
                    };
                    assert!(p.y >= floor - 1e-9 && p.y <= h + 1.0);
                }
            }
            // Neighbouring shards' colliders start clear of each other across their
            // crack, however it wanders, and across their split.
            let extent = |shape: &PieceShape, axis: fn(DVec2) -> f64| {
                shape
                    .hull
                    .iter()
                    .map(|p| axis(shape.plane.project(*p)))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), v| {
                        (low.min(v), high.max(v))
                    })
            };
            for part in HousePart::ALL
                .into_iter()
                .filter(|&p| p != HousePart::Chimney)
            {
                let split = part.is_roof() || part.wall_index().is_some();
                let strip = |shape: &&PieceShape| {
                    if split {
                        shape.piece.shard / 2
                    } else {
                        shape.piece.shard
                    }
                };
                let shards: Vec<&PieceShape> =
                    pieces.iter().filter(|p| p.piece.part == part).collect();
                for a in &shards {
                    for b in &shards {
                        if strip(a) + 1 == strip(b) {
                            assert!(extent(a, |q| q.x).1 < extent(b, |q| q.x).0, "{part:?}");
                        } else if strip(a) == strip(b) && a.piece.shard < b.piece.shard {
                            assert!(extent(a, |q| q.y).1 < extent(b, |q| q.y).0, "{part:?}");
                        }
                    }
                }
            }
            let chimney: Vec<&PieceShape> = pieces
                .iter()
                .filter(|p| p.piece.part == HousePart::Chimney)
                .collect();
            assert!(extent(chimney[0], |q| q.y).1 < extent(chimney[1], |q| q.y).0);
            // The back wall breaks off above the stub the heap keeps.
            let (_, floors) = design.wall_breaks(1);
            for shard in pieces
                .iter()
                .filter(|p| p.piece.part == HousePart::BackWall)
            {
                let (a, b) = floors[usize::from(shard.piece.shard / 2)];
                let lowest = shard.hull.iter().map(|q| q.y).fold(f64::INFINITY, f64::min);
                assert!(lowest >= a.y.max(b.y), "{lowest} {a} {b}");
            }
            let stub = design.back_stub();
            assert!(stub.iter().all(|q| q.y < HOUSE_HEAP.height + 0.05));
            let (center, size) = design.heap();
            assert!(
                center.z + size.z / 2.0 < 0.0,
                "the heap lies in the back half"
            );
            assert!(design.open_floor() > 3.0);
            let seed = heap_seed(0.37, &design);
            let (turns, open, _) = heap_layout(seed);
            assert_eq!(turns, design.turns.rem_euclid(4));
            assert!((open - design.open_floor()).abs() < 0.051);
            let (offset, size) = design.heap_box();
            let (center, found) = heap_house(x + offset.x, z + offset.y, size.x, size.y, seed, h);
            assert!(
                (center.x - x).abs() < 1e-6 && (center.y - z).abs() < 1e-6,
                "{x} {z}"
            );
            assert_eq!(found.style, design.style);
        }
    }

    #[test]
    fn damage_stages_follow_health() {
        assert_eq!(house_damage_stage(HOUSE_HEALTH, HOUSE_HEALTH), 0);
        assert_eq!(house_damage_stage(HOUSE_HEALTH * 0.6, HOUSE_HEALTH), 1);
        assert_eq!(house_damage_stage(HOUSE_HEALTH * 0.2, HOUSE_HEALTH), 2);
        assert!(is_house_heap(HOUSE_HEAP.height));
        assert!(!is_house_heap(1.25));
    }
}
