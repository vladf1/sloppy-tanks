# The Senda large Craftsman house, modelled after Rumbo north lots 3, 7 and 11, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/large-craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes assets/models/houses/large-craftsman.glb and
#   crates/core/data/houses/large-craftsman.plan.json)
#
# Drawn in the street frame on the 6.2 x 8 m lot the kit measures, its garage on the right;
# the game builds it with its door in the eave wall (its ridge along the street) and fits it
# to every lot of its styles. A broad two-storey stucco block under a low flat-tile roof
# gabled at both flanks (a louvred vent high in each gable), its ridge the full width of the
# house; across the front left a one-storey wing under one flat-tile shed: a stucco office
# at the outer corner with a wide window to the street and, beside it by the garage, an
# open porch between two brick piers with the front door at its back, steps straight out
# to the walk; the garage on the right with a one-piece board door hung with strap hinges;
# over it, at the house's right corner, a balcony between stucco cheeks with a railing and
# French doors behind; across the upper floor two windows over the wing and a shuttered
# one beside the balcony. Solar panels on the street slope: a block under the ridge with a
# row along the eave and short columns at its ends (part of lot 3's ring), and a small block
# at the west end of the back slope. The paints are the style's.
#
# Drone footage (from above, the front and the west flank) showed the roof gabled, not
# hipped; the game's structure keeps the coded hip, so each long slope is drawn in three
# pieces: the middle (the hip's long slope) and, beyond the hip lines, the ends (the hip
# ends' parts), so the ends still fall with the hip ends.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import house_kit  # noqa: E402
from house_kit import *  # noqa: E402,F403

# The coded large Craftsman front (the window and door in the wing's half: the office's
# back stands for the window, the porch's front door is the door; the garage's leaves, the
# window over the door and the two over the garage, here the shuttered window and the
# balcony's doors) and the window over the office; two windows a floor down each flank and
# three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.66, 1.0, 1.8, -1.91], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-2.0, "Window"], [0.0, "Window"], [2.0, "Window"]], "upper": [-2.0, 0.0, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.5, "Window"]], "upper": [-2.3, 1.5], "garage_bay": 0.0},
        "left": {"ground": [[-1.8, "Window"], [1.9, "Window"]], "upper": [-1.8, 1.9], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
entry_window_u = _front["ground"][0][0]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]


# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
# The upper windows, each over its planned centre: one over the office, one over the porch
# (towards the office, as on lot 3), a shuttered one left of the balcony; the balcony's
# doors at the house's right corner.
OVER_OFFICE = (entry_window_u - 0.28, entry_window_u + 0.28)
OVER_PORCH = (-0.85 - 0.28, -0.85 + 0.28)
SHUTTERED = (0.9 - 0.26, 0.9 + 0.26)
BAL_FLOOR_TOP = 1.6  # the balcony's floor, clear of the garage door's head trim
BALCONY_DOOR = (1.6, BAL_FLOOR_TOP, 2.6, 2.86)  # French doors over the planned 1.8
# The ground window behind the office (the wing hides it; the wall shows it once the wing
# is gone), narrow enough to stay inside the office's inner wall.
OFFICE_BACK = (entry_window_u - 0.3, GROUND_GLASS[0], entry_window_u + 0.3, GROUND_GLASS[1])
holes = [
    GARAGE,
    door_hole(door_u),
    OFFICE_BACK,
    (OVER_OFFICE[0], UPPER_GLASS[0], OVER_OFFICE[1], UPPER_GLASS[1]),
    (OVER_PORCH[0], UPPER_GLASS[0], OVER_PORCH[1], UPPER_GLASS[1]),
    (SHUTTERED[0], UPPER_GLASS[0], SHUTTERED[1], UPPER_GLASS[1]),
    BALCONY_DOOR,
]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])

