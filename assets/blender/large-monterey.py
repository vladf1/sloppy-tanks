# The Senda large Monterey house, modelled after Rumbo north lot 6, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/large-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/large-monterey.glb and
#   crates/core/data/houses/large-monterey.plan.json)
#
# Drawn in the street frame on the 6.2 x 8 m lot the kit measures, its garage on the right;
# the game builds it with its door in the eave wall. A honey stucco two-storey block under
# one clay hip whose hips rise from all four corners to a short ridge running back from the
# street (seen from above, a broad triangle faces the street), a two-storey front gable over
# its entry side whose left slope is the left hip's plane carried forward, a stucco box bay
# of three windows high in it under a clay hood; before that side the tract's entry: a brick
# office with a blue-shuttered window and, beside it toward the garage, a porch to the front
# door, both under one small clay front gable on a dark post; over the garage's plain
# one-piece door two windows, each with its own board shutters, the right one larger over a
# shelf. Solar panels in two blocks: six on the street triangle, eight on the left slope.
# The small Monterey (Rumbo north lot 4) is small-monterey.py.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403
from house_kit import _move_faces  # noqa: E402

# The coded front (the street's layout reads its door and garage: the porch's front door is
# the door, the window behind the office the ground window; no window over the door) and
# the box bay's window over the entry; two windows a floor down each flank and three
# across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [0.85, 1.85, -1.91], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-2.0, "Window"], [0.0, "Window"], [2.0, "Window"]], "upper": [-2.0, 0.0, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.5, "Window"]], "upper": [-2.3, 1.5], "garage_bay": 0.0},
        "left": {"ground": [[-1.8, "Window"], [1.9, "Window"]], "upper": [-1.8, 1.9], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
office_window_u = _front["ground"][0][0]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]
upper = _front["upper"]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
# Over the garage two windows apart, each between its own pair of narrow board shutters: a
# smaller one toward the gable, its sill a little higher, and a larger one on a shelf. The
# game's front keeps its coded upper windows over the garage (1.0 and 1.8 on this lot,
# house_layout.rs garage_front), so each window's glass spans one of them at the coded
# window's middle height; the small one sits as far toward the gable as that allows (the
# real one is nearer still).
SMALL = (upper[0] - 0.25, UPPER_GLASS[0] + 0.1, upper[0] + 0.25, UPPER_GLASS[1])
LARGE = (upper[1] - 0.27, UPPER_GLASS[0], upper[1] + 0.27, UPPER_GLASS[1])
UPPER_SHUTTER = 0.2
BAY = (upper[2] - 0.66, upper[2] + 0.66)  # the box bay high in the gable
BAY_LO, BAY_HI = 2.12, 2.96
BAY_HOLE = (BAY[0] + 0.07, BAY_LO + 0.06, BAY[1] - 0.07, BAY_HI - 0.04)
# the porch's front door, its leaf over the porch floor
DOOR = (door_u - DOOR_HOLE, SLAB, door_u + DOOR_HOLE, PASSAGE_FLOOR + 1.1 + 0.05)
# a window from the house into the office behind it
OFFICE_WINDOW = (office_window_u - WINDOW_HALF, GROUND_GLASS[0], office_window_u + WINDOW_HALF, GROUND_GLASS[1])
holes = [GARAGE, DOOR, BAY_HOLE, OFFICE_WINDOW, SMALL, LARGE]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor=None, boards=False)
lantern(front, GARAGE[2] + 0.17)
# (no sill: it would stand out into the entry's box)
window("FrontWall", front, fx(OFFICE_WINDOW[0]), OFFICE_WINDOW[1], fx(OFFICE_WINDOW[2]), OFFICE_WINDOW[3], sill=False)

