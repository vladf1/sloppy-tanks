# The Senda Spanish house (styles 3, 4 and 5), modelled after Rumbo north lot 5, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:spanish writes assets/models/houses/spanish.glb and
#   crates/core/data/houses/spanish.plan.json)
#
# Drawn for the 6.2 x 8 m lot with its garage on the right (the game fits it to every lot of
# its styles: mirrored, stretched to the lot's box, wall top and ridge), at the level's scale
# (real sizes x 0.533). As the drone sees it from above: a clay roof whose ridge runs along the
# street over the middle of the house, its flanks rising to stucco gable ends (a clay vent in
# the west one), a long back slope and, in front, a long slope carrying an L of six solar
# panels; a steeper two-storey front gable over the entry side, twin arched windows high in it
# and a clay vent in its peak, standing forward of the upper floor over the garage, its ridge
# running back into the main slope in two valleys. The upper floor beside it is set back over
# the garage's one-piece roll-up door, a clay pent covering the ledge. Before the gable, the
# one-storey wing under one low clay hip: the office with its arched street window on the
# outer side, an arched porch on the garage side, the front door at its back. The paints are
# the style's (the game's palette).
#
# The game's structure for this design is a hip whose ridge runs front to back (its
# `DoorWall::Gable`): the roof's parts are its four slopes, so every roof face is cut along
# the coded hip lines and named after the slope it lies over (`by_slope`), whatever the
# drawn roof's form.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import house_kit  # noqa: E402
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded Spanish plan, which the street's layout reads
# (the path and mailbox find the door): the window behind the office, the door at the back
# of the porch beside it, the garage's two leaves and the windows over them; the twin arched
# windows in the gable, over the office, are one upper window of the structure. The flanks
# and back: two or three windows a floor.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8, -1.91], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.4, "Window"], [2.1, "Window"]], "upper": [-1.6, 0.4, 2.1], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.9, "Window"]], "upper": [-2.3, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-2.5, "Window"], [0.0, "Window"], [2.3, "Window"]], "upper": [-2.5, 0.0, 2.3], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
window_u = _front["ground"][0][0]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- measures
# The main roof: its ridge along the street over the middle at the lot's ridge height, its
# slopes the pitch that reaches the eaves front and back (the kit's along-street pitch).
MAIN_PITCH = ALONG_PITCH
MAIN_EAVE_Y = ALONG_EAVE_Y
# The upper floor beside the gable stands back of the front wall (real 0.75 m) over the
# garage's ledge; the main slope's front eave is over it.
SETBACK = 0.4
UPPER_Z = HALF_D - SETBACK
FRONT_EAVE_Z = UPPER_Z + OVERHANG
# The two-storey front gable: centred over the office, its eaves the main roof's height, at
# the kit's steeper pitch, its right eave over the ledge.
GABLE_X = -1.6
GABLE_PITCH = PITCH
GABLE_LEFT, GABLE_RIGHT = -EAVE_X, 2 * GABLE_X + EAVE_X  # its eaves
GABLE_WALL = 2 * GABLE_X + HALF_W  # its stucco's right edge, the upper floor's corner
# The ledge over the garage, under the pent (its eave and how far it stands out).
LEDGE = 1.42
PENT_OUT, PENT_Y = 0.32, 1.42


def main_y(z):
    """The main roof's top over z."""
    return RIDGE_Y - MAIN_PITCH * abs(z)


def gable_y(x):
    """The front gable's roof over x."""
    return MAIN_EAVE_Y + GABLE_PITCH * (GABLE_X - GABLE_LEFT - abs(x - GABLE_X))


APEX = gable_y(GABLE_X)
MEET_Z = (RIDGE_Y - APEX) / MAIN_PITCH  # where the gable's ridge runs into the main slope
# where the right valley reaches the main slope's front eave
FOOT_X = GABLE_X + (APEX - main_y(FRONT_EAVE_Z)) / GABLE_PITCH