for a, b in (OVER_OFFICE, OVER_PORCH):
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=False)
window("FrontWall", front, fx(SHUTTERED[0]), UPPER_GLASS[0], fx(SHUTTERED[1]), UPPER_GLASS[1], mullion=False)
board_shutters(front, fx(SHUTTERED[0]), fx(SHUTTERED[1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02)
# no sill: it would stand into the wing's box
window("FrontWall", front, fx(OFFICE_BACK[0]), OFFICE_BACK[1], fx(OFFICE_BACK[2]), OFFICE_BACK[3], mullion=False, sill=False)

# The front door at the porch's back.
front_door(front, door_u)

# The garage's one-piece door: boards, four sections, strap hinges, under a dark header.
sectional_garage_door(front, garage_u, decor="straps", boards=True, header=True)
lantern(front, GARAGE[2] + 0.17)

# ---------------------------------------------------------------- the balcony
BAL = (BALCONY_DOOR[0] - 0.15, BALCONY_DOOR[2] + 0.15)  # cheeks' outer edges, round the doors
BAL_OUT = 0.32
BAL_FLOOR = (BAL_FLOOR_TOP - 0.1, BAL_FLOOR_TOP)
BAL_HEAD = (2.9, 3.0)
CHEEK = (0.12, 0.12)  # left and right cheek widths
zb = HALF_D + BAL_OUT
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[0], HALF_D), (BAL[1], BAL_FLOOR[1], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[1], HALF_D), (BAL[0] + CHEEK[0], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[1] - CHEEK[1], BAL_FLOOR[1], HALF_D), (BAL[1], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_HEAD[0], HALF_D), (BAL[1], BAL_HEAD[1], zb), skip=("-z",))
# a dark stained frame inside the opening and a trim band under the floor
box("FrontWall__Wood", (BAL[0] + CHEEK[0], BAL_HEAD[0] - 0.06, zb - 0.08), (BAL[1] - CHEEK[1], BAL_HEAD[0], zb), skip=("+y", "-z"))
box("FrontWall__Trim", (BAL[0] - 0.02, BAL_FLOOR[0] - 0.04, zb - 0.02), (BAL[1] + 0.02, BAL_FLOOR[0] + 0.02, zb + 0.02), skip=("-z", "+y"))
# the railing, a real metre high
r0, r1 = BAL[0] + CHEEK[0], BAL[1] - CHEEK[1]
rz0, rz1 = zb - 0.06, zb - 0.04
RAIL = BAL_FLOOR[1] + 0.53
box("FrontWall__Iron", (r0, RAIL - 0.03, rz0 - 0.01), (r1, RAIL, rz1 + 0.01))
box("FrontWall__Iron", (r0, BAL_FLOOR[1] + 0.05, rz0), (r1, BAL_FLOOR[1] + 0.07, rz1))
x = r0 + 0.07
while x < r1 - 0.03:
    box("FrontWall__Iron", (x - 0.008, BAL_FLOOR[1] + 0.07, rz0), (x + 0.008, RAIL - 0.03, rz1), skip=("+y", "-y"))
    x += 0.07
# the French doors behind
b0, b1 = fx(BALCONY_DOOR[0]), fx(BALCONY_DOOR[2])
by0, by1 = BALCONY_DOOR[1], BALCONY_DOOR[3]
front.quad("FrontWall__Glass", b0, by0, b1, by1, 0.12)
bm = (b0 + b1) / 2
t = 0.04
for a, c, b, d in ((b0, by0, b1, by0 + 0.09), (b0, by1 - t, b1, by1), (b0, by0, b0 + t, by1), (b1 - t, by0, b1, by1), (bm - t, by0, bm + t, by1)):
    front.block("FrontWall__Wood", a, c, b, d, 0.12, 0.09)
for u in ((b0 + bm) / 2, (bm + b1) / 2):
    front.block("FrontWall__Wood", u - 0.01, by0 + 0.09, u + 0.01, by1 - t, 0.12, 0.105)
front.block("FrontWall__Wood", b0, 2.16, b1, 2.18, 0.12, 0.105)


# ---------------------------------------------------------------- the wing
# One flat-tile shed across the front left, leaning on the wall under the upper sills: the
# office at the outer corner, the open porch by the garage between two brick piers. It has
# the kit entry's box (the passage's outer side to the office's inner side, 1.5 m out), so
# the game's cover for it is unchanged; drawn here because the kit's entry puts the passage
# outside and the office by the garage, lot 3 the other way round.
WING = (-HALF_W, -0.3)  # the wing's x, its box's (the kit's PASSAGE_X[0], OFFICE_SPAN[1])
OFFICE_X = (-HALF_W, -1.6)
PORCH_X = (OFFICE_X[1], WING[1])
WING_DEPTH = 1.5  # the kit's OFFICE_DEPTH
SHED_TOP, SHED_EAVE, SHED_OVER = 2.05, 1.72, 0.25  # the kit's office shed
PORCH_FLOOR = 0.24  # two risers up from the walk
BRICK_PIER = 0.24  # the porch piers' square (real 0.45 m)


