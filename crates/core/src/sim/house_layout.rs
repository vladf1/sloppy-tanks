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
use serde::{Deserialize, Serialize};

use super::math::{Random, js_round};

/// A cottage's full health. It stands as long as its frame carries it
/// (`house_structure`); its health only reports the share of its parts still standing.
pub const HOUSE_HEALTH: f64 = 600.0;

/// Height of the fieldstone plinth (two 21 cm steps up to the door).
pub const PLINTH: f64 = 0.42;
/// A garage house's (Senda's) slab over the ground: the tract's houses stand on
/// slabs at grade, one low step up to the door.
pub const GARAGE_HOUSE_PLINTH: f64 = 0.12;
/// Houses this tall to the ridge or taller have two floors, a second row of
/// windows over the first and a belt course between them, and their eaves sit
/// this far up the ridge height whatever their type's attic.
pub const TWO_STOREY: f64 = 6.2;
const TWO_STOREY_EAVE: f64 = 0.7;
/// Horizontal roof overhang past the eave walls and past the gable walls.
pub const EAVE_OVERHANG: f64 = 0.4;
pub const RAKE_OVERHANG: f64 = 0.3;
/// A garage house's (Senda's) roof: its pitch, rise per metre across (the tract's low
/// 5:12, a 2.5 m rise over half an 11.7 m front), and its eave and rake overhangs (m):
/// the tract's 0.55 m eaves over exposed rafter tails at the level's scale, short
/// enough that from overhead each house's roof stands apart from its neighbours' across
/// the side yard.
pub const GARAGE_HOUSE_PITCH: f64 = 0.42;
const GARAGE_HOUSE_OVERHANG: (f64, f64) = (0.3, 0.3);
/// A garage house's hip ends' run (m) where its ridge runs along its street: steeper
/// than its long slopes, so the street sees a long eave under a ridge along the front
/// (a trapezoid) rather than the narrow triangle of a hip end.
const ALONG_STREET_HIP_END: f64 = 1.8;
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
    /// Clay barrel tile: rounded courses running down the slope.
    Tile,
    /// Flat concrete tile: a smooth slope in its paint, crossed by its courses' butts.
    FlatTile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shutters {
    None,
    /// Louvred shutters with angled slats.
    Louvred,
    /// Plank shutters on a Z brace.
    Board,
    /// A garage house's single board-and-batten shutter, on its upper window's outer side
    /// only (the reference house's).
    BoardLeaf,
}

/// A garage house's solar arrays: as its lot has them (none on some, one small block on
/// others, a full block on most, sometimes a second on the opposite slope), or a real
/// house's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Solar {
    Lot,
    /// One small block of two rows of two.
    Small,
    /// A full block and a block on each of two more slopes.
    Three,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallFinish {
    /// Painted clapboard over corner boards.
    Clapboard,
    /// Red brick laid in bond.
    Brick,
    /// Smooth painted stucco over a concrete footing (Senda's houses).
    Stucco,
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
    /// the roof reads as a T from above. A garage house's porch has a front gable.
    pub wing: bool,
    pub shutters: Shutters,
    /// A front-gable cottage's door wall carries a full shed porch roof on
    /// brackets instead of the small pedimented canopy.
    pub porch: bool,
    /// Wall top (the eave line) as a fraction of the ridge height: lower walls
    /// under a steep attic roof, higher ones under a shallow one.
    pub eave: f64,
    /// What a garage house's garage door is finished in.
    pub garage_door: GaragePaint,
    /// A garage house's upper floor opens over the garage onto a small balcony between
    /// stucco cheeks, railed low in dark iron.
    pub balcony: bool,
    /// A garage house's upper floor over the garage stands forward under a two-storey
    /// front gable of its own (a Spanish house's, and the reference house on Rumbo's), the main
    /// roof's hip end behind it.
    pub gable_front: bool,
    /// A garage house's entry (a type with a porch and no wing) is a one-storey stucco room
    /// beside the garage under a small hip that runs on past it as an open porch on dark
    /// timber posts, the door in the room's side under the porch (the reference house's).
    pub room_porch: bool,
    /// A Spanish garage house's wing carries an arched feature window in its face and the
    /// entry as an open arch in its outer side, under no two-storey front gable, a stucco
    /// chimney up the flank beyond the wing (Rumbo north lot 1's).
    pub loggia: bool,
    pub solar: Solar,
    /// A garage house's entry is a one-storey masonry porch under a flat-tile shed roof,
    /// its piers framing a square opening, the door back inside it (Rumbo north lot 3's).
    pub arched_porch: bool,
    /// A garage house's upper floor is clad above its belt course in board-and-batten,
    /// over stucco below, its porch on white square columns turning the street corner (2
    /// Neblina's).
    pub batten_upper: bool,
    /// How its garage door's face is made.
    pub garage_look: GarageLook,
    /// What a Craftsman's front-gable entry wing is faced in.
    pub veneer: Veneer,
}

/// A garage door's face: the tract's one-piece sectional doors in vertical boards (most),
/// full-width raised panels (the Prairie and Craftsman fronts) or a pair of X braces (5
/// Rumbo's), or a two-leaf carriage door on strap hinges (the board-and-batten house's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GarageLook {
    Boards,
    Panels,
    XBrace,
    Carriage,
}

/// A Craftsman entry wing's face (and Rumbo north lot 3's porch's): the tract's dark brown brick
/// ([`SENDA_BRICK`]), or buff stacked stone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Veneer {
    Brick,
    Stone,
}

/// A garage door's finish: stained (the tract's milk-chocolate carriage doors), or painted
/// in the house's accent (its front door and shutters' paint) or its trim colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GaragePaint {
    Stained,
    Accent,
    Trim,
}

impl HouseStyle {
    /// The style with its garage door finished in `paint`.
    const fn garage_door(self, paint: GaragePaint) -> Self {
        Self {
            garage_door: paint,
            ..self
        }
    }

    /// The style with a balcony over its garage.
    const fn balcony(self) -> Self {
        Self {
            balcony: true,
            ..self
        }
    }

    /// The style with a two-storey front gable over its garage.
    const fn gable_front(self) -> Self {
        Self {
            gable_front: true,
            ..self
        }
    }

    /// The style with a stucco room and porch under one small hip for its entry.
    const fn room_porch(self) -> Self {
        Self {
            room_porch: true,
            ..self
        }
    }

    /// The style with a wing whose entry is an open arch in its outer side.
    const fn loggia(self) -> Self {
        Self {
            loggia: true,
            ..self
        }
    }

    /// The style with its solar arrays laid out as `solar`.
    const fn solar(self, solar: Solar) -> Self {
        Self { solar, ..self }
    }

    /// The style with a brick arched porch for its entry.
    const fn arched_porch(self) -> Self {
        Self {
            arched_porch: true,
            ..self
        }
    }

    /// The style with a board-and-batten upper floor and a columned porch.
    const fn batten_upper(self) -> Self {
        Self {
            batten_upper: true,
            ..self
        }
    }

    /// The style with its garage door's face made as `look`.
    const fn garage_look(self, look: GarageLook) -> Self {
        Self {
            garage_look: look,
            ..self
        }
    }

    /// The style with its entry wing faced in `veneer`.
    const fn veneer(self, veneer: Veneer) -> Self {
        Self { veneer, ..self }
    }
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
        garage_door: GaragePaint::Stained,
        balcony: false,
        gable_front: false,
        room_porch: false,
        loggia: false,
        arched_porch: false,
        batten_upper: false,
        garage_look: GarageLook::Boards,
        veneer: Veneer::Brick,
        solar: Solar::Lot,
    }
}

/// Which side of its street front a house's garage stands on, seen from the street.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GarageSide {
    Left,
    Right,
}

impl GarageSide {
    /// The garage's direction along the door wall's frame (+1 to its right, as the
    /// wall frame's along axis runs seen from outside).
    pub fn sign(self) -> f64 {
        match self {
            GarageSide::Left => -1.0,
            GarageSide::Right => 1.0,
        }
    }
}

/// A house laid out by its map rather than by where it stands (Senda's): its type in
/// [`SENDA_STYLES`], the quarter turns its front faces (as [`door_direction`]), the
/// whole degrees it turns further about +y to front a curving street, and the side its
/// garage takes. Mirror-image lots differ only in the garage's side. The cover's box is
/// the house's own, before that last turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HousePlan {
    pub style: u8,
    pub street: i8,
    #[serde(default)]
    pub angle: i8,
    pub garage: GarageSide,
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