# ---------------------------------------------------------------- local roof helpers
def tiled_on(part, points, ref, outer=(), ends=True, cut_below=False, cut_above=False, flat=False):
    """The kit's `tiled` with its ribs and courses laid from `ref` (a point in the slope's
    plane), so slopes cut into pieces keep their tiles in line across the cuts, and a fascia
    only on the `outer` edges (edge i runs from point i to the next)."""
    slope = Slope(points)
    roof, under = f"{part}__Roof", f"{part}__Siding"
    face(roof, [slope.at(u, v, 0) for u, v in slope.uv], slope.N)
    face(under, [slope.at(u, v, -DECK) for u, v in slope.uv], -slope.N)
    n = len(slope.uv)
    centroid = sum(slope.points, Vector()) / n
    for i in outer:
        (ua, va), (ub, vb) = slope.uv[i], slope.uv[(i + 1) % n]
        edge = slope.at(ub, vb) - slope.at(ua, va)
        out = edge.cross(slope.N)
        mid = (slope.at(ua, va) + slope.at(ub, vb)) / 2
        if out.dot(mid - centroid) < 0:
            out = -out
        face(under, [slope.at(ua, va, 0), slope.at(ub, vb, 0), slope.at(ub, vb, -DECK), slope.at(ua, va, -DECK)], out)
    reference = Vector(ref) - slope.origin
    u_ref, v_ref = reference.dot(slope.U), reference.dot(slope.V)
    us = [p[0] for p in slope.uv]
    vs = [p[1] for p in slope.uv]
    spacing, height = FLAT_RIB if flat else RIB
    course = FLAT_COURSE if flat else COURSE
    # a level eave's barrels closed by the kit's strip instead of their open ends
    eave_v = closed_eave(slope, flat) if ends else None
    u = u_ref + spacing / 2 + math.ceil((min(us) - u_ref) / spacing) * spacing
    while u < max(us) - spacing / 2:
        left, right = slope.span(0, u - spacing / 2), slope.span(0, u + spacing / 2)
        if left and right:
            lo, hi = max(left[0], right[0]), min(left[1], right[1])
            if hi - lo > 0.05:
                a, m, b = u - spacing / 2, u, u + spacing / 2
                face(roof, [slope.at(a, lo), slope.at(m, lo, height), slope.at(m, hi, height), slope.at(a, hi)], slope.N - slope.U)
                face(roof, [slope.at(m, lo, height), slope.at(b, lo), slope.at(b, hi), slope.at(m, hi, height)], slope.N + slope.U)
                if ends and (eave_v is None or lo - eave_v > 1e-4):
                    face(roof, [slope.at(a, lo), slope.at(b, lo), slope.at(m, lo, height)], -slope.V)
        u += spacing
    if eave_v is not None:
        eave_closure(roof, slope, eave_v, height)
    # courses run on across a cut (`cut_below`, `cut_above`), kept off a real eave and ridge
    v = v_ref + math.ceil((min(vs) + (0.0 if cut_below else 0.05) - v_ref) / course) * course
    while v < max(vs) - (1e-4 if cut_above else 0.1):
        span = slope.span(1, v)
        if span and span[1] - span[0] > 0.1:
            lo, hi = span
            lift = 0.03 if flat else height * 1.25
            face(roof, [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, lift), slope.at(lo, v, lift)], -slope.V)
            face(roof, [slope.at(lo, v, lift), slope.at(hi, v, lift), slope.at(hi, v + LIP, height * 0.5), slope.at(lo, v + LIP, height * 0.5)], slope.N)
        v += course


# The coded hip's slopes in plan, each the half-planes a x + b z >= c: its hip lines run at
# 45 degrees from the eave corners to the ridge's ends, RIDGE_HALF either way of the middle.
SLOPE_REGIONS = {
    "FrontRoof": [(-1, 1, RIDGE_HALF), (1, 1, RIDGE_HALF)],
    "BackRoof": [(-1, -1, RIDGE_HALF), (1, -1, RIDGE_HALF)],
    "RightRoof": [(1, 0, 0), (1, -1, -RIDGE_HALF), (1, 1, -RIDGE_HALF)],
    "LeftRoof": [(-1, 0, 0), (-1, -1, -RIDGE_HALF), (-1, 1, -RIDGE_HALF)],
}


