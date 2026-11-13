# The reference house (Rumbo north lot 4), modelled for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/small-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:small-monterey writes public/models/houses/small-monterey.glb and its plan,
#   crates/core/data/houses/small-monterey.plan.json)
#
# The house is authored in the game's build frame for its lot, which is the street frame
# here (metres: front +z, the garage side +x, y up; walls x +-3.1, z +-4.0, wall top 3.2,
# ridge 4.5): the tract's 11.7 x 15 m two-storey box at the level's scale. Every object is
# named <HousePart>__<Surface> after the coded house's pieces (crates/core/src/sim/
# house_layout.rs HousePart, building_kit.rs Surface) so the structural house cuts it the
# way it cuts the coded meshes: walls by side with an inner lining, the gables' walls in
# their walls' pieces, and the roof (a side gable with two cross gables, as the drone's
# views from above show it) in the coded front-to-back hip's four slopes, each piece in
# the slope it lies over.
#
# The openings are declared once, in PLAN, in the game's wall-plan form (each wall's
# ground-floor openings by centre along the wall and kind, the upper windows over them):
# the model cuts its windows there and the script writes PLAN for the game, whose
# structure frames its window, door and garage parts from it (HouseDesign::wall_plan).
# That file is gameplay data: it shapes the house's colliders.
#
# Its office and entry are the model's `Entry`, a cover of their own before the front in
# the street's layout; the loggia behind it is one too, drawn by the game, not this model.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded plan, which the street's layout reads (the
# path and mailbox find the door): the window and the door in the entry's half (the
# passage's front door stands for the window, the office's back for the door), the
# garage's two leaves, the window over the door and the box bay's ganged pair over the
# garage, and the model's own: the second small window over the office and the
# shuttered one on the gable. Two windows a floor down each flank, three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.66, 1.0, 1.8, -1.91, 0.2], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.4, "Window"], [0.55, "Window"], [2.05, "Window"]], "upper": [-1.4, 0.55, 2.05], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.9, "Window"]], "upper": [-2.3, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-2.0, "Window"], [1.7, "Window"]], "upper": [-2.0, 1.7], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
garage_u = _front["ground"][2][0]
GABLE_X = garage_u
gable_left = GABLE_X - (HALF_W - GABLE_X)  # the garage gable's wall, from here to the corner