# The front door at the back of the porch: a painted leaf in two raised panels over the
# porch floor, its casing filling the hole, a knob.
a, b = fx(door_u - DOOR_HALF), fx(door_u + DOOR_HALF)
h0, h1 = fx(DOOR[0]), fx(DOOR[2])
leaf_top = DOOR[3] - 0.05
front.quad("FrontWall__Accent", a, SLAB, b, leaf_top, 0.12)
front.block("FrontWall__Accent", a + 0.06, PASSAGE_FLOOR + 0.56, b - 0.06, leaf_top - 0.08, 0.12, 0.105)
front.block("FrontWall__Accent", a + 0.06, PASSAGE_FLOOR + 0.08, b - 0.06, PASSAGE_FLOOR + 0.48, 0.12, 0.105)
front.block("FrontWall__Trim", h0, SLAB, a, DOOR[3], 0.12, 0.08)
front.block("FrontWall__Trim", b, SLAB, h1, DOOR[3], 0.12, 0.08)
front.block("FrontWall__Trim", a, leaf_top, b, DOOR[3], 0.12, 0.08)
front.block("FrontWall__Iron", a + 0.045, PASSAGE_FLOOR + 0.5, a + 0.07, PASSAGE_FLOOR + 0.56, 0.105, 0.08)

# The two windows over the garage, blue board shutters beside each; a shelf on two
# brackets under the larger one.
for (u0, y0, u1, y1), sill in ((SMALL, True), (LARGE, False)):
    window("FrontWall", front, fx(u0), y0, fx(u1), y1, mullion=False, sill=sill)
    board_shutters(front, fx(u0), fx(u1), y0 - 0.01, y1 + 0.01, width=UPPER_SHUTTER)
s0, s1 = fx(LARGE[0]) - 0.05, fx(LARGE[2]) + 0.05
front.block("FrontWall__Wood", s0, LARGE[1] - 0.05, s1, LARGE[1] - 0.01, 0.0, -0.08)
for u in (s0 + 0.1, s1 - 0.1):
    front.block("FrontWall__Wood", u - 0.025, LARGE[1] - 0.17, u + 0.025, LARGE[1] - 0.05, 0.0, -0.06)

# ---------------------------------------------------------------- the box bay
# Three windows high in the front gable, over the entry side, in a shallow box under a tile
# hood; it stands clear of the entry's roof below.
BAY_DEPTH = 0.22
b0, b1 = fx(BAY[0]), fx(BAY[1])
for a, c, b, d in ((b0, BAY_LO, b1, BAY_LO + 0.07), (b0, BAY_HI - 0.04, b1, BAY_HI), (b0, BAY_LO, b0 + 0.05, BAY_HI), (b1 - 0.05, BAY_LO, b1, BAY_HI)):
    front.block("FrontWall__Wood", a, c, b, d, 0.0, -BAY_DEPTH)
for su, sign in ((b0, -1), (b1, 1)):
    face("FrontWall__Wood", [front.p(su, BAY_LO, 0), front.p(su, BAY_HI, 0), front.p(su, BAY_HI, -BAY_DEPTH), front.p(su, BAY_LO, -BAY_DEPTH)], front.along * sign)
face("FrontWall__Wood", [front.p(b0, BAY_LO, 0), front.p(b0, BAY_LO, -BAY_DEPTH), front.p(b1, BAY_LO, -BAY_DEPTH), front.p(b1, BAY_LO, 0)], (0, -1, 0))
lights = 3
pane = (b1 - b0 - 0.1) / lights
gy0, gy1 = BAY_LO + 0.07, BAY_HI - 0.04
for k in range(lights):
    wu0, wu1 = b0 + 0.05 + pane * k, b0 + 0.05 + pane * (k + 1)
    glazing("FrontWall", front, wu0, gy0, wu1, gy1, -BAY_DEPTH + 0.03)
    for a, c, b, d, inner in ((wu0, gy0, wu1, gy0 + FRAME, "+y"), (wu0, gy1 - FRAME, wu1, gy1, "-y"), (wu0, gy0 + FRAME, wu0 + FRAME, gy1 - FRAME, "+u"), (wu1 - FRAME, gy0 + FRAME, wu1, gy1 - FRAME, "-u")):
        front.block("FrontWall__Trim", a, c, b, d, -BAY_DEPTH + 0.03, -BAY_DEPTH + 0.005, faces=("front", inner))
    if k:
        front.block("FrontWall__Wood", wu0 - 0.018, gy0, wu0 + 0.018, gy1, -BAY_DEPTH + 0.02, -BAY_DEPTH - 0.008)