/// Senda's three house types, three paint schemes each, as the neighbourhood builds
/// them, and lots of their own: a broad, low two-storey stucco block under a low roof
/// with broad eaves, an attached two-car garage under its upper floor on the street front
/// behind a one-piece sectional door, and beside the garage a one-storey wing or porch under a
/// roof of its own with the entry, so every front steps down from two storeys to one as
/// the tract's do. Windows are white vinyl over white plantation shutters, hung with
/// broad board-and-batten shutters in slate, sage, brown or burgundy, never black.
///
/// A Craftsman house has a broad, low front-gable wing beside the garage (`wing`) faced in
/// dark brick or buff stacked stone, a shuttered window in its face and the door recessed
/// beside it at its garage end, and a dark stained box bay over the garage,
/// under flat concrete tile (one scheme under a front-gable main roof, the others under
/// a hip whose ridge runs along the street, `DoorWall::Eave`). A Spanish house has a
/// two-storey stucco front gable over its entry side with twin arched windows high in it,
/// a one-storey front-gabled wing standing before it with an arched entry and small clay
/// pipe vents, and a clay pent roof over the garage under a plain upper wall, under clay
/// barrel tile. A Prairie house, the street's commonest, has an open porch beside the
/// garage under a grey flat-tile shed roof on chunky dark posts over stacked-stone piers,
/// its wall clad in board-and-batten, and three openings across its upper floor (or, at 5
/// Rumbo's, a brick porch whose piers frame an arched entry, with a balcony over the
/// garage), under a flat-tile hip whose ridge runs along the street. The reference house
/// has a front gable over the garage holding a dark box bay, and beside it a stucco room
/// under a clay hip that runs on as a porch on dark posts, under clay tile; Rumbo north lot 8 is
/// built to the same plan. Rumbo north lot 1, on the corner at Neblina, is a cream Spanish house
/// whose wing opens in an arch at its corner end beside a tile-framed feature window, a
/// tall chimney rising in front where the wing meets the house and scalloped arches over
/// its garage. Rumbo north lot 6 is the Craftsman front in honey brick under clay tile. Neblina's corner house is tan stucco under a sage board-and-batten upper floor, its porch on
/// white columns.
///
/// Stucco keeps to the street's tans, peaches, apricots and pale yellows, with one white;
/// the flat tile is the dusty tan, grey-brown and brown concrete of the tract's roofs and the
/// barrel tile its muted salmon-brown clay, never red or orange, on about one lot in four,
/// so from overhead the roofscape reads as the aerial's tans and greys, their dark solar
/// arrays standing off them. A brick veneer is the tract's dark brown ([`SENDA_BRICK`]).
/// The tract's nine designs: three floor plans (small, medium, large) each built in three
/// exterior styles (Spanish, Craftsman, Monterey), in [`Design`] order. Each is drawn from a
/// Blender model where it has one ([`HOUSE_MODELS`]), fitted to its lot's size and
/// mirrored for a garage on the other side; the flags here dress the coded house that
/// stands in for it and set its roof (the eave-door hips run their ridges along the
/// street). The paints are the first of each design's three in [`DESIGN_PAINTS`].
#[rustfmt::skip]
const DESIGNS: [HouseStyle; 9] = {
    use DoorWall::{Eave as Along, Gable as Front};
    use GarageLook::{Carriage, Panels, XBrace};
    use GaragePaint::Trim;
    use RoofCover::{FlatTile, Tile};
    use RoofForm::Hip;
    use Shutters::{Board, BoardLeaf};
    use Veneer::Stone;
    use WallFinish::Stucco;
    const NONE: [u32; 4] = [0; 4];
    [
        // Small Spanish (Rumbo north lot 1): a front-gabled wing opening in an arch at its corner end
        // beside a tile-framed feature window, a front chimney, under clay tile.
        style(Hip, Front, Tile, Stucco, NONE, 0, false, Board, false, 0.7)
            .loggia()
            .solar(Solar::Small),
        // Small Craftsman (Rumbo north lot 9): a dark box bay over the garage and a porch under a
        // hip of its own on posts over stone piers, under flat tile.
        style(Hip, Along, FlatTile, Stucco, NONE, 0, true, Board, true, 0.7)
            .garage_look(Panels)
            .veneer(Stone),
        // Small Monterey (Rumbo north lots 4 and 8, south lot 4 mirrored): a clay hip whose
        // ridge runs front to back, a two-storey front gable over the garage holding a dark
        // box bay and one over the back, one board shutter, one small array; its entry porch
        // and back loggia stand before and behind it as covers of their own.
        style(Hip, Front, Tile, Stucco, NONE, 0, false, BoardLeaf, true, 0.7)
            .gable_front()
            .room_porch()
            .solar(Solar::Small),
        // Medium Spanish: the medium plan under clay tile, its porch beside the garage.
        style(Hip, Along, Tile, Stucco, NONE, 0, false, Board, true, 0.72),
        // Medium Craftsman (Rumbo north lot 2): an open porch beside the garage under a flat-tile shed
        // roof on chunky posts over stone piers, three openings across the upper floor.
        style(Hip, Along, FlatTile, Stucco, NONE, 0, false, Board, true, 0.72)
            .garage_look(Panels)
            .solar(Solar::Small),
        // Medium Monterey (Neblina's corner house): a board-and-batten upper floor over a belt course,
        // porch columns and a two-leaf carriage door, under flat tile.
        style(Hip, Along, FlatTile, Stucco, NONE, 0, false, Board, true, 0.72)
            .garage_door(Trim)
            .garage_look(Carriage)
            .batten_upper(),
        // Large Spanish (Rumbo north lots 5 and 10): a two-storey front gable over the entry side with
        // twin arched windows, a one-storey front-gabled wing before it with an arched entry,
        // a clay pent over the garage, under clay tile.
        style(Hip, Front, Tile, Stucco, NONE, 0, false, Board, false, 0.7),
        // Large Craftsman (Rumbo north lots 3, 7 and 11): a brick porch whose piers frame an arched
        // entry and a balcony over the garage, under flat tile.
        style(Hip, Along, FlatTile, Stucco, NONE, 0, false, Board, true, 0.72)
            .arched_porch()
            .balcony()
            .garage_look(XBrace)
            .veneer(Stone)
            .solar(Solar::Three),
        // Large Monterey (Rumbo north lot 6): a broad dark brick front-gable wing round the entry, a
        // shuttered window in it, a bay over the garage, under clay tile.
        style(Hip, Along, Tile, Stucco, NONE, 0, true, Board, true, 0.72),
    ]
};