# ---------------------------------------------------------------- the garage gable's face
# Over 60% of the front: the sectional garage door under a dark header, a lantern and the
# house number beside it; upstairs one window with one teal shutter on its left, and the
# dark box bay of two tall windows over a board-and-batten apron on three corbels.
GARAGE = garage_opening(garage_u)
BAY = (garage_u - 0.78, garage_u + 0.78)
BAY_HOLE = (BAY[0] + 0.05, UPPER_GLASS[0] - 0.08, BAY[1] - 0.05, BAY_HI - 0.05)
SINGLE = (0.2 - 0.21, 0.2 + 0.21)
holes = [GARAGE, BAY_HOLE, (SINGLE[0], UPPER_GLASS[0], SINGLE[1], UPPER_GLASS[1])]
wall("FrontWall", front, fx(gable_left), 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
window("FrontWall", front, fx(SINGLE[0]), UPPER_GLASS[0], fx(SINGLE[1]), UPPER_GLASS[1], mullion=False)
front.block("FrontWall__Trim", fx(SINGLE[0]), 2.45, fx(SINGLE[1]), 2.48, 0.1, 0.07)
front.block("FrontWall__Wood", fx(SINGLE[0]) - 0.08, UPPER_GLASS[1] + 0.05, fx(SINGLE[1]) + 0.08, UPPER_GLASS[1] + 0.11, 0.0, -0.025)
shutter("FrontWall", front, fx(SINGLE[0]) - SHUTTER_WIDTH - 0.02, UPPER_GLASS[0] - 0.01, fx(SINGLE[0]) - 0.02, UPPER_GLASS[1] + 0.01)
sectional_garage_door(front, garage_u, decor="handles")
lantern(front, GARAGE[2] + 0.17)
front.block("FrontWall__Trim", fx(GARAGE[2]) + 0.11, 0.78, fx(GARAGE[2]) + 0.23, 0.86, 0.0, -0.02)
box_bay(front, BAY[0], BAY[1], flat=False)

# ---------------------------------------------------------------- the roof's planes
# The drone's views from above settle the roof: a clay side gable whose ridge runs along
# the street a little behind the middle, its long street slope carrying the array, a
# gable end on each flank under a rake of barge tiles; and two cross gables on the
# garage's side, one over the garage (its ridge running back into the street slope, a
# valley either side of it) and one over the back, lower than the main ridge. Each plane
# is y = a + bx * x + bz * z.
RIDGE_Z = -0.9  # the main ridge's z: behind the middle, as the views from above have it
UPPER_SET_BACK = 1.0  # the upper storey over the office, back from the gable's face
upper_z = HALF_D - UPPER_SET_BACK
front_eave_z = upper_z + OVERHANG
FRONT_PITCH = (RIDGE_Y - EAVE_Y) / (front_eave_z - RIDGE_Z)  # the long street slope's
BACK_PITCH = (RIDGE_Y - EAVE_Y) / (EAVE_Z + RIDGE_Z)
GABLE_APEX = EAVE_Y + PITCH * (EAVE_X - GABLE_X)  # the garage gable's, under the ridge
# The back gable is steeper: from above its ridge runs back nearly to the main one.
BACK_GABLE_APEX = RIDGE_Y - 0.25
BACK_GABLE_PITCH = (BACK_GABLE_APEX - EAVE_Y) / (EAVE_X - GABLE_X)
gable_eave_x = 2 * GABLE_X - EAVE_X
STREET_SLOPE = (RIDGE_Y + FRONT_PITCH * RIDGE_Z, 0.0, -FRONT_PITCH)
BACK_SLOPE = (RIDGE_Y - BACK_PITCH * RIDGE_Z, 0.0, BACK_PITCH)
GABLE_LEFT = (GABLE_APEX - PITCH * GABLE_X, PITCH, 0.0)
GABLE_RIGHT = (GABLE_APEX + PITCH * GABLE_X, -PITCH, 0.0)
BACK_GABLE_LEFT = (BACK_GABLE_APEX - BACK_GABLE_PITCH * GABLE_X, BACK_GABLE_PITCH, 0.0)
BACK_GABLE_RIGHT = (BACK_GABLE_APEX + BACK_GABLE_PITCH * GABLE_X, -BACK_GABLE_PITCH, 0.0)


def height(plane, x, z):
    a, bx, bz = plane
    return a + bx * x + bz * z


# where each cross gable's ridge runs into the main slope
front_meet_z = RIDGE_Z + (RIDGE_Y - GABLE_APEX) / FRONT_PITCH
back_meet_z = RIDGE_Z - (RIDGE_Y - BACK_GABLE_APEX) / BACK_PITCH

# ---------------------------------------------------------------- the entry's half
# The upper storey over the office stands well back from the gable's face under the
# street slope's eave, two small square windows in it over a low tile roof; the office
# projects from the ground storey's wall before it beside the gable under a lean-to,
# whose plane that low roof runs on in back to the upper wall.
# The office's lean-to runs on over an open porch bay beside it to one dark post; straight
# steps rise to the bay between two stone piers, a low stone planter curving before the
# office to the third, and the way in runs back past the office's slit windows and round
# the front corner to the door in the side wall.
SET_BACK = 0.45  # the ground storey's wall behind the office (the entry's box starts here)
back_z = HALF_D - SET_BACK
OFFICE_ROOF_TOP = 2.2  # the lean-to's top at the ground storey's wall
LEAN_TO_FALL = (OFFICE_ROOF_TOP - OFFICE_EAVE) / (OFFICE_DEPTH + OFFICE_OVER)  # kit _office_entry's shed
low_roof_at_upper = OFFICE_ROOF_TOP + LEAN_TO_FALL * (back_z - upper_z)
upper_top = height(STREET_SLOPE, 0.0, upper_z)
upper_wall = Frame((-HALF_W, 0, upper_z), (1, 0, 0), (0, 0, 1))
SMALL = [(-1.91 - 0.21, -1.91 + 0.21), (-0.66 - 0.21, -0.66 + 0.21)]
SMALL_Y = (2.44, 2.8)
holes = [(a, SMALL_Y[0], b, SMALL_Y[1]) for a, b in SMALL]
wall("FrontWall", upper_wall, 0, fx(gable_left), low_roof_at_upper - DECK, upper_top, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
for a, b in SMALL:
    window("FrontWall", upper_wall, fx(a), SMALL_Y[0], fx(b), SMALL_Y[1], mullion=False)
    upper_wall.block("FrontWall__Wood", fx(a) - 0.06, SMALL_Y[1] + 0.05, fx(b) + 0.06, SMALL_Y[1] + 0.1, 0.0, -0.025)
# the ground storey's wall behind the office, up under the lean-to, and the low roof on
# from it to the upper wall in the lean-to's plane
ground_wall = Frame((-HALF_W, 0, back_z), (1, 0, 0), (0, 0, 1))
wall("FrontWall", ground_wall, 0, fx(gable_left), 0, OFFICE_ROOF_TOP, [])
low_x0 = PASSAGE_X[0] - 0.12  # the lean-to's outer edge (kit _office_entry's bay)
tiled("FrontWall", Slope([(low_x0, OFFICE_ROOF_TOP, back_z), (gable_left, OFFICE_ROOF_TOP, back_z),
                          (gable_left, low_roof_at_upper, upper_z), (low_x0, low_roof_at_upper, upper_z)]), eave_edges=(3,), fascia=False)
# the garage gable's return, its side wall back to the upper storey
face("FrontWall__Siding", [(gable_left, 0, upper_z), (gable_left, 0, HALF_D), (gable_left, WALL_TOP, HALF_D), (gable_left, WALL_TOP, upper_z)], (-1, 0, 0))
face("FrontWall__Lining", [(gable_left + WALL, 0, upper_z), (gable_left + WALL, 0, HALF_D), (gable_left + WALL, WALL_TOP, HALF_D), (gable_left + WALL, WALL_TOP, upper_z)], (1, 0, 0))
office_entry(wall_z=back_z, office_roof="shed", window_style="header", popout=True, slits=2,
             inner_slit=True, steps_piers=True, planter=True, top=OFFICE_ROOF_TOP, bay=True)
# "The home entrance is off the side": past the bay, round the front corner, a door in the
# side wall under its own little shed, in the full-height wall behind the low roof's end.
SIDE_DOOR = 2.6
side_entrance(SIDE_DOOR, z_walk=(SIDE_DOOR - 0.6, back_z + 0.6))
# From above, that little shed is a long lean-to down the side walk: run its plane on back
# (kit side_entrance's shed: 0.45 out, its foot 0.22 over the door's head, 0.3 past it).
SIDE_SHED_BACK = 1.2
shed_y = BAY_FLOOR + 1.1 + 0.22
shed_z = SIDE_DOOR - DOOR_HALF - 0.3
tiled("LeftWall", Slope([(-HALF_W - 0.45, shed_y, shed_z), (-HALF_W - 0.45, shed_y, SIDE_SHED_BACK),
                         (-HALF_W, shed_y + 0.42 * 0.45, SIDE_SHED_BACK), (-HALF_W, shed_y + 0.42 * 0.45, shed_z)]), eave_edges=(0, 1), fascia=False)

# ---------------------------------------------------------------- the roof
# Each piece of a plane is named for the coded house's hip slope it lies over (the game's
# roof is a front-to-back hip on this lot: its front and back ends inside hip lines from
# the eave corners to the ridge's ends, its long slopes either side), so the model breaks
# where the game's roof does.
HIP_RUN = (EAVE_Z - RIDGE_HALF) / EAVE_X
REGIONS = {
    "FrontRoof": [lambda x, z: z - RIDGE_HALF - HIP_RUN * x, lambda x, z: z - RIDGE_HALF + HIP_RUN * x],
    "BackRoof": [lambda x, z: -z - RIDGE_HALF - HIP_RUN * x, lambda x, z: -z - RIDGE_HALF + HIP_RUN * x],
    "RightRoof": [lambda x, z: x, lambda x, z: RIDGE_HALF + HIP_RUN * x - z, lambda x, z: z + RIDGE_HALF + HIP_RUN * x],
    "LeftRoof": [lambda x, z: -x, lambda x, z: RIDGE_HALF - HIP_RUN * x - z, lambda x, z: z + RIDGE_HALF - HIP_RUN * x],
}


def region_of(x, z):
    return next(part for part, sides in REGIONS.items() if all(side(x, z) >= -1e-9 for side in sides))


def clip_plan(points, side):
    """A convex polygon cut to where the linear `side(x, z)` >= 0."""
    out = []
    n = len(points)
    for i in range(n):
        a, b = points[i], points[(i + 1) % n]
        fa, fb = side(a.x, a.z), side(b.x, b.z)
        if fa >= 0:
            out.append(a)
        if fa * fb < 0:
            out.append(a.lerp(b, fa / (fa - fb)))
    return out


def area(points):
    return sum(((points[i] - points[0]).cross(points[i + 1] - points[0]) for i in range(1, len(points) - 1)), Vector()).length / 2


def span(uv, axis, value):
    """Slope.span over a polygon's uv."""
    hits = []
    n = len(uv)
    for i in range(n):
        a, b = uv[i], uv[(i + 1) % n]
        if (a[axis] - value) * (b[axis] - value) <= 0 and a[axis] != b[axis]:
            t = (value - a[axis]) / (b[axis] - a[axis])
            hits.append(a[1 - axis] + t * (b[1 - axis] - a[1 - axis]))
    if len(hits) < 2:
        return None
    return min(hits), max(hits)


def deck(part, full, piece, outer):
    """A slope's deck over `piece` (a convex polygon on `full`'s plane) and its underside,
    with a fascia only on the edges `outer(a, b)` says are the slope's own (eaves, rakes)."""
    under = f"{part}__Siding"
    face(f"{part}__Roof", piece, full.N)
    face(under, [p - full.N * DECK for p in piece], -full.N)
    n = len(piece)
    centroid = sum(piece, Vector()) / n
    for i in range(n):
        a, b = piece[i], piece[(i + 1) % n]
        if outer(a, b):
            out = (b - a).cross(full.N)
            if out.dot((a + b) / 2 - centroid) < 0:
                out = -out
            face(under, [a, b, b - full.N * DECK, a - full.N * DECK], out)


def tiles(full, piece):
    """The kit's `tiled` barrels and course lips over `piece` on `full`'s plane, whole
    (a rib split at every seam between the game's slopes would cost phones a third more
    roof), each face in the part its middle lies over."""
    def tile(points, want):
        middle = sum(points, Vector()) / len(points)
        face(f"{region_of(middle.x, middle.z)}__Roof", points, want)

    uv = [((p - full.origin).dot(full.U), (p - full.origin).dot(full.V)) for p in piece]
    us, vs = [u for u, _ in full.uv], [v for _, v in full.uv]
    spacing, rib = RIB
    # a level eave's barrels closed by a strip (the kit's eave_closure) instead of their ends
    low = min(v for _, v in uv)
    eave_v = low if sum(abs(v - low) < 1e-6 for _, v in uv) >= 2 else None
    u = min(us) + spacing / 2
    while u < max(us) - spacing / 2:
        left, right = span(uv, 0, u - spacing / 2), span(uv, 0, u + spacing / 2)
        if left and right:
            lo, hi = max(left[0], right[0]), min(left[1], right[1])
            if hi - lo > 0.05:
                a, m, b = u - spacing / 2, u, u + spacing / 2
                tile([full.at(a, lo), full.at(m, lo, rib), full.at(m, hi, rib), full.at(a, hi)], full.N - full.U)
                tile([full.at(m, lo, rib), full.at(b, lo), full.at(b, hi), full.at(m, hi, rib)], full.N + full.U)
                if eave_v is None or lo - eave_v > 1e-4:
                    tile([full.at(a, lo), full.at(b, lo), full.at(m, lo, rib)], -full.V)
        u += spacing
    if eave_v is not None:
        across = span(uv, 1, eave_v + EAVE_CLOSURE)
        if across:
            lo, hi = across
            v = eave_v + EAVE_CLOSURE
            tile([full.at(lo, v), full.at(hi, v), full.at(hi, v, rib), full.at(lo, v, rib)], -full.V)
    v = min(vs) + COURSE
    while v < max(vs) - 0.1:
        across = span(uv, 1, v)
        if across and across[1] - across[0] > 0.1:
            lo, hi = across
            lift = rib * 1.25
            tile([full.at(lo, v), full.at(hi, v), full.at(hi, v, lift), full.at(lo, v, lift)], -full.V)
            tile([full.at(lo, v, lift), full.at(hi, v, lift), full.at(hi, v + LIP, rib * 0.5), full.at(lo, v + LIP, rib * 0.5)], full.N)
        v += COURSE


def roof_plane(plane, x0, x1, z0, z1, above=None, edges=(), grid=None):
    """The plane over x0..x1, z0..z1 in plan where it stands over `above` (a plane it
    meets in a valley), its deck cut into the coded house's parts and tiled on the grid
    of the plane over `grid` (x0', x1'), so the pieces of one slope line up; `edges` are
    the plan's outer lines, (0, x) or (1, z), that carry a fascia."""
    def lift(x, z):
        return Vector((x, height(plane, x, z), z))

    def rect(x0, x1):
        # the level edge first: the eave (or the line the slope rises from)
        if plane[1] != 0.0:
            return [lift(x0, z0), lift(x0, z1), lift(x1, z1), lift(x1, z0)]
        return [lift(x0, z0), lift(x1, z0), lift(x1, z1), lift(x0, z1)]

    whole = rect(*(grid or (x0, x1)))
    low = min(range(4), key=lambda i: whole[i].y + whole[(i + 1) % 4].y)
    full = Slope(whole[low:] + whole[:low])
    piece = rect(x0, x1)
    if above is not None:
        piece = clip_plan(piece, lambda x, z: height(plane, x, z) - height(above, x, z))

    def outer(a, b):
        return any(abs(a[2 * axis] - value) < 1e-6 and abs(b[2 * axis] - value) < 1e-6 for axis, value in edges)

    for part, sides in REGIONS.items():
        cut = piece
        for side in sides:
            cut = clip_plan(cut, side) if len(cut) >= 3 else cut
        if len(cut) >= 3 and area(cut) > 1e-4:
            deck(part, full, cut, outer)
    tiles(full, piece)


def cap(a, b, **size):
    """A ridge (or rake, or valley) cap in the part its middle lies over."""
    middle = (Vector(a) + Vector(b)) / 2
    ridge_cap(region_of(middle.x, middle.z), a, b, **size)


# the street slope, either side of the garage gable's ridge line, and the back slope
MAIN = (-EAVE_X, EAVE_X)
roof_plane(STREET_SLOPE, -EAVE_X, GABLE_X, RIDGE_Z, front_eave_z, above=GABLE_LEFT, edges=[(0, -EAVE_X), (1, front_eave_z)], grid=MAIN)
roof_plane(STREET_SLOPE, GABLE_X, EAVE_X, RIDGE_Z, front_eave_z, above=GABLE_RIGHT, edges=[(0, EAVE_X)], grid=MAIN)
roof_plane(BACK_SLOPE, -EAVE_X, GABLE_X, -EAVE_Z, RIDGE_Z, above=BACK_GABLE_LEFT, edges=[(0, -EAVE_X), (1, -EAVE_Z)], grid=MAIN)
roof_plane(BACK_SLOPE, GABLE_X, EAVE_X, -EAVE_Z, RIDGE_Z, above=BACK_GABLE_RIGHT, edges=[(0, EAVE_X), (1, -EAVE_Z)], grid=MAIN)
# the cross gables' slopes, where they stand over the main ones
CROSS_GABLES = (
    (STREET_SLOPE, GABLE_LEFT, GABLE_RIGHT, GABLE_APEX, RIDGE_Z, EAVE_Z, EAVE_Z, front_meet_z, front_eave_z),
    (BACK_SLOPE, BACK_GABLE_LEFT, BACK_GABLE_RIGHT, BACK_GABLE_APEX, -EAVE_Z, RIDGE_Z, -EAVE_Z, back_meet_z, -EAVE_Z),
)
for main, left, right, apex, z0, z1, end, meet, foot in CROSS_GABLES:
    roof_plane(left, gable_eave_x, GABLE_X, z0, z1, above=main, edges=[(0, gable_eave_x), (1, end)])
    roof_plane(right, GABLE_X, EAVE_X, z0, z1, above=main, edges=[(0, EAVE_X), (1, end)])

# the caps: the main ridge, the cross gables' ridges, barge tiles down every rake and a
# thin flashing in each valley
RAKE = {"half": 0.05, "lift": 0.045}
VALLEY = {"half": 0.04, "lift": 0.01}
cap((-EAVE_X, RIDGE_Y, RIDGE_Z), (0.0, RIDGE_Y, RIDGE_Z))
cap((0.0, RIDGE_Y, RIDGE_Z), (EAVE_X, RIDGE_Y, RIDGE_Z))
for main, left, right, apex, z0, z1, end, meet, foot in CROSS_GABLES:
    cap((GABLE_X, apex, end), (GABLE_X, apex, meet))
    cap((gable_eave_x, EAVE_Y, end), (GABLE_X, apex, end), **RAKE)
    cap((EAVE_X, EAVE_Y, end), (GABLE_X, apex, end), **RAKE)
    cap((gable_eave_x, EAVE_Y, foot), (GABLE_X, apex, meet), **VALLEY)
    cap((EAVE_X, EAVE_Y, foot), (GABLE_X, apex, meet), **VALLEY)
for x in (-EAVE_X, EAVE_X):
    cap((x, EAVE_Y, -EAVE_Z), (x, RIDGE_Y, RIDGE_Z), **RAKE)
    cap((x, EAVE_Y, front_eave_z), (x, RIDGE_Y, RIDGE_Z), **RAKE)


def cross_gable_end(part, z, apex, pitch, outward):
    """The kit's gable_end_wall for a cross gable of any pitch: its stucco triangle (and
    lining) over the wall top from the gable's return to the right corner; returns its
    peak's height."""
    def y_at(x):
        return apex - pitch * abs(x - GABLE_X) - DECK * 1.1

    outline = [(gable_left, WALL_TOP), (HALF_W, WALL_TOP), (HALF_W, max(WALL_TOP, y_at(HALF_W))), (GABLE_X, y_at(GABLE_X)), (gable_left, max(WALL_TOP, y_at(gable_left)))]
    face(f"{part}__Siding", [(x, y, z) for x, y in outline], (0, 0, outward))
    face(f"{part}__Lining", [(x, y, z - outward * WALL) for x, y in outline], (0, 0, -outward))
    return y_at(GABLE_X)


# the cross gables' stucco ends, a small lattice vent near each one's peak
peak = cross_gable_end("FrontWall", HALF_D, GABLE_APEX, PITCH, 1)
lattice_vent("FrontWall", front, fx(GABLE_X), peak - 0.32)
peak = cross_gable_end("BackWall", -HALF_D, BACK_GABLE_APEX, BACK_GABLE_PITCH, -1)
lattice_vent("BackWall", WALL_FRAMES["back"], -GABLE_X, peak - 0.32)


def side_gable(side, z0, z1):
    """The main roof's stucco gable end over a flank's wall top, from z0 to z1."""
    x = HALF_W if side == "right" else -HALF_W
    outward = 1 if side == "right" else -1
    part = WALL_PARTS[side]
    under = DECK * 1.1
    zb = RIDGE_Z - (RIDGE_Y - under - WALL_TOP) / BACK_PITCH
    zf = RIDGE_Z + (RIDGE_Y - under - WALL_TOP) / FRONT_PITCH
    outline = [(max(zb, z0), WALL_TOP), (RIDGE_Z, RIDGE_Y - under), (min(zf, z1), WALL_TOP)]
    face(f"{part}__Siding", [(x, y, z) for z, y in outline], (outward, 0, 0))
    face(f"{part}__Lining", [(x - outward * WALL, y, z) for z, y in outline], (-outward, 0, 0))


side_gable("right", -HALF_D, HALF_D)
side_gable("left", -HALF_D, upper_z)

# The array as the drone shows it: one block of six large panels, two across and three up
# the street slope, laid landscape, over the left half high on the slope, clear of the
# garage gable's valley.
SOLAR = {"cols": 2, "rows": 3, "panel": (1.09, 0.58), "x": (-2.85, -0.65), "middle_z": 0.75}
street = Slope([(-EAVE_X, EAVE_Y, front_eave_z), (EAVE_X, EAVE_Y, front_eave_z), (EAVE_X, RIDGE_Y, RIDGE_Z), (-EAVE_X, RIDGE_Y, RIDGE_Z)])
block_up = SOLAR["rows"] * SOLAR["panel"][1] + (SOLAR["rows"] - 1) * SOLAR_GAP
foot_v = (front_eave_z - SOLAR["middle_z"]) * math.hypot(1.0, FRONT_PITCH) - block_up / 2
for col in range(SOLAR["cols"]):
    for row in range(SOLAR["rows"]):
        pu = SOLAR["x"][0] + EAVE_X + col * (SOLAR["panel"][0] + SOLAR_GAP)
        pv = foot_v + row * (SOLAR["panel"][1] + SOLAR_GAP)
        corners = [(pu, pv), (pu + SOLAR["panel"][0], pv), (pu + SOLAR["panel"][0], pv + SOLAR["panel"][1]), (pu, pv + SOLAR["panel"][1])]
        middle = street.at(pu + SOLAR["panel"][0] / 2, pv + SOLAR["panel"][1] / 2)
        face(f"{region_of(middle.x, middle.z)}__Solar", [street.at(u, v, SOLAR_LIFT) for u, v in corners], street.N)

# ---------------------------------------------------------------- the sides and back
plain_wall("back", PLAN, half_width={-1.4: 0.42, 0.55: 0.42, 2.05: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27, span=(-HALF_D, upper_z), extra_holes=[side_entrance_hole(SIDE_DOOR)])
# the flank's ground storey on to the wall behind the office, under the low roof
left_low = [(upper_z, 0.0), (back_z, 0.0), (back_z, OFFICE_ROOF_TOP), (upper_z, low_roof_at_upper)]
face("LeftWall__Siding", [(-HALF_W, y, z) for z, y in left_low], (-1, 0, 0))
face("LeftWall__Lining", [(-HALF_W + WALL, y, z) for z, y in left_low], (1, 0, 0))

export(PLAN)