hood = HALF_D + BAY_DEPTH + 0.08
tiled("FrontWall", Slope([(BAY[0] - 0.07, BAY_HI + 0.02, hood), (BAY[1] + 0.07, BAY_HI + 0.02, hood), (BAY[1] + 0.07, BAY_HI + 0.02 + 0.42 * (hood - HALF_D), HALF_D), (BAY[0] - 0.07, BAY_HI + 0.02 + 0.42 * (hood - HALF_D), HALF_D)]))

# ---------------------------------------------------------------- the roof
# One clay hip over the block, every slope at the same pitch, so its hips rise at 45
# degrees in plan from the four corners to a short ridge running back from the street: the
# street sees a broad triangle, the flanks long slopes. The front gable over the entry side
# spans from the left eave to the garage side's eave; its left slope is the left slope's
# plane carried forward, its ridge runs back to the front left hip (where the two meet,
# since the pitches match) and its right slope is a triangle down to a valley that meets
# the eave at the gable's right corner.
hip = main_hip()
ridge_back, ridge_front = hip["ridge"]
GABLE = (-EAVE_X, -0.25)  # the front gable's eaves
GABLE_X = (GABLE[0] + GABLE[1]) / 2  # its ridge
GABLE_APEX = EAVE_Y + PITCH * (GABLE[1] - GABLE[0]) / 2
GABLE_MEET = EAVE_Z - (GABLE_APEX - EAVE_Y) / PITCH  # where its ridge meets the hip
meet = (GABLE_X, GABLE_APEX, GABLE_MEET)
valley_foot = (GABLE[1], EAVE_Y, EAVE_Z)
corner_l, corner_r = (-EAVE_X, EAVE_Y, EAVE_Z), (EAVE_X, EAVE_Y, EAVE_Z)
street = Slope(hip["front"])  # the street triangle whole, for placing its panels
tiled("FrontRoof", Slope([valley_foot, corner_r, ridge_front, meet]))
tiled("BackRoof", Slope(hip["back"]))
tiled("RightRoof", Slope(hip["right"]))
left = Slope(hip["left"])
tiled("LeftRoof", left)
ridge_cap("LeftRoof", ridge_back, ridge_front)
ridge_cap("BackRoof", (-EAVE_X, EAVE_Y, -EAVE_Z), ridge_back)
ridge_cap("BackRoof", (EAVE_X, EAVE_Y, -EAVE_Z), ridge_back)
ridge_cap("FrontRoof", corner_r, ridge_front)
ridge_cap("FrontRoof", meet, ridge_front)

# The front gable: its two clay slopes (each hung from its ridge), its stucco triangle on
# the front wall over the bay, a clay vent in its peak.
gable_ridge = (GABLE_X, GABLE_APEX, EAVE_Z)
tiled("FrontRoof", Slope([meet, gable_ridge, corner_l]))
tiled("FrontRoof", Slope([gable_ridge, meet, valley_foot]))
ridge_cap("FrontRoof", gable_ridge, meet)
ridge_cap("FrontRoof", valley_foot, meet, half=0.04, lift=0.01)
peak = gable_face("FrontWall", HALF_D, -HALF_W, GABLE[1] - OVERHANG, WALL_TOP, GABLE[0], GABLE[1], EAVE_Y, PITCH, under=DECK * 1.1)
clay_vent("FrontWall", GABLE_X, peak - 0.16, HALF_D)


