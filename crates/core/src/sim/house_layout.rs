//! The village cottage's plan, shared by the simulation and its model: the design a
//! cottage is built to (roof form, finishes and paints, chosen from where it stands
//! and turned to front its street) and the measurements of its walls and roof.
//!
//! A cottage is built in its own frame: ridge along z, gable walls (or hip ends) at
//! ±z, eave walls at ±x, ground at y = 0. Its cover box (`w` along world x, `d` along
//! world z, `h` to the ridge) is the collision footprint; whole quarter turns carry
//! the frame into the world, swapping the box's width and depth on odd turns.

use std::f64::consts::PI;

use glam::{DVec2, DVec3};

use super::math::{Random, js_round};

/// A cottage's full health. It stands as long as its frame carries it
/// (`house_structure`); its health only reports the share of its parts still standing.
pub const HOUSE_HEALTH: f64 = 600.0;

/// Height of the fieldstone plinth (two 21 cm steps up to the door).
pub const PLINTH: f64 = 0.42;
/// Houses this tall to the ridge or taller have two floors, a second row of
/// windows over the first and a belt course between them, and their eaves sit
/// this far up the ridge height whatever their type's attic.
pub const TWO_STOREY: f64 = 6.2;
const TWO_STOREY_EAVE: f64 = 0.7;
/// Horizontal roof overhang past the eave walls and past the gable walls.
pub const EAVE_OVERHANG: f64 = 0.4;
pub const RAKE_OVERHANG: f64 = 0.3;
/// Roof deck thickness, measured square to the slope.
pub const ROOF_THICKNESS: f64 = 0.1;
/// How far a wall reaches in behind its outside face.
pub const WALL_DEPTH: f64 = 0.2;
/// Chimney: plan size of the stack, height of the brick above the ridge, and the
/// pot's mouth above the ridge, where the smoke leaves.
pub const CHIMNEY: f64 = 0.62;
pub const CHIMNEY_TOP: f64 = 0.3;
pub const POT_MOUTH: f64 = 0.7;

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
/// own (moss, blue-grey slate, warm grey slate, teal, terracotta, faded brick red,
/// barn-red steel and weathered blue slate), never the brown of the dirt or the
/// olive of the grass, so from overhead every roof stands off the ground by value and
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
        // Sage side-gable with an oxblood door under a cross gable and warm grey
        // weathered slate, a dormer lighting the back of the attic.
        style(Gable, Eave, Shingles, Clapboard, [0xa3ae8e, 0xe3dbc8, 0x6a2a24, 0xaea796], 3, true, Board, false, 0.6),
        // Weathered tan front gable with a shed porch over the door, under
        // weathered teal standing-seam steel.
        style(Gable, Front, Seams, Clapboard, [0xc4a882, 0xe2dacb, 0x2e2c2a, 0x4a706a], 0, false, None, true, 0.68),
        // Ochre hip cottage under faded terracotta shingles.
        style(Hip, Front, Shingles, Clapboard, [0xd2b26e, 0xe6e0d4, 0x5c3c22, 0xb87a62], 0, false, Board, false, 0.7),
        // Slate-grey side-gable with black louvres and a cross gable under
        // faded brick-red shingles.
        style(Gable, Eave, Shingles, Clapboard, [0x9ea9b2, 0xe6e0d4, 0x232a28, 0xd07a62], 0, true, Louvred, false, 0.62),
        // Chalk-white front gable with a porch, under weathered barn-red
        // standing-seam steel and black shutters.
        style(Gable, Front, Seams, Clapboard, [0xe6e2d8, 0xeae6dd, 0x1d2622, 0x7a4c40], 2, false, Board, true, 0.6),
        // Sand clapboard side-gable with weathered blue-grey slate, green louvres
        // and a cross gable.
        style(Gable, Eave, Shingles, Clapboard, [0xd8c8a2, 0xe5dfd2, 0x2f4b3a, 0x939fa4], 0, true, Louvred, false, 0.64),
    ]
};

/// Roof measurements shared by the deck, shingles, gutters, chimney and the
/// cottage's structure, in the build frame.
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// The parts a cottage's model is built in, in the build frame: front is +z (the
/// gable or hip end the ridge points at), right is +x. Hip roofs add the two end
/// slopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// What stays on the ground: the plinth, the door's steps and the yard.
    Ground,
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
        Self::Ground,
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
}

/// Rafters hang this far under a roof deck; a slab's collider takes them in only
/// inside the wall line, clear of the wall tops.
pub const RAFTER_DEPTH: f64 = 0.16;

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
}

/// The walls as (outside-face centre, outward normal, along-wall direction, half
/// length): front (+z), back, right (+x) and left, the order the model builds them.
/// The along direction is the wall frame's +x seen from outside.
/// The kitchen garden behind some cottages: the longest its bed runs along the back
/// wall and how deep it is (m), and how far out from the wall it starts.
pub const GARDEN: (f64, f64) = (3.8, 1.8);
pub const GARDEN_GAP: f64 = 0.85;

/// A cottage's garden bed along its back wall ([`wall_frames`] 1), in that wall's frame:
/// from `u0` to `u1` along it and from `near` to `far` out from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GardenBed {
    pub u0: f64,
    pub u1: f64,
    pub near: f64,
    pub far: f64,
    /// Its length along the wall, its middle there and its depth out from it.
    pub length: f64,
    pub offset: f64,
    pub depth: f64,
}