def _clip_half(points, a, b, c):
    out = []
    n = len(points)
    for i in range(n):
        p, q = points[i], points[(i + 1) % n]
        fp, fq = a * p.x + b * p.z - c, a * q.x + b * q.z - c
        if fp >= -1e-9:
            out.append(p)
        if (fp > 1e-9 and fq < -1e-9) or (fp < -1e-9 and fq > 1e-9):
            out.append(p.lerp(q, fp / (fp - fq)))
    return out


def _area(points):
    total = Vector()
    for i in range(1, len(points) - 1):
        total += (points[i] - points[0]).cross(points[i + 1] - points[0])
    return total.length / 2


def by_slope(source):
    """Every face drawn as `source`'s cut along the coded hip lines into the slope parts it
    lies over."""
    for key in [key for key in geometry if key.split("__")[0] == source]:
        vertices, faces = geometry.pop(key)
        surface = key.split("__")[1]
        for f in faces:
            polygon = [vertices[i] for i in f]
            for part, sides in SLOPE_REGIONS.items():
                piece = polygon
                for a, b, c in sides:
                    piece = _clip_half(piece, a, b, c)
                    if len(piece) < 3:
                        break
                if len(piece) >= 3 and _area(piece) > 1e-7:
                    face(f"{part}__{surface}", piece)


# ---------------------------------------------------------------- the roof
# Drawn as "Main" and cut into the coded slopes at the end. The points: the ridge's ends, the
# gable's front apex and where its ridge meets the main slope, its eave corners, the right
# valley's foot on the main front eave.
R0, R1 = (-EAVE_X, RIDGE_Y, 0.0), (EAVE_X, RIDGE_Y, 0.0)
MEET = (GABLE_X, APEX, MEET_Z)
GABLE_APEX = (GABLE_X, APEX, EAVE_Z)
LEFT_CORNER = (GABLE_LEFT, MAIN_EAVE_Y, EAVE_Z)
GABLE_CORNER = (GABLE_RIGHT, MAIN_EAVE_Y, EAVE_Z)
GABLE_BACK = (GABLE_RIGHT, MAIN_EAVE_Y, FRONT_EAVE_Z)
FOOT = (FOOT_X, main_y(FRONT_EAVE_Z), FRONT_EAVE_Z)
EAVE_RIGHT = (EAVE_X, main_y(FRONT_EAVE_Z), FRONT_EAVE_Z)
MEET_LEFT, MEET_RIGHT = (-EAVE_X, APEX, MEET_Z), (EAVE_X, APEX, MEET_Z)
BACK_LEFT, BACK_RIGHT = (-EAVE_X, MAIN_EAVE_Y, -EAVE_Z), (EAVE_X, MAIN_EAVE_Y, -EAVE_Z)

# the long back slope, rakes at both gable ends
tiled_on("Main", [BACK_RIGHT, BACK_LEFT, R0, R1], BACK_RIGHT, outer=(0, 1, 3))
# the front slope: the strip from the ridge down to the gable's ridge, the piece west of the
# gable down to the front corner, and the piece east of it down to the eave over the upper floor
front_ref = (0.0, main_y(FRONT_EAVE_Z), FRONT_EAVE_Z)
tiled_on("Main", [MEET_LEFT, MEET_RIGHT, R1, R0], front_ref, outer=(1, 3), ends=False, cut_below=True)
tiled_on("Main", [MEET_LEFT, MEET, LEFT_CORNER], front_ref, outer=(2,), cut_above=True)
tiled_on("Main", [FOOT, EAVE_RIGHT, MEET_RIGHT, MEET], front_ref, outer=(0, 1), cut_above=True)
# the front gable's two slopes (the right one also over the corner by the valley's foot)
tiled_on("Main", [GABLE_APEX, MEET, LEFT_CORNER], GABLE_APEX, outer=(2,))
tiled_on("Main", [GABLE_APEX, MEET, GABLE_CORNER], GABLE_APEX, outer=(2,))
tiled_on("Main", [GABLE_BACK, GABLE_CORNER, FOOT], GABLE_APEX, outer=(0, 2))
ridge_cap("Main", R0, R1)
ridge_cap("Main", GABLE_APEX, MEET)
ridge_cap("Main", MEET, LEFT_CORNER, half=0.04, lift=0.01)
ridge_cap("Main", MEET, FOOT, half=0.04, lift=0.01)