def porch_wing(wall_z=HALF_D):
    """The office and porch under their shed, drawn as FrontWall__ and moved to Entry__ as
    the kit's office_entry does; records the kit's ENTRY_BOX."""
    before = {key: len(faces) for key, (_, faces) in geometry.items() if key.startswith("FrontWall__")}
    zf = wall_z + WING_DEPTH
    # the shed stops just past the flank (seen from behind, a wider overhang read as a stub)
    xa, xb, ze = WING[0] - 0.1, WING[1] + 0.1, zf + SHED_OVER

    def under(z):
        """The shed's underside over z."""
        return SHED_EAVE + (SHED_TOP - SHED_EAVE) * (ze - z) / (ze - wall_z) - DECK

    wall_top = SHED_EAVE - 0.08
    # the office's street face and its wide window
    ox0, ox1 = OFFICE_X
    width = ox1 - ox0
    face_frame = Frame((ox0, 0, zf), (1, 0, 0), (0, 0, 1))
    w0, w1 = width / 2 - 0.38, width / 2 + 0.38
    head = DOOR_TOP + 0.05
    wall("FrontWall", face_frame, 0, width, 0, wall_top, [(w0, GROUND_GLASS[0], w1, head)])
    window("FrontWall", face_frame, w0, GROUND_GLASS[0], w1, head)
    face_frame.block("FrontWall__Trim", w0, (GROUND_GLASS[0] + head) / 2 - 0.015, w1, (GROUND_GLASS[0] + head) / 2 + 0.015, 0.1, 0.07)
    # its side walls, plain, closed up to the shed
    for x, outward in ((ox0, -1), (ox1, 1)):
        frame = Frame((x, 0, wall_z), (0, 0, 1), (-1, 0, 0)) if outward < 0 else Frame((x, 0, zf), (0, 0, -1), (1, 0, 0))
        wall("FrontWall", frame, 0, WING_DEPTH, 0, wall_top, [])
        face("FrontWall__Siding", [(x, wall_top, wall_z), (x, wall_top, zf), (x, under(zf), zf), (x, under(wall_z), wall_z)], (outward, 0, 0))
    # the shed, its fascia on the eave and both ends
    tiled("FrontWall", Slope([(xa, SHED_EAVE, ze), (xb, SHED_EAVE, ze), (xb, SHED_TOP, wall_z), (xa, SHED_TOP, wall_z)]), eave_edges=(0, 1, 3), fascia=False, flat=True)
    # the porch: its floor, steps straight out to the walk, a brick pier at each front corner
    # under a header beam to the office, a beam back to the wall over the open side
    px0, px1 = PORCH_X
    # (its side to the office lies in the office's wall)
    box("FrontWall__Concrete", (px0, 0.0, wall_z), (px1, PORCH_FLOOR, zf), skip=("-x",))
    for k in range(2):
        box("FrontWall__Concrete", (px0 + 0.3, 0.0, zf), (px1 - 0.35, PORCH_FLOOR - PORCH_FLOOR / 2 * k, zf + 0.2 * (k + 1)))
    beam_top = under(zf) - 0.01
    beam = (beam_top - 0.14, beam_top)
    # each pier stands proud of the floor's front (and the right one of its side), whose
    # faces would otherwise lie on the pier's
    for x0 in (px0, px1 - BRICK_PIER + ENTRY_INSET):
        z1 = zf + ENTRY_INSET
        box("FrontWall__Brick", (x0, 0.0, z1 - BRICK_PIER), (x0 + BRICK_PIER, beam[0] - 0.04, z1))
        box("FrontWall__Concrete", (x0 - 0.02, beam[0] - 0.04, z1 - BRICK_PIER - 0.02), (x0 + BRICK_PIER + 0.02, beam[0], z1 + 0.02))
    box("FrontWall__Wood", (px0, beam[0], zf - 0.16), (px1, beam[1], zf), skip=("+y",))
    box("FrontWall__Wood", (px1 - 0.16, beam[0], wall_z), (px1, beam[1], zf - 0.16), skip=("+y",))

    top = 0.0
    for key in [key for key in geometry if key.startswith("FrontWall__")]:
        start = before.get(key, 0)
        vertices, faces = geometry[key]
        for f in faces[start:]:
            top = max(top, *(vertices[i][1] for i in f))
        house_kit._move_faces(key, "Entry__" + key.split("__")[1], start)
    ENTRY_BOX.update(x=(WING[0] + WING[1]) / 2, z=(wall_z + zf) / 2, w=WING[1] - WING[0], d=zf - wall_z, h=round(top, 3))


porch_wing()