impl GardenBed {
    /// The bed's box in the build frame, from the ground up to `height`.
    pub fn bounds(&self, design: &HouseDesign, height: f64) -> (DVec3, DVec3) {
        let (center, out, along, _) = wall_frames(design.w, design.d)[1];
        let corner = |u: f64, o: f64, y: f64| center + along * u + out * o + DVec3::Y * y;
        let (a, b) = (
            corner(self.u0, self.near, 0.0),
            corner(self.u1, self.far, height),
        );
        (a.min(b), a.max(b))
    }
}

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

    /// Its garden bed, which one cottage in three keeps (the rest stack firewood by the
    /// wall), and the cosmetic random stream its planting goes on with.
    pub fn garden_bed(&self) -> Option<(GardenBed, Random)> {
        if !self.variant.is_multiple_of(3) {
            return None;
        }
        let half = wall_frames(self.w, self.d)[1].3;
        let lot = (self.style.siding >> 3) ^ self.turns.rem_euclid(4) as u32;
        let mut rng = Random::new(f64::from(self.variant) * 3.0 + f64::from(lot % 97) + 77.0);
        let length = (2.0 * half - 0.9).min(GARDEN.0);
        let offset = rng.range(-0.5, 0.5) * (2.0 * half - 0.6 - length).max(0.0);
        let depth = GARDEN.1 + rng.range(-0.15, 0.2);
        let bed = GardenBed {
            u0: offset - length / 2.0,
            u1: offset + length / 2.0,
            near: GARDEN_GAP,
            far: GARDEN_GAP + depth,
            length,
            offset,
            depth,
        };
        Some((bed, rng))
    }

    pub fn yaw(&self) -> f64 {
        f64::from(self.turns) * PI / 2.0
    }

    pub fn roof(&self) -> Roof {
        let eave = if self.storeys() == 2 {
            TWO_STOREY_EAVE
        } else {
            self.style.eave
        };
        Roof::new(self.style.form, self.w, self.d, self.h, eave)
    }

    /// Floors under the roof: two on a house at least [`TWO_STOREY`] tall.
    pub fn storeys(&self) -> u32 {
        if self.h >= TWO_STOREY { 2 } else { 1 }
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
        let (world_w, world_d) = self.world_box();
        self.to_local(chimney_flue(world_w, world_d, self.h))
    }

    /// The cover box's width and depth along the world's x and z.
    pub fn world_box(&self) -> (f64, f64) {
        if self.turns.rem_euclid(2) == 0 {
            (self.w, self.d)
        } else {
            (self.d, self.w)
        }
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
}

/// Window centres along a wall `half` long either way: either side of the door,
/// a pair on a gable or long wall, otherwise one in the middle.
pub fn window_columns(door: bool, gable: bool, half: f64) -> Vec<f64> {
    if door || (gable && half >= 2.8) {
        vec![-half * 0.54, half * 0.54]
    } else if half >= 2.75 {
        vec![-half * 0.48, half * 0.48]
    } else {
        vec![0.0]
    }
}

/// The chimney pot's mouth relative to the cottage's ground centre in world axes
/// (`w`, `d` the cover box), where the village's chimney smoke rises.
pub fn chimney_flue(w: f64, d: f64, h: f64) -> DVec3 {
    DVec3::new(-w * 0.25, h + POT_MOUTH, -d * 0.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn village() -> Vec<(f64, f64, f64, f64, f64)> {
        crate::sim::arena::arena_layout()
            .into_iter()
            .filter(|c| c.kind == crate::sim::types::CoverKind::House)
            .map(|c| (c.x, c.z, c.w, c.d, c.h))
            .collect()
    }

    /// The row mixes one- and two-storey houses, mirrored between the teams'
    /// halves, and a two-storey house's walls are tall enough for both floors.
    #[test]
    fn the_village_mixes_storeys_mirrored_between_the_halves() {
        let houses = village();
        let mut two = 0;
        for &(x, z, w, d, h) in &houses {
            assert!(
                houses.contains(&(-x, z, w, d, h)),
                "{x} {z} has a mirror image"
            );
            let design = HouseDesign::of(x, z, w, d, h);
            if design.storeys() == 2 {
                two += 1;
                assert!(design.roof().wall - PLINTH > 4.0, "{x} {z}");
            }
        }
        assert!(
            (2..houses.len() / 2).contains(&two),
            "{two} two-storey houses"
        );
        let heights: std::collections::BTreeSet<u64> =
            houses.iter().map(|&(.., h)| h.to_bits()).collect();
        assert!(heights.len() >= 4, "the row's ridges differ");
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
        // Side-gable cottages raise a cross gable over the door.
        let wings: Vec<&HouseDesign> = designs.iter().filter(|d| d.wing().is_some()).collect();
        assert!(wings.len() >= 3, "{}", wings.len());
        for design in wings {
            let wing = design.wing().expect("wing");
            assert!(wing.crest() < design.h, "under the main ridge");
            assert!(wing.z.abs() + wing.half < design.d / 2.0);
        }
    }
}