# The array: six panels laid landscape on the front slope beside the gable's ridge, two rows
# of two under the ridge and two more down the garage side: an L.
PANEL_ACROSS, PANEL_UP = SOLAR_PANEL[1], SOLAR_PANEL[0] / math.hypot(1, MAIN_PITCH)
_normal = Vector((0, 1, MAIN_PITCH)).normalized()


def solar_panel(x0, z0):
    corners = [(x0, z0 + PANEL_UP), (x0 + PANEL_ACROSS, z0 + PANEL_UP), (x0 + PANEL_ACROSS, z0), (x0, z0)]
    face("Main__Solar", [Vector((x, main_y(z), z)) + _normal * SOLAR_LIFT for x, z in corners], _normal)


ARRAY_RIGHT, ARRAY_TOP = 0.85, 0.28
for column, rows in ((0, 4), (1, 2)):
    for row in range(rows):
        solar_panel(ARRAY_RIGHT - (column + 1) * PANEL_ACROSS - column * SOLAR_GAP, ARRAY_TOP + row * (PANEL_UP + SOLAR_GAP))

by_slope("Main")

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
TWIN = (window_u - 0.34, window_u + 0.34)  # the gable's two arched windows, in one hole
TWIN_SPRING, TWIN_RISE = 2.62, 0.17
OVER_GARAGE = [(0.76, 1.24), (1.38, 2.3)]  # a single and a double window over the garage
LEAF_TOP = PASSAGE_FLOOR + 1.1  # the front door's, over the porch floor
DOOR = (door_u - DOOR_HOLE, SLAB, door_u + DOOR_HOLE, LEAF_TOP + 0.05)
OFFICE_WINDOW = (window_u - 0.3, GROUND_GLASS[0], window_u + 0.3, GROUND_GLASS[1])
# The gable's wall, full height: the window behind the office, the door at the porch's back,
# the twin windows.
holes = [DOOR, OFFICE_WINDOW, (TWIN[0], UPPER_GLASS[0], TWIN[1], TWIN_SPRING + TWIN_RISE)]
wall("FrontWall", front, 0, fx(GABLE_WALL), 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
twin_mid = (TWIN[0] + TWIN[1]) / 2
for a, b in ((TWIN[0], twin_mid - 0.02), (twin_mid + 0.02, TWIN[1])):
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), TWIN_SPRING + TWIN_RISE, mullion=False)
    arch_head("FrontWall", front, fx(a), fx(b), TWIN_SPRING, (b - a) / 2, depth=WALL)
front.block("FrontWall__Trim", fx(twin_mid - 0.02), UPPER_GLASS[0], fx(twin_mid + 0.02), TWIN_SPRING + 0.04, 0.1, 0.03)
# The window behind the office: the house keeps out of the entry's box (it stands whole once
# the entry is gone), so its sill stands only 0.03 proud.
window("FrontWall", front, fx(OFFICE_WINDOW[0]), OFFICE_WINDOW[1], fx(OFFICE_WINDOW[2]), OFFICE_WINDOW[3], sill=False)
front.block("FrontWall__Trim", fx(OFFICE_WINDOW[0]) - 0.03, OFFICE_WINDOW[1] - 0.035, fx(OFFICE_WINDOW[2]) + 0.03, OFFICE_WINDOW[1], 0.0, -0.03, faces=("front", "+y", "-y", "+u", "-u"))

