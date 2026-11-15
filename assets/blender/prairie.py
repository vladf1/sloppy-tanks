# The Senda Prairie house (styles 12 to 14), modelled after Rumbo north lot 2, for Sloppy Tanks in
# Blender (headless):
#   blender --background --python assets/blender/prairie.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:prairie writes assets/models/houses/prairie.glb and
#   crates/core/data/houses/prairie.plan.json)
#
# Drawn in the street frame on a 6.2 x 8 m lot (the tract's 11.7 x 15 m box at the level's
# scale), its garage on the right; the game builds it with its door in the eave wall (its
# hip's ridge along the street) and fits it to every lot of its styles. A broad two-storey
# stucco block under a low flat-tile roof drawn as the lot's is (a main ridge front to back,
# a cross hip over the front, a hip end at the back; the collider stays the coded hip),
# three openings across its upper floor (a shuttered pair over the door, a small single and
# a shuttered pair on brackets over the garage), a one-storey office beside the garage
# under a flat-tile shed, the entry passage up its side under a lower shed on tapered posts
# over stacked-stone piers, its wall clad in board and batten, and the garage's one-piece
# roll-up door with a row of lites. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Prairie front (the street's layout reads its door and garage) and its five
# upper windows; two windows a floor down the right flank, three down the left (one by its
# back corner, one a third of the way along, one toward the front, as the lot's west wall
# has them) and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.96, -0.36, 0.7, 1.7, 2.5], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.7, "Window"], [0.2, "Window"], [1.9, "Window"]], "upper": [-1.7, 0.2, 1.9], "garage_bay": 0.0},
        "right": {"ground": [[-2.0, "Window"], [2.0, "Window"]], "upper": [-2.0, 2.0], "garage_bay": 0.0},
        "left": {"ground": [[-3.0, "Window"], [-1.1, "Window"], [1.6, "Window"]], "upper": [-3.0, -1.1, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]
up = _front["upper"]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
PAIR = 0.33  # how far a ganged pair runs past its windows' centres
UPPER = [(up[0] - PAIR, up[1] + PAIR), (up[2] - 0.25, up[2] + 0.25), (up[3] - PAIR, up[4] + PAIR)]  # the pair, the single, the pair
# The passage's front door stands for the coded ground window (its hole spans -1.91); the
# coded door stands behind the office.
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in UPPER]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor="lites", boards=True, header=True)
front_door(front, door_u)
for a, b in UPPER:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
# both pairs hung with board shutters, the single plain
for a, b in (UPPER[0], UPPER[2]):
    board_shutters(front, fx(a), fx(b), UPPER_GLASS[0] - 0.01, UPPER_GLASS[1] + 0.01)
# brackets and a shelf under the pair over the garage
for u in (fx(UPPER[2][0]) + 0.12, fx(UPPER[2][1]) - 0.12):
    front.block("FrontWall__Wood", u - 0.03, UPPER_GLASS[0] - 0.18, u + 0.03, UPPER_GLASS[0] - 0.04, 0.0, -0.09)
front.block("FrontWall__Wood", fx(UPPER[2][0]) - 0.05, UPPER_GLASS[0] - 0.05, fx(UPPER[2][1]) + 0.05, UPPER_GLASS[0] - 0.015, 0.0, -0.1)

# The passage's wall clad in board and batten round the front door, under the cover.
CLAD = (PASSAGE_X[0] + 0.05, PASSAGE_X[1])
CLAD_FOOT, CLAD_TOP = PASSAGE_FLOOR + 0.02, COVER_EAVE + 0.2
entry0, _, entry1, entry_top = entry_holes()[0]
boards = [
    (CLAD[0], CLAD_FOOT, entry0 - 0.03, CLAD_TOP),
    (entry0 - 0.03, entry_top + 0.03, entry1 + 0.03, CLAD_TOP),
    (entry1 + 0.03, CLAD_FOOT, CLAD[1], CLAD_TOP),
]
for a, y0, b, y1 in boards:
    front.block("FrontWall__Cladding", fx(a), y0, fx(b), y1, 0.0, -0.012)
    u = a + 0.1
    while u < b - 0.04:
        front.block("FrontWall__Cladding", fx(u) - 0.01, y0, fx(u) + 0.01, y1, -0.012, -0.03)
        u += 0.2

