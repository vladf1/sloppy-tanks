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
/// Wall top (the eave line) as a fraction of the ridge height.
const WALL_FRACTION: f64 = 0.68;
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
    /// Dormer plan on a gable roof: 0 none, 1 a pair on one slope, 2 one each side.
    pub dormers: u32,
    pub shutters: Shutters,
}

const fn style(
    form: RoofForm,
    door: DoorWall,
    cover: RoofCover,
    walls: WallFinish,
    [siding, trim, accent, roof]: [u32; 4],
    dormers: u32,
    shutters: Shutters,
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
        shutters,
    }
}

/// The village's house types, ordered so neighbours in the table differ in form,
/// finish and colour; [`HouseDesign::facing`] picks one per cottage.
#[rustfmt::skip]
pub const HOUSE_STYLES: [HouseStyle; 8] = {
    use DoorWall::{Eave, Gable as Front};
    use RoofCover::{Seams, Shingles};
    use RoofForm::{Gable, Hip};
    use Shutters::{Board, Louvred, None};
    use WallFinish::{Brick, Clapboard};
    [
        // Cream clapboard, bottle-green plank shutters, charcoal shingles and dormers.
        style(Gable, Front, Shingles, Clapboard, [0xe8dcc0, 0xf6f2e8, 0x2f5a40, 0x3d4652], 1, Board),
        // Red brick under a slate hip roof, navy door and louvres.
        style(Hip, Front, Shingles, Brick, [0xf2e2d2, 0xece4d2, 0x24385a, 0x58626e], 0, Louvred),
        // Sage side-gable with an oxblood door and terracotta shingles.
        style(Gable, Eave, Shingles, Clapboard, [0x8fa37c, 0xf1ead8, 0x6a2a24, 0xb24c34], 2, Board),
        // Barn-red front gable under a green standing-seam roof.
        style(Gable, Front, Seams, Clapboard, [0xa6402f, 0xf4f0e6, 0x2e2c2a, 0x2f6a48], 0, None),
        // Butter-yellow hip cottage, umber shingles.
        style(Hip, Front, Shingles, Clapboard, [0xe0bd62, 0xf6f2e8, 0x5c3c22, 0x7a4c30], 0, Board),
        // Powder-blue side-gable with navy louvres under charcoal.
        style(Gable, Eave, Shingles, Clapboard, [0x88a6bf, 0xf6f2e8, 0x22314d, 0x383d46], 0, Louvred),
        // White front gable under a red standing-seam roof, black plank shutters.
        style(Gable, Front, Seams, Clapboard, [0xf0ece2, 0xffffff, 0x1d2622, 0xa83a2c], 2, Board),
        // Brick side-gable with moss shingles and green louvres.
        style(Gable, Eave, Shingles, Brick, [0xe6d2c0, 0xe8dfca, 0x2f4b3a, 0x4d6440], 0, Louvred),
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
    pub fn new(form: RoofForm, w: f64, d: f64, h: f64) -> Self {
        let wall = h * WALL_FRACTION;
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
}

impl HousePart {
    pub const ALL: [Self; 9] = [
        Self::FrontWall,
        Self::BackWall,
        Self::RightWall,
        Self::LeftWall,
        Self::RightRoof,
        Self::LeftRoof,
        Self::FrontRoof,
        Self::BackRoof,
        Self::Chimney,
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

    /// The most shards a collapse breaks the part into.
    pub const fn max_shards(self) -> u8 {
        match self {
            Self::FrontWall | Self::BackWall | Self::RightWall | Self::LeftWall => 3,
            Self::RightRoof | Self::LeftRoof | Self::FrontRoof | Self::BackRoof => 2,
            Self::Chimney => 1,
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

    /// Every shard any cottage can break into, part by part.
    pub const ALL: [Self; 21] = {
        use HousePart::*;
        let s = Self::new;
        [
            s(FrontWall, 0),
            s(FrontWall, 1),
            s(FrontWall, 2),
            s(BackWall, 0),
            s(BackWall, 1),
            s(BackWall, 2),
            s(RightWall, 0),
            s(RightWall, 1),
            s(RightWall, 2),
            s(LeftWall, 0),
            s(LeftWall, 1),
            s(LeftWall, 2),
            s(RightRoof, 0),
            s(RightRoof, 1),
            s(LeftRoof, 0),
            s(LeftRoof, 1),
            s(FrontRoof, 0),
            s(FrontRoof, 1),
            s(BackRoof, 0),
            s(BackRoof, 1),
            s(Chimney, 0),
        ]
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
/// Rafters hang this far under a roof deck; a slab's collider takes them in only
/// inside the wall line, clear of the wall tops it rests on.
pub const RAFTER_DEPTH: f64 = 0.16;
const RAFTER_INSET: f64 = 0.45;

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
        Roof::new(self.style.form, self.w, self.d, self.h)
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
            _ => Vec::new(),
        }
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
        [rng.range(-0.35, 0.35)]
            .into_iter()
            .map(|offset| {
                let mut z = offset * ridge;
                // Past a dormer rather than through it.
                if let Some(d) = dormers.iter().find(|d| (z - **d).abs() < 0.9) {
                    let away = if z >= *d { 1.0 } else { -1.0 };
                    z = d + away * 0.95;
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

    /// Every shard the cottage collapses into, with its collider: each wall in two
    /// or three panels (the back wall above the stub it leaves standing in the
    /// heap), each roof slope in two slabs and the chimney.
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
            for shard in 0..=cracks.len() {
                let (left, right) = (shard.checked_sub(1).map(|k| &cracks[k]), cracks.get(shard));
                let floor = floors.get(shard).map(|&(a, b)| {
                    Side::through(a, b, DVec2::new(a.x, roof.ridge), EdgeKind::Broken)
                });
                let region = ShardRegion::between(left, right, floor);
                // The collider: the shard's straight core between its cracks' extremes.
                let core = ShardRegion::between(
                    left.map(|c| straight(c, true)).as_ref(),
                    right.map(|c| straight(c, false)).as_ref(),
                    floors.get(shard).map(|&(a, b)| {
                        let level = a.y.max(b.y) + CRACK_GAP;
                        Side::through(
                            DVec2::new(0.0, level),
                            DVec2::new(1.0, level),
                            DVec2::new(0.0, roof.ridge),
                            EdgeKind::Broken,
                        )
                    }),
                );
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
                    HousePiece::new(part, shard as u8),
                    plane,
                    outline.clone(),
                    region,
                    hull,
                ));
            }
        }
        let (xe, ze, rh) = (roof.eave_x, roof.rake_z, roof.ridge_half);
        // A slab's collider: its deck and shingles, and the rafters under it inside
        // the wall line.
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
            let wall_line = self.w / 2.0 - RAFTER_INSET;
            for shard in 0..=cracks.len() {
                let (left, right) = (shard.checked_sub(1).map(|k| &cracks[k]), cracks.get(shard));
                let region = ShardRegion::between(left, right, None);
                let core = ShardRegion::between(
                    left.map(|c| straight(c, true)).as_ref(),
                    right.map(|c| straight(c, false)).as_ref(),
                    None,
                );
                let points = core.outline_points(&outline);
                // The rafters' reach, cut off at the wall line.
                let rafters = core.outline_points(&[
                    DVec2::new(-ze, 0.0),
                    DVec2::new(ze, 0.0),
                    DVec2::new(ze, wall_line),
                    DVec2::new(-ze, wall_line),
                ]);
                let hull = points
                    .iter()
                    .flat_map(|q| lift(side * q.y, q.x, false))
                    .chain(rafters.iter().map(|q| lift(side * q.y, q.x, true)[1]))
                    .collect();
                pieces.push(PieceShape::new(
                    HousePiece::new(part, shard as u8),
                    plane,
                    outline.clone(),
                    region,
                    hull,
                ));
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
                for shard in 0..2 {
                    let (left, right) = if shard == 0 {
                        (None, Some(&crack))
                    } else {
                        (Some(&crack), None)
                    };
                    let core = ShardRegion::between(
                        left.map(|c| straight(c, true)).as_ref(),
                        right.map(|c| straight(c, false)).as_ref(),
                        None,
                    );
                    let hull: Vec<DVec3> = core
                        .outline_points(&outline)
                        .iter()
                        .flat_map(|q| lift(q.x, end * q.y, false))
                        .collect();
                    if hull.len() < 6 {
                        continue;
                    }
                    pieces.push(PieceShape::new(
                        HousePiece::new(part, shard),
                        plane,
                        outline.clone(),
                        ShardRegion::between(left, right, None),
                        hull,
                    ));
                }
            }
        }
        let flue = self.flue();
        let (_, top) = self.chimney_span();
        let half = CHIMNEY / 2.0 + 0.05;
        let corners = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(sx, sz)| DVec2::new(flue.x + sx * half, flue.z + sz * half));
        // The stack's collider starts clear of the roof it pierced.
        let bottom = corners
            .iter()
            .map(|c| roof.top(c.x, c.y))
            .fold(f64::NEG_INFINITY, f64::max)
            + 0.1
            + 2.0 * GAP;
        let hull = corners
            .into_iter()
            .flat_map(|c| [bottom, top + 0.16].map(|y| DVec3::new(c.x, y, c.y)))
            .collect();
        pieces.push(PieceShape::new(
            HousePiece::new(HousePart::Chimney, 0),
            PartPlane {
                origin: DVec3::ZERO,
                s: DVec3::X,
                t: DVec3::Z,
            },
            Vec::new(),
            ShardRegion::between(None, None, None),
            hull,
        ));
        pieces
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
    }

    #[test]
    fn pieces_stay_inside_the_cottage_and_the_heap_leaves_the_front_open() {
        for (x, z, w, d, h) in village() {
            let design = HouseDesign::of(x, z, w, d, h);
            let pieces = design.pieces();
            let roof = design.roof();
            // Every wall in two or three panels, each side slope in two slabs, the
            // hip ends whole and the chimney.
            let walls = |length: f64| if length >= LONG_WALL { 3 } else { 2 };
            let expected = 2 * walls(design.w)
                + 2 * walls(design.d)
                + 4
                + if roof.form == RoofForm::Hip { 4 } else { 0 }
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
                    assert!(p.y >= PLINTH - 1e-9 && p.y <= h + 1.0);
                }
            }
            // Neighbouring shards' colliders start clear of each other across their
            // crack, however it wanders.
            for part in HousePart::ALL {
                let shards: Vec<&PieceShape> =
                    pieces.iter().filter(|p| p.piece.part == part).collect();
                for pair in shards.windows(2) {
                    let s = |shape: &PieceShape, f: fn(f64, f64) -> f64, from: f64| {
                        shape
                            .hull
                            .iter()
                            .map(|p| shape.plane.project(*p).x)
                            .fold(from, f)
                    };
                    assert!(
                        s(pair[0], f64::max, f64::NEG_INFINITY)
                            < s(pair[1], f64::min, f64::INFINITY),
                        "{part:?}"
                    );
                }
            }
            // The back wall breaks off above the stub the heap keeps.
            let (_, floors) = design.wall_breaks(1);
            for shard in pieces
                .iter()
                .filter(|p| p.piece.part == HousePart::BackWall)
            {
                let (a, b) = floors[usize::from(shard.piece.shard)];
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