# The gable's stucco over the wall top, a clay vent in its peak.
_x0, _x1 = -HALF_W, GABLE_WALL
_outline = [(x, max(WALL_TOP, gable_y(x) - DECK * 1.1)) for x in (_x1, GABLE_X, _x0)]
_outline = [(_x0, WALL_TOP), (_x1, WALL_TOP)] + _outline
face("FrontWall__Siding", [(x, y, HALF_D) for x, y in _outline], (0, 0, 1))
face("FrontWall__Lining", [(x, y, HALF_D - WALL) for x, y in _outline], (0, 0, -1))
clay_vent("FrontWall", GABLE_X, APEX - DECK * 1.1 - 0.25, HALF_D, spacing=0.05)

# The front door at the porch's back, its leaf over the porch floor.
_a, _b = fx(door_u - DOOR_HALF), fx(door_u + DOOR_HALF)
_h0, _h1 = fx(DOOR[0]), fx(DOOR[2])
front.quad("FrontWall__Accent", _a, SLAB, _b, LEAF_TOP, 0.12)
for _y0, _y1 in ((PASSAGE_FLOOR + 0.08, PASSAGE_FLOOR + 0.48), (PASSAGE_FLOOR + 0.56, LEAF_TOP - 0.08)):
    front.block("FrontWall__Accent", _a + 0.06, _y0, _b - 0.06, _y1, 0.12, 0.105)
front.block("FrontWall__Trim", _h0, SLAB, _a, LEAF_TOP + 0.05, 0.12, 0.08)
front.block("FrontWall__Trim", _b, SLAB, _h1, LEAF_TOP + 0.05, 0.12, 0.08)
front.block("FrontWall__Trim", _a, LEAF_TOP, _b, LEAF_TOP + 0.05, 0.12, 0.08)
front.block("FrontWall__Iron", _b - 0.07, PASSAGE_FLOOR + 0.5, _b - 0.045, PASSAGE_FLOOR + 0.56, 0.105, 0.08)