# ---------------------------------------------------------------- the office and entry
# The office projecting before the left half under a flat-tile shed, the entry passage
# up its outer side under a small street gable on tapered posts over stone piers.
office_entry(office_roof="shed", flat=True, cover="shed", supports="tapered", window_style="header")

# ---------------------------------------------------------------- the roof
# Seen from above, the lot's roof is no hip along the street: its main ridge runs front to
# back between two long side slopes (the array on the left one) to a hip end at the back,
# and a cross hip over the front carries a short ridge along the street, its street slope
# at the main pitch and its ends dropping steeply to the side eaves, the ridges meeting in
# a T; behind the cross ridge its back slope falls to the side slopes in two valleys.
# The game's collider stays the coded hip along the street: each face is cut along the
# coded hip lines into the part whose slope it lies over ([`coded_part`]), so it falls
# with that part.
# From the side the main ridge sits a little under the cross ridge, which tops the house.
EX, EZ, EY = EAVE_X, EAVE_Z, EAVE_Y
CROSS_Z = HALF_D - (RIDGE_Y - WALL_TOP) / PITCH  # the cross ridge's z: its street slope at the main pitch
CROSS_HALF = 0.9  # half the cross ridge's length
END_FOOT = 0.0  # how far back the cross hip's ends reach down the side eaves
MAIN_RIDGE_Y = RIDGE_Y - 0.3
SIDE_PITCH = (MAIN_RIDGE_Y - WALL_TOP) / HALF_W  # the side slopes' and the back hip's
SIDE_EAVE_Y = WALL_TOP - SIDE_PITCH * OVERHANG
# the cross hip's back slope, from its ridge down to the side eaves at END_FOOT, and where
# the main ridge runs into it
CROSS_BACK_PITCH = (RIDGE_Y - SIDE_EAVE_Y) / (CROSS_Z - END_FOOT)
MAIN_RIDGE_FRONT = CROSS_Z - (RIDGE_Y - MAIN_RIDGE_Y) / CROSS_BACK_PITCH
CORNER = {"fw": (-EX, EY, EZ), "fe": (EX, EY, EZ), "be": (EX, SIDE_EAVE_Y, -EZ), "bw": (-EX, SIDE_EAVE_Y, -EZ)}
RIDGE_BACK = (0.0, MAIN_RIDGE_Y, -EZ + (MAIN_RIDGE_Y - SIDE_EAVE_Y) / SIDE_PITCH)
RIDGE_FRONT = (0.0, MAIN_RIDGE_Y, MAIN_RIDGE_FRONT)
CROSS_W, CROSS_E = (-CROSS_HALF, RIDGE_Y, CROSS_Z), (CROSS_HALF, RIDGE_Y, CROSS_Z)
FOOT_W, FOOT_E = (-EX, SIDE_EAVE_Y, END_FOOT), (EX, SIDE_EAVE_Y, END_FOOT)
ROOF = {
    # each polygon convex, its first edge level (an eave, or the ridge a valley hangs from)
    "street": [CORNER["fw"], CORNER["fe"], CROSS_E, CROSS_W],
    "back": [CORNER["be"], CORNER["bw"], RIDGE_BACK],
    "west": [CORNER["bw"], FOOT_W, RIDGE_FRONT, RIDGE_BACK],
    "east": [FOOT_E, CORNER["be"], RIDGE_BACK, RIDGE_FRONT],
    "west end": [FOOT_W, CORNER["fw"], CROSS_W],
    "east end": [CORNER["fe"], FOOT_E, CROSS_E],
    # the cross hip's back slope, one plane in three
    "cross back": [CROSS_W, CROSS_E, RIDGE_FRONT],
    "west valley": [CROSS_W, RIDGE_FRONT, FOOT_W],
    "east valley": [RIDGE_FRONT, CROSS_E, FOOT_E],
}