/// Each design's three paint schemes: siding, trim, door and shutters, roof.
#[rustfmt::skip]
const DESIGN_PAINTS: [[[u32; 4]; 3]; 9] = [
    // Small Spanish: Rumbo north lot 1's cream, apricot, white.
    [[0xe6d9bf, 0xe0d2b6, 0x5a4636, 0x8e6c5e], [0xdcb48c, 0xe0bc98, 0x6f8a96, 0x8e6c5e], [0xece6da, 0xe4dccc, 0x5a4636, 0x94705f]],
    // Small Craftsman: sand with sage shutters, peach with burgundy, ochre with slate.
    [[0xd6c29c, 0x66523f, 0x7f8c74, 0x8a7f70], [0xdcc4a2, 0xe8dcc8, 0x6e3a36, 0x6e6458], [0xcfae78, 0x6e5644, 0x6f8a96, 0x7d7266]],
    // Small Monterey: the reference house's tan under rose-brown clay, Rumbo north lot 8's cream, peach.
    [[0xc9a57a, 0xd2b48c, 0x5f7884, 0x6b4540], [0xdccaa8, 0xe4d8c0, 0x5f7884, 0x7d6a5e], [0xdcc4a2, 0xe8dcc8, 0x6e3a36, 0x8e6c5e]],
    // Medium Spanish: apricot, white, butter.
    [[0xdcb48c, 0xe0bc98, 0x6f8a96, 0x8e6c5e], [0xece6da, 0xe4dccc, 0x5a4636, 0x94705f], [0xe8d2a4, 0xead6b0, 0x5f7884, 0x86675a]],
    // Medium Craftsman: Rumbo north lot 2's tan, tan with slate, warm sand with brown.
    [[0xcfb48a, 0xd8c4a0, 0x5f7884, 0x7a7874], [0xcfb48a, 0xd8c4a0, 0x5f7884, 0x7d7266], [0xdcc296, 0xe6d6b6, 0x5a4636, 0x8a7f70]],
    // Medium Monterey: Neblina's corner house's tan under grey, cream, honey.
    [[0xd2b896, 0xeeeae2, 0x4a3a30, 0x7a7670], [0xdccaa8, 0xe4d8c0, 0x5f7884, 0x7d7266], [0xdcbc8a, 0xe2c89c, 0x7f8c74, 0x8a7f70]],
    // Large Spanish: Rumbo north lot 5's apricot, white, Rumbo north lot 10's butter.
    [[0xdcb48c, 0xe0bc98, 0x6f8a96, 0x8e6c5e], [0xece6da, 0xe4dccc, 0x5a4636, 0x94705f], [0xe8d2a4, 0xead6b0, 0x5f7884, 0x86675a]],
    // Large Craftsman: Rumbo north lot 3's grey-beige, warm sand with brown, sand with sage.
    [[0xd4bc94, 0xdcc8a4, 0x7f8c74, 0x8a8278], [0xcdb38e, 0xd8c4a0, 0x5a4636, 0x6e6458], [0xdcc296, 0xe6d6b6, 0x7f8c74, 0x8a7f70]],
    // Large Monterey: Rumbo north lot 6's honey, peach, cream.
    [[0xdcbc8a, 0xe2c89c, 0x7f8c74, 0x94705f], [0xdcc4a2, 0xe8dcc8, 0x6e3a36, 0x8e6c5e], [0xe6d9bf, 0xe0d2b6, 0x5a4636, 0x8e6c5e]],
];

/// A design of the tract: its floor plan and exterior style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plan {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Exterior {
    Spanish,
    Craftsman,
    Monterey,
}

/// The index in [`SENDA_STYLES`] of `plan` in `exterior`, painted in its scheme `paint`
/// (0 to 2).
pub const fn senda_style(plan: Plan, exterior: Exterior, paint: usize) -> u8 {
    ((plan as usize * 3 + exterior as usize) * 3 + paint) as u8
}

/// The design of a [`SENDA_STYLES`] index.
pub const fn style_design(style: u8) -> (Plan, Exterior) {
    let design = style as usize / 3;
    let plan = match design / 3 {
        0 => Plan::Small,
        1 => Plan::Medium,
        _ => Plan::Large,
    };
    let exterior = match design % 3 {
        0 => Exterior::Spanish,
        1 => Exterior::Craftsman,
        _ => Exterior::Monterey,
    };
    (plan, exterior)
}

/// Senda's houses: each of the nine [`DESIGNS`] in its three paints, design by design
/// ([`senda_style`]).
pub const SENDA_STYLES: [HouseStyle; 27] = {
    let mut styles = [DESIGNS[0]; 27];
    let mut index = 0;
    while index < 27 {
        let [siding, trim, accent, roof] = DESIGN_PAINTS[index / 3][index % 3];
        styles[index] = HouseStyle {
            siding,
            trim,
            accent,
            roof,
            ..DESIGNS[index / 3]
        };
        index += 1;
    }
    styles
};

/// The tint of a Senda brick veneer over the brick texture: the tract's dark brown
/// brick, held off the texture's orange-red (the tint darkens red far more than green and
/// blue).
pub const SENDA_BRICK: u32 = 0x8ac0c8;

/// The reference house: the small Monterey in its own paint.
pub const REFERENCE_HOUSE: u8 = senda_style(Plan::Small, Exterior::Monterey, 0);
/// Neblina's corner house, the street's one board-and-batten house: the medium Monterey.
pub const BOARD_AND_BATTEN: u8 = senda_style(Plan::Medium, Exterior::Monterey, 0);
/// Rumbo north lot 1, on the corner at Neblina: the small Spanish.
pub const CORNER_SPANISH: u8 = senda_style(Plan::Small, Exterior::Spanish, 0);
/// Rumbo's north and south rows in `senda_houses`' rows (lots counted from Neblina),
/// and Neblina's west row (lots counted up from the bend) with the lot of its top house by
/// Senda Parkway, Neblina's corner house.
const RUMBO_NORTH_ROW: usize = 0;
const RUMBO_SOUTH_ROW: usize = 1;
const NEBLINA_WEST_ROW: usize = 3;
const NEBLINA_WEST_TOP: usize = 6;
/// Rumbo's north row from Neblina, lot by lot: the small Spanish on the corner, the
/// medium Craftsman, the large Craftsman, the reference house, the large Spanish, the
/// large Monterey, the large Craftsman again, the reference house's design in cream, the
/// small Craftsman and the large Spanish in butter. Every garage there is on the right.
const RUMBO_NORTH: [u8; 10] = {
    use Exterior::{Craftsman, Monterey, Spanish};
    use Plan::{Large, Medium, Small};
    [
        senda_style(Small, Spanish, 0),
        senda_style(Medium, Craftsman, 0),
        senda_style(Large, Craftsman, 0),
        REFERENCE_HOUSE,
        senda_style(Large, Spanish, 0),
        senda_style(Large, Monterey, 0),
        senda_style(Large, Craftsman, 1),
        senda_style(Small, Monterey, 1),
        senda_style(Small, Craftsman, 0),
        senda_style(Large, Spanish, 2),
    ]
};
/// Rumbo south lot 4, across the street from the reference house: its design mirrored, the garage on the left.
const MIRRORED_REFERENCE_LOT: usize = 3;

/// Senda lot `lot` of row `row` (as `senda_houses` counts them): its design and paint
/// in [`SENDA_STYLES`] and its garage's side. Rumbo's north row is built house by house
/// as the street stands, Rumbo south lot 4 mirrors the reference house and Neblina's corner house is the medium Monterey;
/// elsewhere neighbours differ in plan, style and paint, and pairs of lots mirror each
/// other with their garages side by side, as the builder plotted them.
pub fn senda_lot(row: usize, lot: usize) -> (u8, GarageSide) {
    if row == RUMBO_NORTH_ROW
        && let Some(&style) = RUMBO_NORTH.get(lot)
    {
        return (style, GarageSide::Right);
    }
    if row == RUMBO_SOUTH_ROW && lot == MIRRORED_REFERENCE_LOT {
        return (
            senda_style(Plan::Small, Exterior::Monterey, 2),
            GarageSide::Left,
        );
    }
    let garage = if (lot + row).is_multiple_of(2) {
        GarageSide::Left
    } else {
        GarageSide::Right
    };
    if row == NEBLINA_WEST_ROW && lot == NEBLINA_WEST_TOP {
        return (BOARD_AND_BATTEN, garage);
    }
    // The plan steps with each lot and the style with each pair, so no two neighbours share
    // a design; the paint steps every third lot.
    let plan = (lot + row) % 3;
    let exterior = (lot / 2 + row * 2) % 3;
    let paint = (lot / 3 + row) % 3;
    let style = ((plan * 3 + exterior) * 3 + paint) as u8;
    // Neblina's corner house's green board and batten is the street's only one.
    (
        if style == BOARD_AND_BATTEN {
            style + 1
        } else {
            style
        },
        garage,
    )
}

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
    /// A hip end's run (m) from the ridge's end out to its eave: the side slopes' run
    /// ([`Roof::eave_x`]) where the hips take their pitch, shorter (a steeper end) where
    /// a garage house's ridge runs along its street over a front narrower than its depth.
    pub end_run: f64,
    /// How far the eaves overhang the walls (m).
    pub overhang: f64,
    /// Two low storeys under the eaves (a garage house's, Senda's): smaller windows
    /// and the upper floor just over the doors, so the walls stay about half as high as
    /// the house is wide, as the tract's are.
    pub low_storeys: bool,
    /// The plinth's (or slab's) height, where the ground floor and its openings start.
    pub plinth: f64,
}