# Beside the gable: the garage's storey on the front line up to the ledge, the upper floor
# set back over it, the gable's side between them.
wall("FrontWall", front, fx(GABLE_WALL), 2 * HALF_W, 0, LEDGE, [(fx(GARAGE[0]), GARAGE[1], fx(GARAGE[2]), GARAGE[3])])
upper = Frame((-HALF_W, 0, UPPER_Z), (1, 0, 0), (0, 0, 1))
UPPER_TOP = main_y(UPPER_Z) - DECK
wall("FrontWall", upper, fx(GABLE_WALL), 2 * HALF_W, LEDGE, UPPER_TOP, [(fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1]) for a, b in OVER_GARAGE])
for a, b in OVER_GARAGE:
    window("FrontWall", upper, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
face("FrontWall__Siding", [(GABLE_WALL, LEDGE, HALF_D), (GABLE_WALL, LEDGE, UPPER_Z), (GABLE_WALL, UPPER_TOP, UPPER_Z), (GABLE_WALL, gable_y(GABLE_WALL) - DECK * 1.1, HALF_D)], (1, 0, 0))

# The garage's one-piece roll-up door in stained boards, the clay pent over the ledge.
sectional_garage_door(front, garage_u, decor=None, boards=True, header=False)
PENT_TOP = PENT_Y + PITCH * (PENT_OUT + SETBACK)
tiled("FrontWall", Slope([(GABLE_WALL, PENT_Y, HALF_D + PENT_OUT), (HALF_W + 0.1, PENT_Y, HALF_D + PENT_OUT), (HALF_W + 0.1, PENT_TOP, UPPER_Z), (GABLE_WALL, PENT_TOP, UPPER_Z)]))

# ---------------------------------------------------------------- the entry
# The one-storey wing before the gable, a model of its own (`Entry__<Surface>`, which the
# game stands before the house as a cover) in the kit's office-and-entry box: the office on
# the outer side, its arched street window over a brick sill and a battered apron; the porch
# on the garage side, open to the street and the drive in stucco arches, its floor two
# risers up, the front door at its back; one low clay hip over both.
WING_X = (PASSAGE_X[0], OFFICE_SPAN[1])
WING_SPLIT = -1.55  # the office's inner wall: the coded window behind it stays covered
WING_FRONT = HALF_D + OFFICE_DEPTH
WING_TOP = 2.0  # where its hip meets the wall, under the twin windows' sills


def wing():
    x0, x1 = WING_X
    zw, zf = HALF_D, WING_FRONT
    wall_top = OFFICE_EAVE - 0.08
    head = DOOR_TOP + 0.05
    # the office's street face and arched window
    width = WING_SPLIT - x0
    face_frame = Frame((x0, 0, zf), (1, 0, 0), (0, 0, 1))
    w0, w1 = width / 2 - 0.31, width / 2 + 0.31
    wall("Entry", face_frame, 0, width, 0, wall_top, [(w0, GROUND_GLASS[0], w1, head)])
    window("Entry", face_frame, w0, GROUND_GLASS[0], w1, head, mullion=False, sill=False)
    face_frame.block("Entry__Trim", w0, (GROUND_GLASS[0] + head) / 2 - 0.015, w1, (GROUND_GLASS[0] + head) / 2 + 0.015, 0.1, 0.07)
    arch_head("Entry", face_frame, w0, w1, head - 0.18, 0.18, depth=WALL)
    face_frame.block("Entry__Brick", w0 - 0.14, GROUND_GLASS[0] - 0.07, w1 + 0.14, GROUND_GLASS[0], 0.0, -0.13)
    a, b = w0 - 0.12, w1 + 0.12
    top_y, out_top, out_foot = GROUND_GLASS[0] - 0.07, 0.11, 0.2
    face("Entry__Siding", [face_frame.p(a, 0, -out_foot), face_frame.p(b, 0, -out_foot), face_frame.p(b, top_y, -out_top), face_frame.p(a, top_y, -out_top)], (0, 0.3, 1))
    for u, sign in ((a, -1), (b, 1)):
        face("Entry__Siding", [face_frame.p(u, 0, 0), face_frame.p(u, 0, -out_foot), face_frame.p(u, top_y, -out_top), face_frame.p(u, top_y, 0)], face_frame.along * sign)
    # the office's outer side and its wall to the porch
    wall("Entry", Frame((x0, 0, zw), (0, 0, 1), (-1, 0, 0)), 0, OFFICE_DEPTH, 0, wall_top, [])
    wall("Entry", Frame((WING_SPLIT, 0, zf), (0, 0, -1), (1, 0, 0)), 0, OFFICE_DEPTH, 0, wall_top, [])
    # the porch: an arch to the street, one to the drive running back to the house wall
    spring = PASSAGE_FLOOR + 1.0
    span = x1 - WING_SPLIT
    street = Frame((WING_SPLIT, 0, zf), (1, 0, 0), (0, 0, 1))
    wall("Entry", street, 0, span, 0, wall_top, [(0.14, PASSAGE_FLOOR, span - 0.14, spring + 0.2)])
    arch_head("Entry", street, 0.14, span - 0.14, spring, 0.2, depth=WALL)
    drive = Frame((x1, 0, zf), (0, 0, -1), (1, 0, 0))
    wall("Entry", drive, 0, OFFICE_DEPTH, 0, wall_top, [(0.3, PASSAGE_FLOOR, OFFICE_DEPTH, spring + 0.2)])
    arch_head("Entry", drive, 0.3, OFFICE_DEPTH, spring, 0.2, depth=WALL)
    face("Entry__Siding", [(WING_SPLIT, wall_top, zw), (x1, wall_top, zw), (x1, wall_top, zf), (WING_SPLIT, wall_top, zf)], (0, -1, 0))
    # its floor, run to the arches' walls' inner faces (its top meeting their openings'
    # sills rather than lying on them), and the steps up to it
    box("Entry__Concrete", (WING_SPLIT, 0.0, zw), (x1 - WALL, PASSAGE_FLOOR, zf - WALL))
    risers = round(PASSAGE_FLOOR / 0.1)
    for k in range(risers):
        box("Entry__Concrete", (WING_SPLIT + 0.16, 0.0, zf), (x1 - 0.16, PASSAGE_FLOOR - PASSAGE_FLOOR / risers * k, zf + 0.2 * (k + 1)))
    # one low hip over the office and the porch, leaning on the house wall
    xa, xb, ze = x0 - 0.12, x1 + 0.02, zf + OFFICE_OVER
    xm, half = (xa + xb) / 2, (xb - xa) / 2
    rz = max(zw, ze - half)
    eave = OFFICE_EAVE
    tiled("Entry", Slope([(xa, eave, ze), (xb, eave, ze), (xm, WING_TOP, rz)]))
    tiled("Entry", Slope([(xa, eave, zw), (xa, eave, ze), (xm, WING_TOP, rz), (xm, WING_TOP, zw)]))
    tiled("Entry", Slope([(xb, eave, ze), (xb, eave, zw), (xm, WING_TOP, zw), (xm, WING_TOP, rz)]))
    ridge_cap("Entry", (xa, eave, ze), (xm, WING_TOP, rz))
    ridge_cap("Entry", (xb, eave, ze), (xm, WING_TOP, rz))
    ridge_cap("Entry", (xm, WING_TOP, rz), (xm, WING_TOP, zw))
    top = max(v.y for key, (vertices, _) in geometry.items() if key.startswith("Entry__") for v in vertices)
    house_kit.ENTRY_BOX.update(x=(x0 + x1) / 2, z=(zw + zf) / 2, w=x1 - x0, d=zf - zw, h=round(top, 3))


wing()

# ---------------------------------------------------------------- the flanks and back
def flank_gable(side, z0, z1):
    """A flank's stucco gable end over the wall top from z0 to z1, under the main roof."""
    x = HALF_W if side == "right" else -HALF_W
    outward = 1 if side == "right" else -1
    part = WALL_PARTS[side]
    foot = (RIDGE_Y - DECK * 1.1 - WALL_TOP) / MAIN_PITCH  # where the stucco rises off the wall top
    outline = [(max(z0, -foot), WALL_TOP), (min(z1, foot), WALL_TOP)]
    if z1 < foot:
        outline.append((z1, main_y(z1) - DECK * 1.1))
    outline.append((0.0, RIDGE_Y - DECK * 1.1))
    face(f"{part}__Siding", [(x, y, z) for z, y in outline], (outward, 0, 0))
    face(f"{part}__Lining", [(x - outward * WALL, y, z) for z, y in outline], (-outward, 0, 0))


def flank_vent(side, y):
    """Three clay pipe ends in a flank's gable, in a triangle."""
    frame, part = WALL_FRAMES[side], WALL_PARTS[side]
    for du, dy in ((-0.025, 0), (0.025, 0), (0, 0.0425)):
        frame.block(f"{part}__Terracotta", du - 0.022, y + dy - 0.022, du + 0.022, y + dy + 0.022, 0.02, -0.03)
        frame.block(f"{part}__Dark", du - 0.012, y + dy - 0.012, du + 0.012, y + dy + 0.012, -0.03, -0.032)


# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-1.6: 0.42, 0.4: 0.42, 2.1: 0.3})
plain_wall("left", PLAN, half_width=0.3)
flank_gable("left", -HALF_D, HALF_D)
flank_vent("left", RIDGE_Y - DECK * 1.1 - 0.3)
# the garage side stops at the upper floor's setback; under the ledge it runs on to the front
plain_wall("right", PLAN, half_width=0.27, span=(-UPPER_Z, HALF_D))
flank_gable("right", -HALF_D, UPPER_Z)
right = WALL_FRAMES["right"]
wall("RightWall", right, -HALF_D, -UPPER_Z, 0, LEDGE, [])


def pent_under(z):
    return PENT_Y + PITCH * (HALF_D + PENT_OUT - z) - DECK


face("RightWall__Siding", [(HALF_W, LEDGE, HALF_D), (HALF_W, LEDGE, UPPER_Z), (HALF_W, pent_under(UPPER_Z), UPPER_Z), (HALF_W, pent_under(HALF_D), HALF_D)], (1, 0, 0))

export(PLAN)