# The coded hip along the street, in plan: its ridge's ends at x = +-CODED_RIDGE and its
# hips from there to the eave corners. Each region is convex: half-planes a x + b z + c >= 0.
CODED_RIDGE = ALONG_RIDGE_HALF
_k = (EX - CODED_RIDGE) / EZ
CODED_REGIONS = {
    "RightRoof": [(1, -_k, -CODED_RIDGE), (1, _k, -CODED_RIDGE)],
    "LeftRoof": [(-1, -_k, -CODED_RIDGE), (-1, _k, -CODED_RIDGE)],
    "FrontRoof": [(0, 1, 0), (-1, _k, CODED_RIDGE), (1, _k, CODED_RIDGE)],
    "BackRoof": [(0, -1, 0), (-1, -_k, CODED_RIDGE), (1, -_k, CODED_RIDGE)],
}


def clip_plan(polygon, a, b, c):
    """A convex polygon cut to a x + b z + c >= 0 in plan."""
    out = []
    n = len(polygon)
    for i in range(n):
        p, q = Vector(polygon[i]), Vector(polygon[(i + 1) % n])
        fp, fq = a * p.x + b * p.z + c, a * q.x + b * q.z + c
        if fp >= 0:
            out.append(p)
        if fp * fq < 0:
            out.append(p.lerp(q, fp / (fp - fq)))
    return out


def coded_part(point):
    """The coded roof part over a point in plan."""
    for part, planes in CODED_REGIONS.items():
        if all(a * point[0] + b * point[2] + c >= -1e-9 for a, b, c in planes):
            return part
    return "FrontRoof"


def plan_area(polygon):
    return abs(sum(p.x * q.z - q.x * p.z for p, q in zip(polygon, polygon[1:] + polygon[:1]))) / 2


class Piece(Slope):
    """A piece of a roof face cut by the coded hip lines, measured in the whole face's
    frame, so its tile courses and ribs run on from its neighbours'."""

    def __init__(self, whole, points):
        self.points = [Vector(p) for p in points]
        self.U, self.V, self.N, self.origin = whole.U, whole.V, whole.N, whole.origin
        self.uv = [((p - self.origin).dot(self.U), (p - self.origin).dot(self.V)) for p in self.points]


def tiled_piece(part, slope, eave_y):
    """The kit's flat-tile slope (`tiled`) for a piece: its ribs and courses on the whole
    face's grid, its fascia only along the eaves (its other edges meet roof)."""
    roof, under = f"{part}__Roof", f"{part}__Siding"
    face(roof, [slope.at(u, v, 0) for u, v in slope.uv], slope.N)
    n = len(slope.uv)
    # its underside only where it overhangs a wall: a piece clear of the eaves is over the
    # rooms, out of sight
    if any(p.y < eave_y + 1e-3 and q.y < eave_y + 1e-3 for p, q in zip(slope.points, slope.points[1:] + slope.points[:1])):
        face(under, [slope.at(u, v, -DECK) for u, v in slope.uv], -slope.N)
    centroid = sum(slope.points, Vector()) / n
    for i in range(n):
        a, b = slope.points[i], slope.points[(i + 1) % n]
        if max(a.y, b.y) > eave_y + 1e-3:
            continue
        out = (b - a).cross(slope.N)
        if out.dot((a + b) / 2 - centroid) < 0:
            out = -out
        face(under, [a, b, b - slope.N * DECK, a - slope.N * DECK], out)
    spacing, height = FLAT_RIB
    us = [p[0] for p in slope.uv]
    u = (math.floor(min(us) / spacing) + 0.5) * spacing
    while u < max(us) - spacing / 2:
        left, right = slope.span(0, u - spacing / 2), slope.span(0, u + spacing / 2)
        if u - spacing / 2 >= min(us) - 1e-6 and left and right:
            lo, hi = max(left[0], right[0]), min(left[1], right[1])
            if hi - lo > 0.05:
                # one face a rib (the kit's two and an end): a cut roof has twice the
                # ribs, and a 1.3 cm rib reads the same as a step
                a, b = u - spacing / 2, u + spacing / 2
                face(roof, [slope.at(a, lo), slope.at(b, lo, height), slope.at(b, hi, height), slope.at(a, hi)], slope.N - slope.U)
        u += spacing
    vs = [p[1] for p in slope.uv]
    v = (math.floor(min(vs) / FLAT_COURSE) + 1) * FLAT_COURSE
    while v < max(vs) - 0.1:
        span = slope.span(1, v)
        if span and span[1] - span[0] > 0.1:
            lo, hi = span
            face(roof, [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, 0.03), slope.at(lo, v, 0.03)], -slope.V)
            face(roof, [slope.at(lo, v, 0.03), slope.at(hi, v, 0.03), slope.at(hi, v + LIP, height * 0.5), slope.at(lo, v + LIP, height * 0.5)], slope.N)
        v += FLAT_COURSE