impl Roof {
    /// The roof over a `w` × `d` house `h` to the ridge whose walls stand `eave` of
    /// that high, its eaves and rakes overhanging the walls by `overhang`.
    pub fn new(form: RoofForm, w: f64, d: f64, h: f64, eave: f64, overhang: (f64, f64)) -> Self {
        let (eaves, rakes) = overhang;
        let wall = h * eave;
        let pitch = (h - wall) / (w / 2.0);
        let eave_x = w / 2.0 + eaves;
        let (rake_z, ridge_half) = match form {
            RoofForm::Gable => (d / 2.0 + rakes, d / 2.0 + rakes),
            // Hips take the side slopes' pitch, so the ridge is as much shorter than
            // the roof as the roof is wide.
            RoofForm::Hip => {
                let end = d / 2.0 + eaves;
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
            eave_y: wall - pitch * eaves,
            thick: ROOF_THICKNESS * (1.0 + pitch * pitch).sqrt(),
            ridge_half,
            end_run: eave_x,
            overhang: eaves,
            low_storeys: false,
            plinth: PLINTH,
        }
    }

    /// Horizontal run from the ridge line to `(x, z)` down the slope it lies on, as the
    /// side slopes measure it (a steeper hip end's counts for more).
    pub fn run(&self, x: f64, z: f64) -> f64 {
        match self.form {
            RoofForm::Gable => x.abs(),
            RoofForm::Hip => x
                .abs()
                .max((z.abs() - self.ridge_half) * (self.eave_x / self.end_run)),
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
/// A bed of beans: the height (m) its pole A-frames' tops meet at, how far out from the
/// bed's middle line their feet stand, and how thick a frame's poles bear what falls on
/// them.
pub const BEAN_POLE: f64 = 1.15;
pub const BEAN_SPREAD: f64 = 0.3;
const BEAN_FRAME: f64 = 0.12;

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
    /// It grows beans up pole A-frames (otherwise rows of cabbages).
    pub beans: bool,
}

impl GardenBed {
    /// Where along the wall a bed of beans stands its pole A-frames, each across the bed.
    pub fn bean_frames(&self) -> Vec<f64> {
        if !self.beans {
            return Vec::new();
        }
        let count = (self.length / 0.8).floor().clamp(2.0, 4.0) as usize;
        (0..count)
            .map(|k| self.u0 + 0.3 + k as f64 * (self.length - 0.9) / (count - 1) as f64)
            .collect()
    }

    /// What in the bed bears a fallen part, as convex hulls in the build frame: the plants
    /// up to `height` over the whole bed, and each bean A-frame to its top, so a wall
    /// coming down on the poles rests on them rather than through them.
    pub fn props(&self, design: &HouseDesign, height: f64) -> Vec<Vec<DVec3>> {
        let (a, b) = self.bounds(design, height);
        let mut hulls = vec![
            (0..8)
                .map(|k| {
                    DVec3::new(
                        if k & 1 == 0 { a.x } else { b.x },
                        if k & 2 == 0 { a.y } else { b.y },
                        if k & 4 == 0 { a.z } else { b.z },
                    )
                })
                .collect(),
        ];
        let (center, out, along, _) = wall_frames(design.w, design.d)[1];
        let middle = (self.near + self.far) / 2.0;
        for u in self.bean_frames() {
            let at = |du: f64, o: f64, y: f64| center + along * (u + du) + out * o + DVec3::Y * y;
            hulls.push(
                [-BEAN_FRAME / 2.0, BEAN_FRAME / 2.0]
                    .into_iter()
                    .flat_map(|du| {
                        [
                            at(du, middle - BEAN_SPREAD, 0.0),
                            at(du, middle + BEAN_SPREAD, 0.0),
                            at(du, middle, BEAN_POLE),
                        ]
                    })
                    .collect(),
            );
        }
        hulls
    }

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
    /// A two-car garage in the door wall, on this side of the door (Senda's houses).
    pub garage: Option<GarageSide>,
    /// How far (radians) a planned house turns past its quarter turns to front a
    /// curving street.
    pub angle: f64,
}

impl HouseDesign {
    /// The cottage a village cover box at `x, z` stands for, fronting its street.
    pub fn of(x: f64, z: f64, w: f64, d: f64, h: f64) -> Self {
        Self::facing(x, z, w, d, h, cottage_turns(x, z))
    }

    /// The house a cover box stands for: its map's plan where it has one, otherwise
    /// the village cottage for where it stands ([`HouseDesign::of`]).
    pub fn for_cover(x: f64, z: f64, w: f64, d: f64, h: f64, plan: Option<HousePlan>) -> Self {
        match plan {
            Some(plan) => Self::planned(x, z, w, d, h, plan),
            None => Self::of(x, z, w, d, h),
        }
    }

    /// A house built to `plan`: its type and garage chosen by the map, fronting its street.
    /// Its box is the house's own: `w` across its front, `d` from front to back,
    /// whichever way it faces.
    pub fn planned(x: f64, z: f64, w: f64, d: f64, h: f64, plan: HousePlan) -> Self {
        let style = SENDA_STYLES[usize::from(plan.style) % SENDA_STYLES.len()];
        let street = i32::from(plan.street);
        // [`HouseDesign::styled`] reads a box in world axes, which an odd quarter turn
        // swaps.
        let (w, d) = if street.rem_euclid(2) == 0 {
            (w, d)
        } else {
            (d, w)
        };
        Self {
            garage: Some(plan.garage),
            angle: f64::from(plan.angle).to_radians(),
            ..Self::styled(x, z, w, d, h, street, style)
        }
    }

    /// A cottage whose door faces `street` quarter turns (see [`door_direction`]);
    /// scenery outside the arena chooses its own.
    pub fn facing(x: f64, z: f64, w: f64, d: f64, h: f64, street: i32) -> Self {
        let variant = house_variant(x, z);
        // Mirror-image cottages take different types, so the symmetric plan does
        // not read as copies.
        let quadrant = u32::from(x > 0.0) * 2 + u32::from(z > 0.0);
        let pick = (variant / 2).wrapping_add(quadrant * 2 + 3) as usize;
        Self::styled(
            x,
            z,
            w,
            d,
            h,
            street,
            HOUSE_STYLES[pick % HOUSE_STYLES.len()],
        )
    }

    /// A house of `style` whose door faces `street` quarter turns.
    fn styled(x: f64, z: f64, w: f64, d: f64, h: f64, street: i32, style: HouseStyle) -> Self {
        let variant = house_variant(x, z);
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
            // A planned hip whose ridge runs along its street: the door in an eave wall.
            (RoofForm::Hip, DoorWall::Eave) => 2,
            // A hip ridge runs along the longer side, whichever wall the door is in.
            (RoofForm::Hip, DoorWall::Gable) => {
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
            garage: None,
            angle: 0.0,
        }
    }

    /// Its garden bed, which one cottage in three keeps (the rest stack firewood by the
    /// wall; a garage house keeps none), and the cosmetic random stream its planting
    /// goes on with.
    pub fn garden_bed(&self) -> Option<(GardenBed, Random)> {
        if !self.variant.is_multiple_of(3) || self.garage.is_some() {
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
            beans: !lot.is_multiple_of(2),
        };
        Some((bed, rng))
    }

    pub fn yaw(&self) -> f64 {
        f64::from(self.turns) * PI / 2.0 + self.angle
    }

    pub fn roof(&self) -> Roof {
        let eave = if self.storeys() == 2 {
            TWO_STOREY_EAVE
        } else {
            self.style.eave
        };
        if self.garage.is_some() {
            // Walls as high whichever way the ridge runs: under the rise a low pitch takes
            // across the front, whatever the house's height.
            let front = 2.0 * wall_frames(self.w, self.d)[self.door_wall].3;
            let eave = 1.0 - GARAGE_HOUSE_PITCH * front / 2.0 / self.h;
            let mut roof = Roof {
                low_storeys: true,
                plinth: GARAGE_HOUSE_PLINTH,
                ..Roof::new(
                    self.style.form,
                    self.w,
                    self.d,
                    self.h,
                    eave,
                    GARAGE_HOUSE_OVERHANG,
                )
            };
            if roof.form == RoofForm::Hip && self.door_wall == 2 {
                // The ridge along the street: hip ends steeper than the long slopes.
                roof.ridge_half = (roof.rake_z - ALONG_STREET_HIP_END).max(0.0);
                roof.end_run = roof.rake_z - roof.ridge_half;
            }
            return roof;
        }
        Roof::new(
            self.style.form,
            self.w,
            self.d,
            self.h,
            eave,
            (EAVE_OVERHANG, RAKE_OVERHANG),
        )
    }

    /// Floors under the roof: two on a house at least [`TWO_STOREY`] tall, and on every
    /// garage house (Senda's), whose low storeys sit under a shallow roof.
    pub fn storeys(&self) -> u32 {
        if self.h >= TWO_STOREY || self.garage.is_some() {
            2
        } else {
            1
        }
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

    /// Whether the house has a chimney: every village cottage; Senda's houses heat with
    /// gas and have none.
    pub fn chimney(&self) -> bool {
        self.garage.is_none()
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
    /// width), on the right (+x) slope over the door wall. It centres on the door (a
    /// garage front's on the room beside the door, as far as it fits) unless the
    /// chimney stands there, when it moves along clear of the stack.
    pub fn wing(&self) -> Option<Wing> {
        let roof = self.roof();
        if !self.style.wing || roof.form != RoofForm::Gable || self.door_wall != 2 {
            return None;
        }
        let half = (self.d * 0.21).clamp(1.0, WING_HALF);
        let flue = self.flue();
        let clear = half + CHIMNEY / 2.0 + 0.25;
        // The door wall's along axis runs down z.
        let room = (self.d / 2.0 - 0.6 - half).max(0.0);
        let middle = self
            .door_side()
            .map_or(0.0, |(from, to)| (-(from + to) / 2.0).clamp(-room, room));
        let z = if self.chimney() && flue.x > 0.0 && (flue.z - middle).abs() < clear {
            flue.z - (flue.z - middle).signum() * clear
        } else {
            middle
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

/// The corner posts' plan size (m): a wall's openings stand clear of them.
pub const CORNER_POST: f64 = 0.3;
/// Half widths (m) of the wall columns a window, the front door and each half of a
/// garage door take with their casings: the structure frames each column as parts of
/// its own, and the model's opening lies inside it. A cottage's, and a garage house's
/// (Senda's) at the tank's scale: windows up to 0.9 wide with their frames, a 0.5 m
/// door with its casing and a 2.6 m two-car door in two columns.
pub const WINDOW_BAY: f64 = 0.57;
pub const DOOR_BAY: f64 = 0.62;
pub const LOW_WINDOW_BAY: f64 = 0.5;
pub const LOW_DOOR_BAY: f64 = 0.36;
pub const GARAGE_BAY: f64 = 0.7;
/// Room (m) a narrow garage front keeps between its front door's column and its garage
/// door's, so neither runs into the other.
const GARAGE_DOOR_GAP: f64 = 0.02;
/// A ground-floor window's sill over the plinth, and the front door's height (m): a
/// cottage's, and a garage house's under its two low storeys.
pub const SILL: f64 = 0.9;
pub const DOOR_HEIGHT: f64 = 2.0;
pub const LOW_DOOR_HEIGHT: f64 = 1.1;
/// A two-car garage door's height (m), standing on the slab like the front door and
/// lower than it, and how much narrower it is than its two columns (its casing): as wide
/// as the front leaves it, up to [`GARAGE_BAY`] a column: about two fifths of a full front,
/// as the tract's two-car doors are, leaving the entry wing beside it nearly half.
pub const GARAGE_HEIGHT: f64 = 1.15;
pub const GARAGE_CASING: f64 = 0.2;
/// A two-storey house's floors: how far over the ground-floor windows' heads its
/// belt course and upper floor run, how far over that the upper windows' sills sit
/// and the tallest they get (m).
const BELT_OVER: f64 = 0.3;
const UPPER_SILL: f64 = 0.35;
const UPPER_WINDOW: f64 = 1.2;
/// Two low storeys ([`Roof::low_storeys`]): a ground-floor window's sill over the slab
/// (its column's head level with the garage door's, the taller, so the plates over the
/// ground floor's windows and doors run at one level), how far the upper floor runs over
/// that, how far over the floor the upper windows' sills sit, the tallest they get and
/// the wall they keep under the eave (m): two 1.6 m storeys (the tract's 3 m), sills
/// about 0.5 and 2.1 over the ground, windows up to 0.77 high, and the tract's upper
/// windows well down from the eaves, under a band of plain stucco.
const LOW_SILL: f64 = 0.38;
const LOW_FLOOR_OVER: f64 = 0.35;
const LOW_UPPER_SILL: f64 = 0.48;
const LOW_UPPER_WINDOW: f64 = 0.72;
const LOW_EAVE_WALL: f64 = 0.3;
/// The wall (m) a garage front keeps between the corner post and the garage door's
/// column (where it has room: none, the garage door filling its half of the front as
/// the tract's do), between that and the front door's, between the front door's and its
/// corner post at least, and the least it keeps beside a window in the room by the door.
const GARAGE_PIER: f64 = 0.0;
const DOOR_PIER: f64 = 0.3;
const DOOR_CORNER_PIER: f64 = 0.35;
const WINDOW_PIER: f64 = 0.15;
/// How far either side of the garage door's middle the pair of windows ganged over it
/// stand (m): together they read as one wide window. A spread front's pair over the
/// front door stands closer, both over the door's column.
pub const GARAGE_PAIR: f64 = 0.4;
pub const DOOR_PAIR: f64 = 0.3;

/// What an opening in a wall's ground floor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum OpeningKind {
    Window,
    Door,
    /// A two-car garage door: two columns of the wall wide.
    Garage,
}

/// A wall's openings, as the model cuts them and the structure frames them: the
/// ground floor's centres along the wall frame's along axis with what each is, and a
/// two-storey house's upper windows over them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct WallPlan {
    pub ground: Vec<(f64, OpeningKind)>,
    pub upper: Vec<f64>,
    /// Half the width of each of the garage door's two columns (none without one).
    pub garage_bay: f64,
}

impl WallPlan {
    /// The front door's centre along the wall, if the wall has it.
    pub fn door(&self) -> Option<f64> {
        self.opening(OpeningKind::Door)
    }

    /// The garage door's centre along the wall, if the wall has it.
    pub fn garage(&self) -> Option<f64> {
        self.opening(OpeningKind::Garage)
    }

    /// The garage door's width.
    pub fn garage_width(&self) -> f64 {
        4.0 * self.garage_bay - GARAGE_CASING
    }

    fn opening(&self, kind: OpeningKind) -> Option<f64> {
        self.ground
            .iter()
            .find(|(_, what)| *what == kind)
            .map(|&(u, _)| u)
    }

    /// The ground floor's window centres, in the order the model dresses them.
    pub fn windows(&self) -> impl Iterator<Item = f64> + '_ {
        self.ground
            .iter()
            .filter(|(_, what)| *what == OpeningKind::Window)
            .map(|&(u, _)| u)
    }
}

/// A house type drawn from a Blender model (`assets/blender/`): the name of its runtime
/// file (`models/houses/<name>.glb`), the Senda styles it draws and the wall plan its
/// script wrote beside the model. The plan is gameplay data (it frames the structure's
/// window, door and garage parts), so it is built in; the model is only looks.
pub struct HouseModel {
    pub name: &'static str,
    pub styles: &'static [u8],
    plan: &'static str,
}

/// Senda's houses drawn from Blender models.
pub const HOUSE_MODELS: &[HouseModel] = {
    use Exterior::{Craftsman, Monterey, Spanish};
    use Plan::{Large, Medium, Small};
    &[
        // The small Monterey, after the reference house.
        HouseModel {
            name: "small-monterey",
            styles: &design_styles(Small, Monterey),
            plan: include_str!("../../data/houses/small-monterey.plan.json"),
        },
        // The large Spanish, after Rumbo north lot 5.
        HouseModel {
            name: "spanish",
            styles: &design_styles(Large, Spanish),
            plan: include_str!("../../data/houses/spanish.plan.json"),
        },
        // The small Craftsman, after Rumbo north lot 9.
        HouseModel {
            name: "craftsman",
            styles: &design_styles(Small, Craftsman),
            plan: include_str!("../../data/houses/craftsman.plan.json"),
        },
        // The medium Craftsman, after Rumbo north lot 2.
        HouseModel {
            name: "prairie",
            styles: &design_styles(Medium, Craftsman),
            plan: include_str!("../../data/houses/prairie.plan.json"),
        }, // The small Spanish, after Rumbo north lot 1.
        HouseModel {
            name: "small-spanish",
            styles: &design_styles(Small, Spanish),
            plan: include_str!("../../data/houses/small-spanish.plan.json"),
        },
        // The medium Spanish, the medium plan in the Spanish style.
        HouseModel {
            name: "medium-spanish",
            styles: &design_styles(Medium, Spanish),
            plan: include_str!("../../data/houses/medium-spanish.plan.json"),
        },
        // The medium Monterey, after Neblina's corner house.
        HouseModel {
            name: "medium-monterey",
            styles: &design_styles(Medium, Monterey),
            plan: include_str!("../../data/houses/medium-monterey.plan.json"),
        },
        // The large Craftsman, after Rumbo north lots 3, 7 and 11.
        HouseModel {
            name: "large-craftsman",
            styles: &design_styles(Large, Craftsman),
            plan: include_str!("../../data/houses/large-craftsman.plan.json"),
        },
        // The large Monterey, after Rumbo north lot 6.
        HouseModel {
            name: "large-monterey",
            styles: &design_styles(Large, Monterey),
            plan: include_str!("../../data/houses/large-monterey.plan.json"),
        },
    ]
};

/// A design's three [`SENDA_STYLES`] indices, one for each of its paints.
const fn design_styles(plan: Plan, exterior: Exterior) -> [u8; 3] {
    [
        senda_style(plan, exterior, 0),
        senda_style(plan, exterior, 1),
        senda_style(plan, exterior, 2),
    ]
}

/// A model's wall plans as its script wrote them, in [`wall_frames`] order, with the
/// measures the model was drawn at: its walls' box, wall top and ridge, and which side
/// its garage stands on.
#[derive(Deserialize)]
pub struct AuthoredPlan {
    pub w: f64,
    pub d: f64,
    pub wall: f64,
    pub ridge: f64,
    pub garage: GarageSide,
    /// The wall its door is in, in [`wall_frames`] order: the front (0) or, for a hip whose
    /// ridge runs along the street, the eave wall (2).
    #[serde(default)]
    pub door_wall: usize,
    /// The box of the single-storey office and entry before its front, which the level
    /// stands as a cover of its own (`sim::senda_entries`).
    #[serde(default)]
    pub entry: Option<EntryBox>,
    walls: [WallPlan; 4],
}

/// A model's office-and-entry box in the build frame it was drawn in (m): its centre on
/// the ground, its extent along x and z and its height. It runs from the house's front
/// wall out to the office's front, over the enclosed room and the porch bay beside it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct EntryBox {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
}

static AUTHORED: std::sync::OnceLock<Vec<AuthoredPlan>> = std::sync::OnceLock::new();

impl AuthoredPlan {
    /// Its wall plan `index` as written (the tests compare a lot's plan with it).
    #[cfg(test)]
    pub fn walls_for_test(&self, index: usize) -> WallPlan {
        self.walls[index].clone()
    }
}

impl HouseModel {
    /// The plan its script wrote.
    pub fn authored(&self) -> &'static AuthoredPlan {
        let index = HOUSE_MODELS
            .iter()
            .position(|model| model.name == self.name)
            .expect("a listed model");
        &AUTHORED.get_or_init(|| {
            HOUSE_MODELS
                .iter()
                .map(|model| serde_json::from_str(model.plan).expect("an authored plan"))
                .collect()
        })[index]
    }
}

/// How a model drawn for one lot fits another: mirrored when the lot's garage stands on
/// the other side, its walls stretched along x and z to the lot's box, and heights
/// mapped piecewise so its wall top and ridge land on the lot's (the roof keeps its
/// planes, the walls stay upright).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelFit {
    pub mirror: bool,
    /// The mirror runs along z, the front wall's axis on a house whose door is in its eave
    /// wall (else along x).
    pub mirror_z: bool,
    pub scale_x: f64,
    pub scale_z: f64,
    /// The model's wall top and ridge, and the lot's.
    pub from: (f64, f64),
    pub to: (f64, f64),
}

impl ModelFit {
    /// Whether the model is drawn as it is.
    pub fn identity(&self) -> bool {
        !self.mirror
            && (self.scale_x - 1.0).abs() < 1e-9
            && (self.scale_z - 1.0).abs() < 1e-9
            && (self.from.0 - self.to.0).abs() < 1e-9
            && (self.from.1 - self.to.1).abs() < 1e-9
    }

    /// The height scale at height `y` of the model: under its wall top, and over it.
    pub fn height_scale(&self, y: f64) -> f64 {
        if y <= self.from.0 {
            self.to.0 / self.from.0
        } else {
            (self.to.1 - self.to.0) / (self.from.1 - self.from.0)
        }
    }

    /// A point of the model where it stands on the lot.
    pub fn point(&self, p: DVec3) -> DVec3 {
        let y = if p.y <= self.from.0 {
            p.y * self.height_scale(p.y)
        } else {
            self.to.0 + (p.y - self.from.0) * self.height_scale(p.y)
        };
        self.mirrored(DVec3::new(p.x * self.scale_x, y, p.z * self.scale_z))
    }

    /// A normal of the model at height `y`, fitted (the inverse transpose of the stretch).
    pub fn normal(&self, n: DVec3, y: f64) -> DVec3 {
        let n = DVec3::new(
            n.x / self.scale_x,
            n.y / self.height_scale(y),
            n.z / self.scale_z,
        );
        self.mirrored(n).normalize_or_zero()
    }

    /// `v` across the mirror, when there is one.
    fn mirrored(&self, v: DVec3) -> DVec3 {
        match (self.mirror, self.mirror_z) {
            (false, _) => v,
            (true, false) => DVec3::new(-v.x, v.y, v.z),
            (true, true) => DVec3::new(v.x, v.y, -v.z),
        }
    }

    /// The model's wall drawn as the lot's wall `index` (a mirror swaps the walls across
    /// it: the flanks, or front and back on an eave-door house).
    pub fn source_wall(&self, index: usize) -> usize {
        match (self.mirror, self.mirror_z) {
            (false, _) => index,
            (true, false) => [0, 1, 3, 2][index],
            (true, true) => [1, 0, 2, 3][index],
        }
    }

    /// The lot's wall plan `index` from the model's: centres mirrored and stretched with
    /// the wall, the garage's leaves with it.
    fn plan(&self, walls: &[WallPlan; 4], index: usize) -> WallPlan {
        let wall = &walls[self.source_wall(index)];
        let scale = if index < 2 {
            self.scale_x
        } else {
            self.scale_z
        };
        let along = |u: f64| if self.mirror { -u * scale } else { u * scale };
        WallPlan {
            ground: wall
                .ground
                .iter()
                .map(|&(u, kind)| (along(u), kind))
                .collect(),
            upper: wall.upper.iter().map(|&u| along(u)).collect(),
            garage_bay: wall.garage_bay * scale,
        }
    }
}

/// The Blender model `design` is drawn from and how it fits its lot, when its style has one.
pub fn house_model(design: &HouseDesign) -> Option<(&'static HouseModel, ModelFit)> {
    let model = HOUSE_MODELS.iter().find(|model| {
        model
            .styles
            .iter()
            .any(|&style| SENDA_STYLES[usize::from(style)] == design.style)
    })?;
    let garage = design.garage?;
    let authored = model.authored();
    let roof = design.roof();
    Some((
        model,
        ModelFit {
            mirror: garage != authored.garage,
            mirror_z: design.door_wall == 2,
            scale_x: design.w / authored.w,
            scale_z: design.d / authored.d,
            from: (authored.wall, authored.ridge),
            to: (roof.wall, roof.ridge),
        },
    ))
}

impl Roof {
    /// A ground-floor window's sill height and its height (m).
    pub fn ground_window(&self) -> (f64, f64) {
        if self.low_storeys {
            let sill = self.plinth + LOW_SILL;
            let head = self.door_height().max(GARAGE_HEIGHT);
            return (sill, self.plinth + head - sill);
        }
        (PLINTH + SILL, (self.wall - PLINTH - 1.25).min(1.3))
    }

    /// Half the width of a window's column and the front door's (m): a garage house's
    /// smaller openings take narrower ones.
    pub fn window_bay(&self) -> f64 {
        if self.low_storeys {
            LOW_WINDOW_BAY
        } else {
            WINDOW_BAY
        }
    }

    pub fn door_bay(&self) -> f64 {
        if self.low_storeys {
            LOW_DOOR_BAY
        } else {
            DOOR_BAY
        }
    }

    /// The front door's height (m).
    pub fn door_height(&self) -> f64 {
        if self.low_storeys {
            LOW_DOOR_HEIGHT
        } else {
            DOOR_HEIGHT
        }
    }

    /// Where a two-storey house's belt course runs and its upper floor lies (m): low
    /// storeys' just over the doors.
    pub fn belt_line(&self) -> f64 {
        let (sill, height) = self.ground_window();
        if self.low_storeys {
            return (sill + height).max(self.plinth + GARAGE_HEIGHT) + LOW_FLOOR_OVER;
        }
        sill + height + BELT_OVER
    }

    /// An upper-floor window's sill height and its height (m), between the belt course
    /// and the eave.
    pub fn upper_window(&self) -> (f64, f64) {
        if self.low_storeys {
            let sill = self.belt_line() + LOW_UPPER_SILL;
            return (
                sill,
                (self.wall - LOW_EAVE_WALL - sill).min(LOW_UPPER_WINDOW),
            );
        }
        let sill = self.belt_line() + UPPER_SILL;
        (sill, (self.wall - 0.35 - sill).min(UPPER_WINDOW))
    }
}

impl HouseDesign {
    /// Wall `index`'s openings (in [`wall_frames`] order). A village cottage lights each
    /// wall with [`window_columns`] round a door in the middle of its door wall; a
    /// garage front ([`garage_front`]) holds the garage door, the front door and a
    /// window beside it.
    pub fn wall_plan(&self, index: usize) -> WallPlan {
        let Some((model, fit)) = house_model(self) else {
            return self.coded_wall_plan(index);
        };
        let walls = &model.authored().walls;
        if index != self.door_wall {
            return fit.plan(walls, index);
        }
        // The front keeps the coded plan on every lot: its door and garage leave the
        // piers the structure needs however narrow the lot, and the street's layout reads
        // them. The model adds the upper windows the coded front leaves out, each at its
        // offset over the opening below it (an entry tower's arched pair over the door).
        let mut front = self.coded_wall_plan(index);
        let authored = &walls[fit.source_wall(index)];
        let flip = if fit.mirror { -1.0 } else { 1.0 };
        for &u in &authored.upper {
            let Some(&(under, kind)) = authored.ground.iter().find(|&&(at, kind)| {
                let reach = match kind {
                    OpeningKind::Window => LOW_WINDOW_BAY,
                    OpeningKind::Door => LOW_DOOR_BAY,
                    OpeningKind::Garage => 2.0 * authored.garage_bay,
                };
                (u - at).abs() < reach
            }) else {
                continue;
            };
            let Some(&(over, _)) = front.ground.iter().find(|(_, what)| *what == kind) else {
                continue;
            };
            let at = over + flip * (u - under);
            if front
                .upper
                .iter()
                .all(|upper| (upper - at).abs() > LOW_WINDOW_BAY)
            {
                front.upper.push(at);
            }
        }
        front
    }

    /// A wall's openings as the coded house lays them out (a model's front starts from
    /// them).
    pub fn coded_wall_plan(&self, index: usize) -> WallPlan {
        let half = wall_frames(self.w, self.d)[index].3;
        let door = index == self.door_wall;
        let two_storey = self.storeys() == 2;
        if let (true, Some(garage)) = (door, self.garage) {
            return garage_front(
                half,
                garage,
                two_storey,
                self.entry_tower(),
                self.spread_upper(),
            );
        }
        let gable = self.style.form == RoofForm::Gable && index < 2;
        let back = index == if self.door_wall == 0 { 1 } else { 3 };
        let columns = if self.garage.is_some() && !back {
            // A garage house's flanks are plain, lit by one small window a floor toward
            // the back, as the tract's side walls face the neighbour's a side yard away.
            vec![-half * 0.3 * if index == 2 { -1.0 } else { 1.0 }]
        } else {
            window_columns(door, gable, half)
        };
        let mut ground: Vec<(f64, OpeningKind)> = columns
            .into_iter()
            .map(|u| (u, OpeningKind::Window))
            .collect();
        if door {
            ground.push((0.0, OpeningKind::Door));
        }
        // The upper floor lights each ground-floor window's bay and the stair over the
        // door.
        let upper = if two_storey {
            ground.iter().map(|&(u, _)| u).collect()
        } else {
            Vec::new()
        };
        WallPlan {
            ground,
            upper,
            garage_bay: 0.0,
        }
    }

    /// Whether the front door stands in a one-storey entry whose gable rises over where
    /// the window over the door would be: a Spanish garage house's arched entry or a
    /// Craftsman's brick front-gable wing (`wing`), under two low storeys whose upper
    /// floor starts just over the doors.
    pub fn entry_tower(&self) -> bool {
        self.garage.is_some() && (!self.style.porch || self.style.wing) && self.roof().low_storeys
    }

    /// Whether a box bay stands over the garage (a Craftsman house's), three narrow
    /// windows ganged in its face.
    pub fn box_bay(&self) -> bool {
        self.garage.is_some() && self.style.porch && self.style.wing
    }

    /// Whether the upper floor of a garage front spreads three openings across it, as a
    /// Prairie house's does with nothing standing out over its garage: a pair over the
    /// porch, a single in the middle and a pair over the garage.
    pub fn spread_upper(&self) -> bool {
        let style = self.style;
        self.garage.is_some()
            && style.porch
            && !style.wing
            && !style.balcony
            && !style.gable_front
            && !style.arched_porch
    }

    /// The garage front's room beside the front door, from the corner to the garage
    /// door's column, as `(from, to)` along the door wall; none on a cottage.
    pub fn door_side(&self) -> Option<(f64, f64)> {
        let garage = self.garage?;
        let plan = self.wall_plan(self.door_wall);
        let half = wall_frames(self.w, self.d)[self.door_wall].3;
        let s = garage.sign();
        let inner = plan.garage()? - s * 2.0 * plan.garage_bay;
        let corner = -s * half;
        Some((corner.min(inner), corner.max(inner)))
    }
}

/// A garage front `half` long either way, its garage toward `garage`: the garage door's
/// two columns a pier in from the corner post, the front door a pier past them and, where
/// the room beside the door leaves space, a window there; upstairs (`two_storey`) one
/// window over the garage (a pair ganged in the middle, one over each leaf of the garage
/// door, which the structure frames apart) and one over the door, but over the door only
/// where no entry tower (`tower`) stands in front: the tract's fronts are mostly plain
/// stucco, two or three openings across a whole upper floor. A spread front (`spread`)
/// has three: a pair over the door, a single over the garage's inner leaf and a pair over
/// its outer one.
fn garage_front(
    half: f64,
    garage: GarageSide,
    two_storey: bool,
    tower: bool,
    spread: bool,
) -> WallPlan {
    let s = garage.sign();
    let end = half - CORNER_POST;
    // The garage door takes what the front door's column and the pier by its corner leave,
    // up to its widest.
    let garage_bay = ((2.0 * end - 2.0 * LOW_DOOR_BAY - DOOR_CORNER_PIER - GARAGE_DOOR_GAP) / 4.0)
        .min(GARAGE_BAY);
    // A narrow front puts its garage against the corner post, keeping a pier by the door;
    // a narrower one (Neblina's west row) sets the door against the garage door, still
    // keeping the pier by the corner that frames the door apart from the post.
    let openings = 4.0 * garage_bay + 2.0 * LOW_DOOR_BAY;
    let garage_pier = (2.0 * end - openings - DOOR_PIER - DOOR_CORNER_PIER).clamp(0.0, GARAGE_PIER);
    let door_pier = (2.0 * end - openings - DOOR_CORNER_PIER).clamp(0.0, DOOR_PIER);
    let corner_pier = (2.0 * end - openings - door_pier).clamp(0.0, DOOR_CORNER_PIER);
    let garage_u = s * (end - garage_pier - 2.0 * garage_bay);
    // The door stays inside the wall however short it is.
    let door_reach = (s * garage_u - 2.0 * garage_bay - door_pier - LOW_DOOR_BAY)
        .max(LOW_DOOR_BAY + corner_pier - end);
    let door_u = s * door_reach;
    let mut ground = vec![(door_u, OpeningKind::Door), (garage_u, OpeningKind::Garage)];
    let mut upper = if spread && !tower {
        let outer = garage_u + s * garage_bay;
        vec![
            door_u - DOOR_PAIR,
            door_u + DOOR_PAIR,
            garage_u - s * garage_bay,
            outer - GARAGE_PAIR,
            outer + GARAGE_PAIR,
        ]
    } else {
        vec![door_u, garage_u - GARAGE_PAIR, garage_u + GARAGE_PAIR]
    };
    // The room between the door's column and the far corner.
    let (near, far) = (-(door_reach - LOW_DOOR_BAY), end);
    if far - near >= 2.0 * (LOW_WINDOW_BAY + WINDOW_PIER) {
        let window_u = -s * (near + far) / 2.0;
        ground.insert(0, (window_u, OpeningKind::Window));
    }
    if tower {
        upper.retain(|&u| u != door_u);
    }
    if !two_storey {
        upper.clear();
    }
    WallPlan {
        ground,
        upper,
        garage_bay,
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

    /// A garage front holds the garage on its side, the front door beside it and, on a
    /// wide enough front, a window past the door, their columns apart and inside the corner posts, and upstairs a
    /// window over each column; every type fronts its street.
    #[test]
    fn a_garage_front_lays_out_its_garage_door_and_window_apart() {
        for style in 0..SENDA_STYLES.len() as u8 {
            for garage in [GarageSide::Left, GarageSide::Right] {
                for (street, (w, d, windows)) in [0, 1, 2, 3].into_iter().flat_map(|street| {
                    // A narrow front leaves no room for a window by the door; the narrow
                    // rows' and the level's own fronts do.
                    [(5.0, 7.0, 0), (5.8, 7.4, 1), (6.2, 8.0, 1)].map(|lot| (street, lot))
                }) {
                    let plan = HousePlan {
                        angle: 0,
                        style,
                        street,
                        garage,
                    };
                    let design = HouseDesign::planned(8.0, -30.0, w, d, 7.0, plan);
                    // A style drawn from a Blender model keeps this front and adds the
                    // model's own upper windows (imported_house's tests check those).
                    let modelled = house_model(&design).is_some();
                    let door = design.to_world(if design.door_wall == 0 {
                        DVec3::Z
                    } else {
                        DVec3::X
                    });
                    let toward = door_direction(street.into());
                    assert!((door.x - toward.x).abs() < 1e-9 && (door.z - toward.y).abs() < 1e-9);
                    let front = design.wall_plan(design.door_wall);
                    let half = wall_frames(design.w, design.d)[design.door_wall].3;
                    let column = |kind: OpeningKind| match kind {
                        OpeningKind::Window => LOW_WINDOW_BAY,
                        OpeningKind::Door => LOW_DOOR_BAY,
                        OpeningKind::Garage => 2.0 * front.garage_bay,
                    };
                    let mut spans: Vec<(f64, f64)> = front
                        .ground
                        .iter()
                        .map(|&(u, kind)| (u - column(kind), u + column(kind)))
                        .collect();
                    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
                    assert_eq!(spans.len(), 2 + windows, "{plan:?} {w}");
                    assert!(
                        spans[0].0 >= -half + CORNER_POST
                            && spans[spans.len() - 1].1 <= half - CORNER_POST
                    );
                    assert!(
                        spans.windows(2).all(|pair| pair[0].1 <= pair[1].0),
                        "{spans:?}"
                    );
                    let garage_u = front.garage().expect("a garage");
                    assert!(
                        garage_u * garage.sign() > 0.0
                            && front.door().unwrap() * garage.sign() < 0.0
                    );
                    // One window over the garage, and one over the door unless an entry
                    // tower stands where it would be; a spread front a pair over the
                    // door, a single and a pair over the garage, all inside the corners.
                    if modelled {
                        // The model keeps the coded front's upper windows and only adds.
                        let coded = design.coded_wall_plan(design.door_wall);
                        assert!(
                            coded.upper.iter().all(|u| front.upper.contains(u)),
                            "{coded:?} {front:?}"
                        );
                    } else if design.spread_upper() {
                        assert_eq!(front.upper.len(), 5);
                        assert!(front.upper.contains(&(front.door().unwrap() + DOOR_PAIR)));
                    } else {
                        let tower = usize::from(design.entry_tower());
                        assert_eq!(front.upper.len(), 3 - tower);
                        assert!(front.upper.contains(&(garage_u + GARAGE_PAIR)));
                    }
                    assert!(front.upper.iter().all(|u| u.abs() < half - CORNER_POST));
                }
            }
        }
        // The village's cottages keep their windows round a door in the middle.
        for (x, z, w, d, h) in village() {
            let design = HouseDesign::of(x, z, w, d, h);
            let plan = design.wall_plan(design.door_wall);
            assert_eq!(plan.door(), Some(0.0));
            assert!(plan.garage().is_none() && design.door_side().is_none());
        }
    }

    /// The fourth house on Rumbo's north side from Neblina is the reference house,
    /// after the Spanish corner house, the shed porch and the brick arched porch, before
    /// the Spanish front; every garage on the right.
    #[test]
    fn rumbo_north_stands_as_the_street_does() {
        let row = |lot| senda_lot(RUMBO_NORTH_ROW, lot);
        assert_eq!(row(0), (CORNER_SPANISH, GarageSide::Right));
        assert_eq!(row(3), (REFERENCE_HOUSE, GarageSide::Right));
        let style = |lot| SENDA_STYLES[usize::from(row(lot).0)];
        let corner = style(0);
        assert!(corner.loggia && corner.cover == RoofCover::Tile && !corner.porch);
        let home = style(3);
        assert!(home.gable_front && home.room_porch && home.cover == RoofCover::Tile);
        assert!(style(1).porch && !style(1).arched_porch);
        assert!(style(2).arched_porch && style(2).balcony);
        assert_eq!(style(4).cover, RoofCover::Tile);
        assert!(!style(4).porch && !style(4).loggia);
        // Rumbo north lot 6's brick front-gable wing under clay, and Rumbo north lot 10's Spanish front.
        assert!(style(5).wing && style(5).porch && style(5).cover == RoofCover::Tile);
        assert_eq!(style(5).veneer, Veneer::Brick);
        // One-piece garage doors but for lot 3's X braces: boards on the reference house.
        assert_eq!(style(2).garage_look, GarageLook::XBrace);
        assert_eq!(home.garage_look, GarageLook::Boards);
        assert!(!style(9).porch && style(9).cover == RoofCover::Tile && !style(9).loggia);
        // Their own arrays: small ones on lots 1, 2 and 4, three blocks on lot 3.
        assert_eq!(
            [0, 1, 2, 3].map(|lot| style(lot).solar),
            [Solar::Small, Solar::Small, Solar::Three, Solar::Small]
        );
        assert_eq!(RUMBO_NORTH.len(), 10);
        // The street's one green house stands only at the top of Neblina's west row.
        for row in 0..7 {
            for lot in 0..12 {
                let green = senda_lot(row, lot).0 == BOARD_AND_BATTEN;
                assert_eq!(green, (row, lot) == (NEBLINA_WEST_ROW, NEBLINA_WEST_TOP));
            }
        }
    }

    /// A planned hip roof over a front narrower than the house is deep runs its ridge
    /// along the street, so the street sees a long eave under it; its walls stand as
    /// high as a front-facing hip's, and its roof meets its eaves all round.
    #[test]
    fn an_along_street_hip_shows_its_ridge_to_the_street() {
        let plan = |style| HousePlan {
            style,
            street: 0,
            angle: 0,
            garage: GarageSide::Left,
        };
        let along = HouseDesign::planned(
            0.0,
            0.0,
            6.2,
            8.0,
            4.5,
            plan(senda_style(Plan::Small, Exterior::Craftsman, 0)),
        );
        let facing = HouseDesign::planned(
            0.0,
            0.0,
            6.2,
            8.0,
            4.5,
            plan(senda_style(Plan::Large, Exterior::Spanish, 0)),
        );
        assert_eq!(along.door_wall, 2);
        assert_eq!(facing.door_wall, 0);
        let roof = along.roof();
        assert!((roof.wall - facing.roof().wall).abs() < 1e-9);
        // The ridge runs along the front (the build frame's z), most of its width.
        assert!(roof.ridge_half * 2.0 > 0.4 * 2.0 * roof.rake_z, "{roof:?}");
        for (x, z) in [
            (roof.eave_x, 0.0),
            (0.0, roof.rake_z),
            (roof.eave_x, roof.rake_z),
        ] {
            assert!((roof.top(x, z) - roof.thick - roof.eave_y).abs() < 1e-9);
        }
        // Its front faces the street.
        let door = along.to_world(DVec3::X);
        assert!((door.z - 1.0).abs() < 1e-9, "{door:?}");
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