def panels(part, slope, rects):
    """Solar panels on `slope`, each (u0, v0, across, up) in its plane (u along its first
    edge from its first corner, v up the slope); one that would hang past the slope's
    edges stops the build."""
    for u0, v0, w, h in rects:
        corners = [(u0, v0), (u0 + w, v0), (u0 + w, v0 + h), (u0, v0 + h)]
        for u, v in corners:
            span = slope.span(1, v)
            assert span is not None and span[0] + 0.05 <= u <= span[1] - 0.05, f"{part}: a panel off the slope at {u:.2f}, {v:.2f}"
        face(part + "__Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


STEP_ACROSS = SOLAR_PANEL[0] + SOLAR_GAP
STEP_UP = SOLAR_PANEL[1] + SOLAR_GAP
# Six panels laid landscape on the street triangle right of the gable, in two columns of
# three, the right one a panel lower.
LONG, SHORT = SOLAR_PANEL[1], SOLAR_PANEL[0]
block_x = 0.35  # the block's middle
street_panels = []
for column, (u0, foot) in enumerate(((block_x - LONG - SOLAR_GAP / 2, 0.5 + SHORT + SOLAR_GAP), (block_x + SOLAR_GAP / 2, 0.5))):
    for k in range(3):
        street_panels.append((u0 + EAVE_X, foot + k * (SHORT + SOLAR_GAP), LONG, SHORT))
panels("FrontRoof", street, street_panels)
# Eight portrait panels on the left slope behind the gable, stepped up toward the ridge
# and the street: four along the eave, three over them, one at the top.
left_panels = [(z + EAVE_Z, 0.75, SHORT, LONG) for z in (-2.6 + STEP_ACROSS * k for k in range(4))]
left_panels += [(z + EAVE_Z, 0.75 + STEP_UP, SHORT, LONG) for z in (-1.85 + STEP_ACROSS * k for k in range(3))]
left_panels += [(-0.75 + EAVE_Z, 0.75 + 2 * STEP_UP, SHORT, LONG)]
panels("LeftRoof", left, left_panels)

# ---------------------------------------------------------------- the entry
# The office out from the gable side, its brick walls (a blue-shuttered window to the
# street, a slit in its outer side), and beside it toward the garage a porch up two steps to
# the front door, both under one small clay front gable whose pediment runs on over the
# porch on a beam to a dark post at its corner; the ridge stays under the bay. Exported as
# the `Entry` model like the kit's office ([`office_entry`]), which records its box.
OFFICE = (-3.05, -1.2)  # the office's x
PORCH = (OFFICE[1], -0.3)  # the porch's
ENTRY_EAVE = 1.62
ENTRY_PITCH = 0.27  # its ridge under the bay's foot


def office_and_porch(wall_z=HALF_D):
    before = {key: len(faces) for key, (_, faces) in geometry.items() if key.startswith("FrontWall__")}
    ox0, ox1 = OFFICE
    px0, px1 = PORCH
    zf = wall_z + OFFICE_DEPTH
    skin = "FrontWall__Brick"

    def slab(frame, length, height, holes):
        """A slab of the office's, in brick."""
        start = len(geometry["FrontWall__Siding"][1])
        wall("FrontWall", frame, 0, length, 0, height, holes)
        _move_faces("FrontWall__Siding", skin, start)

    xa, xb = ox0 - 0.2, px1 + 0.15
    xc = (xa + xb) / 2
    ze = zf + 0.25
    apex = ENTRY_EAVE + ENTRY_PITCH * (xb - xa) / 2
    assert apex < BAY_LO, "the entry's ridge runs into the bay"

    def roof_y(x):
        return ENTRY_EAVE + ENTRY_PITCH * ((xb - xa) / 2 - abs(x - xc)) - DECK

    wall_top = ENTRY_EAVE - 0.08

    # the office's street face and its shuttered window over a brick sill
    width = ox1 - ox0
    office = Frame((ox0, 0, zf), (1, 0, 0), (0, 0, 1))
    w0, w1 = width / 2 - 0.3, width / 2 + 0.3
    head = DOOR_TOP + 0.05
    slab(office, width, wall_top, [(w0, GROUND_GLASS[0], w1, head)])
    window("FrontWall", office, w0, GROUND_GLASS[0], w1, head, mullion=False, sill=False)
    office.block("FrontWall__Trim", w0, (GROUND_GLASS[0] + head) / 2 - 0.015, w1, (GROUND_GLASS[0] + head) / 2 + 0.015, 0.1, 0.07)
    for a in (w0 - SHUTTER_WIDTH - 0.02, w1 + 0.02):
        shutter("FrontWall", office, a, GROUND_GLASS[0] - 0.01, a + SHUTTER_WIDTH, head + 0.01)
    office.block("FrontWall__Brick", w0 - 0.06, GROUND_GLASS[0] - 0.07, w1 + 0.06, GROUND_GLASS[0], 0.0, -0.1)
    # its side walls: a slit window in the outer one, the inner one along the porch
    for x, outward in ((ox0, -1), (ox1, 1)):
        side = Frame((x, 0, wall_z), (0, 0, 1), (-1, 0, 0)) if outward < 0 else Frame((x, 0, zf), (0, 0, -1), (1, 0, 0))
        slits = [(OFFICE_DEPTH / 2 - 0.08, GROUND_GLASS[0], OFFICE_DEPTH / 2 + 0.08, head)] if outward < 0 else []
        slab(side, OFFICE_DEPTH, wall_top, slits)
        for hole in slits:
            window("FrontWall", side, *hole, mullion=False)
        if roof_y(x) > wall_top + 0.01:
            face(skin, [(x, wall_top, wall_z), (x, wall_top, zf), (x, roof_y(x), zf), (x, roof_y(x), wall_z)], (outward, 0, 0))
    # the roof and its pediment, over the office's front and on over the porch
    tiled("FrontWall", Slope([(xa, ENTRY_EAVE, wall_z), (xa, ENTRY_EAVE, ze), (xc, apex, ze), (xc, apex, wall_z)]))
    tiled("FrontWall", Slope([(xb, ENTRY_EAVE, ze), (xb, ENTRY_EAVE, wall_z), (xc, apex, wall_z), (xc, apex, ze)]))
    ridge_cap("FrontWall", (xc, apex, ze), (xc, apex, wall_z))
    outline = [(ox0, wall_top), (px1, wall_top), (px1, roof_y(px1)), (xc, roof_y(xc)), (ox0, roof_y(ox0))]
    face(skin, [(x, y, zf) for x, y in outline], (0, 0, 1))
    face("FrontWall__Lining", [(x, y, zf - WALL) for x, y in outline], (0, 0, -1))
    # the porch: its floor and two steps up to it, the post at its outer corner and the
    # beams from it to the office and back to the wall
    box("FrontWall__Concrete", (px0, 0.0, wall_z), (px1, PASSAGE_FLOOR, zf))
    risers = round(PASSAGE_FLOOR / 0.1)
    for k in range(risers):
        box("FrontWall__Concrete", (px0 + 0.05, 0.0, zf), (px1 - 0.02, PASSAGE_FLOOR - PASSAGE_FLOOR / risers * k, zf + 0.2 * (k + 1)))
    post_x, post_z = px1 - 0.09, zf - 0.09
    beam_y = wall_top
    box("FrontWall__Concrete", (post_x - 0.1, PASSAGE_FLOOR, post_z - 0.1), (post_x + 0.1, PASSAGE_FLOOR + 0.14, post_z + 0.1))
    box("FrontWall__Wood", (post_x - 0.07, PASSAGE_FLOOR + 0.14, post_z - 0.07), (post_x + 0.07, beam_y - 0.12, post_z + 0.07))
    box("FrontWall__Wood", (ox1, beam_y - 0.12, post_z - 0.07), (px1, beam_y, post_z + 0.07))
    box("FrontWall__Wood", (post_x - 0.07, beam_y - 0.12, wall_z), (post_x + 0.07, beam_y, post_z - 0.07))

    top = 0.0
    for key in [key for key in geometry if key.startswith("FrontWall__")]:
        start = before.get(key, 0)
        vertices, faces = geometry[key]
        for f in faces[start:]:
            top = max(top, *(vertices[i][1] for i in f))
        _move_faces(key, "Entry__" + key.split("__")[1], start)
    ENTRY_BOX.update(x=(ox0 + px1) / 2, z=(wall_z + zf) / 2, w=px1 - ox0, d=zf - wall_z, h=round(top, 3))


office_and_porch()

# ---------------------------------------------------------------- the flanks and back
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-2.0: 0.4, 0.0: 0.4, 2.0: 0.3})
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

export(PLAN, frame="along")