def coded_cap(a, b, **cap):
    """A ridge, hip or valley cap, cut where it crosses the coded hip lines."""
    a, b = Vector(a), Vector(b)
    cuts = {0.0, 1.0}
    for planes in CODED_REGIONS.values():
        for pa, pb, pc in planes:
            fa, fb = pa * a.x + pb * a.z + pc, pa * b.x + pb * b.z + pc
            if fa * fb < 0:
                cuts.add(fa / (fa - fb))
    cuts = sorted(cuts)
    for t0, t1 in zip(cuts, cuts[1:]):
        if t1 - t0 > 1e-3:
            ridge_cap(coded_part(a.lerp(b, (t0 + t1) / 2)), a.lerp(b, t0), a.lerp(b, t1), **cap)


for name, polygon in ROOF.items():
    whole = Slope(polygon)
    for part, planes in CODED_REGIONS.items():
        piece = list(whole.points)
        for plane in planes:
            piece = clip_plan(piece, *plane)
            if len(piece) < 3:
                break
        if len(piece) >= 3 and plan_area(piece) > 1e-3:
            tiled_piece(part, Piece(whole, piece), max(EY, SIDE_EAVE_Y))

coded_cap(RIDGE_BACK, RIDGE_FRONT)
coded_cap(CROSS_W, CROSS_E)
for corner in ("bw", "be"):
    coded_cap(CORNER[corner], RIDGE_BACK)
for foot, corner, end in ((FOOT_W, CORNER["fw"], CROSS_W), (FOOT_E, CORNER["fe"], CROSS_E)):
    coded_cap(corner, end)
    coded_cap(foot, end)
    coded_cap(foot, RIDGE_FRONT, half=0.04, lift=0.01)

# The array: one small block of six panels laid landscape, three along the left slope by
# two up it, over the middle of the house.
ARRAY_Z, ARRAY_UP = -1.39, 0.12  # its middle's z, its foot's share of the way up


def landscape_array(slope, z_mid, up, cols=3, rows=2):
    """`solar_array` with its panels turned landscape (long side along the eave), each in
    the coded part it lies over."""
    long, short = SOLAR_PANEL[1], SOLAR_PANEL[0]
    width = cols * long + (cols - 1) * SOLAR_GAP
    u0 = (Vector((0, 0, z_mid)) - slope.origin).dot(slope.U) - width / 2
    vs = [v for _, v in slope.uv]
    v0 = min(vs) + (max(vs) - min(vs)) * up
    for c in range(cols):
        for r in range(rows):
            pu, pv = u0 + c * (long + SOLAR_GAP), v0 + r * (short + SOLAR_GAP)
            corners = [(pu, pv), (pu + long, pv), (pu + long, pv + short), (pu, pv + short)]
            if all((span := slope.span(1, v)) and span[0] + 0.05 <= u <= span[1] - 0.05 for u, v in corners):
                part = coded_part(slope.at(pu + long / 2, pv + short / 2))
                face(part + "__Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


landscape_array(Slope(ROOF["west"]), ARRAY_Z, ARRAY_UP)

# ---------------------------------------------------------------- the flanks and back
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-1.7: 0.42, 0.2: 0.42, 1.9: 0.3})
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

export(PLAN, frame="along")
