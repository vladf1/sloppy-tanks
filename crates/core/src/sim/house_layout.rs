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

use super::math::js_round;

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

/// The parts a collapsing cottage breaks into, in the build frame: front is +z (the
/// gable or hip end the ridge points at), right is +x. Hip roofs add the two end
/// slopes; the plinth stays as the heap's footing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HousePiece {
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

impl HousePiece {
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
}

/// A collapse piece's collider: hull points in the build frame and their bounds.
#[derive(Clone, Debug)]
pub struct PieceShape {
    pub piece: HousePiece,
    pub hull: Vec<DVec3>,
    pub center: DVec3,
    pub size: DVec3,
}

impl PieceShape {
    fn new(piece: HousePiece, hull: Vec<DVec3>) -> Self {
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
            hull,
            center: (min + max) / 2.0,
            size: max - min,
        }
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

    /// Every piece the cottage collapses into, with its collider. The back wall
    /// crumbles where it stood into the heap [`Self::heap`] leaves, so it has none.
    pub fn pieces(&self) -> Vec<PieceShape> {
        let roof = self.roof();
        let mut pieces = Vec::new();
        for (index, (center, out, along, half)) in
            wall_frames(self.w, self.d).into_iter().enumerate()
        {
            if HousePiece::wall(index) == HousePiece::BackWall {
                continue;
            }
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
            let hull = outline
                .iter()
                .flat_map(|p| {
                    let face = center + along * p.x + DVec3::Y * p.y;
                    [face, face - out * WALL_DEPTH]
                })
                .collect();
            pieces.push(PieceShape::new(HousePiece::wall(index), hull));
        }
        let lift = |p: DVec3| [p, p + DVec3::Y * (roof.thick + 0.03)];
        let (xe, ye, ze, rh) = (roof.eave_x, roof.eave_y, roof.rake_z, roof.ridge_half);
        let ridge = roof.ridge + GAP;
        for side in [1.0, -1.0] {
            let hull = [
                DVec3::new(0.0, ridge, -rh),
                DVec3::new(0.0, ridge, rh),
                DVec3::new(side * xe, ye + GAP, -ze),
                DVec3::new(side * xe, ye + GAP, ze),
            ]
            .into_iter()
            .flat_map(lift)
            .collect();
            let piece = if side > 0.0 {
                HousePiece::RightRoof
            } else {
                HousePiece::LeftRoof
            };
            pieces.push(PieceShape::new(piece, hull));
        }
        if roof.form == RoofForm::Hip {
            for end in [1.0, -1.0] {
                let hull = [
                    DVec3::new(0.0, ridge, end * rh),
                    DVec3::new(-xe, ye + GAP, end * ze),
                    DVec3::new(xe, ye + GAP, end * ze),
                ]
                .into_iter()
                .flat_map(lift)
                .collect();
                let piece = if end > 0.0 {
                    HousePiece::FrontRoof
                } else {
                    HousePiece::BackRoof
                };
                pieces.push(PieceShape::new(piece, hull));
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
            + 0.03
            + 2.0 * GAP;
        let hull = corners
            .into_iter()
            .flat_map(|c| [bottom, top + 0.16].map(|y| DVec3::new(c.x, y, c.y)))
            .collect();
        pieces.push(PieceShape::new(HousePiece::Chimney, hull));
        pieces
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
            // Three walls (the back one crumbles into the heap), two or four roof
            // slopes and the chimney.
            let expected = if roof.form == RoofForm::Hip { 8 } else { 6 };
            assert_eq!(pieces.len(), expected);
            assert!(
                pieces
                    .iter()
                    .all(|piece| piece.piece != HousePiece::BackWall)
            );
            for piece in &pieces {
                for p in &piece.hull {
                    assert!(p.x.abs() <= roof.eave_x + 1e-9 && p.z.abs() <= roof.rake_z + 1e-9);
                    assert!(p.y >= PLINTH - 1e-9 && p.y <= h + 1.0);
                }
            }
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