# ---------------------------------------------------------------- the roof
# Gabled at both flanks, its ridge the house's width. Each long slope is cut along the
# coded hip's lines: the middle is the hip's long slope's part, the ends (to the rakes) the
# hip ends' parts, as the game's structure has them.
EX, EZ, EY = ALONG_EAVE[0], ALONG_EAVE[1], ALONG_EAVE_Y
RH = ALONG_RIDGE_HALF
hip = along_hip()
tiled("FrontRoof", Slope(hip["front"]), eave_edges=(0,), fascia=False, flat=True)
tiled("BackRoof", Slope(hip["back"]), eave_edges=(0,), fascia=False, flat=True)
for side, part in ((1, "RightRoof"), (-1, "LeftRoof")):
    for zs in (1, -1):
        # first edge along the ridge, then the rake down to the eave corner
        tiled(part, Slope([(side * RH, RIDGE_Y, 0.0), (side * EX, RIDGE_Y, 0.0), (side * EX, EY, zs * EZ)]), eave_edges=(1,), fascia=False, flat=True)
    ridge_cap(part, (side * RH, RIDGE_Y, 0.0), (side * EX, RIDGE_Y, 0.0))
ridge_cap("FrontRoof", (-RH, RIDGE_Y, 0.0), (RH, RIDGE_Y, 0.0))


def gable_end(side):
    """A flank's stucco gable over its wall top under the rakes, lined inside, and a louvred
    vent high in it."""
    part = "RightWall" if side > 0 else "LeftWall"
    x = side * HALF_W
    peak = EY + ALONG_PITCH * EZ - DECK * 1.1
    outline = [(x, WALL_TOP, -HALF_D), (x, WALL_TOP, HALF_D), (x, peak, 0.0)]
    face(f"{part}__Siding", outline, (side, 0, 0))
    face(f"{part}__Lining", [(x - side * WALL, y, z) for _, y, z in outline], (-side, 0, 0))
    # the vent: a trim frame round a dark louvred face (real 0.6 x 0.4 m)
    frame = WALL_FRAMES["right" if side > 0 else "left"]
    vy, vw, vh = peak - 0.42, 0.16, 0.11
    frame.quad(f"{part}__Dark", -vw, vy - vh, vw, vy + vh, -0.003)
    for a, c, b, d in ((-vw - 0.03, vy - vh - 0.03, vw + 0.03, vy - vh), (-vw - 0.03, vy + vh, vw + 0.03, vy + vh + 0.03), (-vw - 0.03, vy - vh, -vw, vy + vh), (vw, vy - vh, vw + 0.03, vy + vh)):
        frame.block(f"{part}__Trim", a, c, b, d, 0.0, -0.02)
    for k in (-1, 0, 1):
        frame.block(f"{part}__Trim", -vw, vy + k * 0.06 - 0.008, vw, vy + k * 0.06 + 0.008, 0.0, -0.012)


# ---------------------------------------------------------------- flanks and back
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-2.0: 0.4, 0.0: 0.4, 2.0: 0.3})
plain_wall("right", PLAN, half_width=0.27)
# the west flank's upper floor: a pair of windows and a wider one towards the front
plain_wall("left", PLAN, half_width=0.27, upper_half={-1.8: 0.4, 1.9: 0.45})
gable_end(1)
gable_end(-1)


# ---------------------------------------------------------------- solar
# Panels laid landscape (a real 1.7 x 1.0 m panel across the slope), placed by their
# slope's u (along the eave, from its first corner) and v (up from the eave).
LANDSCAPE = (SOLAR_PANEL[1], SOLAR_PANEL[0])


def panels(part, slope, u0, v0, cols, rows, size=LANDSCAPE):
    for c in range(cols):
        for r in range(rows):
            pu = u0 + c * (size[0] + SOLAR_GAP)
            pv = v0 + r * (size[1] + SOLAR_GAP)
            corners = [(pu, pv), (pu + size[0], pv), (pu + size[0], pv + size[1]), (pu, pv + size[1])]
            face(part + "__Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


# The street slope (u from its left eave corner): part of lot 3's ring, a row along the
# eave and a column up from each end, round a block between them.
street = Slope([(-EX, EY, EZ), (EX, EY, EZ), (EX, RIDGE_Y, 0.0), (-EX, RIDGE_Y, 0.0)])
row = 5 * LANDSCAPE[0] + 4 * SOLAR_GAP
ring_u = EX - row / 2 + 0.1
panels("FrontRoof", street, ring_u, 0.6, 5, 1)
for c in (0, 4):
    panels("FrontRoof", street, ring_u + c * (LANDSCAPE[0] + SOLAR_GAP), 0.6 + LANDSCAPE[1] + SOLAR_GAP, 1, 3)
panels("FrontRoof", street, EX + 0.1 - LANDSCAPE[0] - SOLAR_GAP / 2, 1.45, 2, 3)
# The back slope (u from its right eave corner): a block at its west end.
back = Slope([(EX, EY, -EZ), (-EX, EY, -EZ), (-EX, RIDGE_Y, 0.0), (EX, RIDGE_Y, 0.0)])
panels("BackRoof", back, 3.0, 0.35, 3, 2)

export(PLAN, frame="along")
