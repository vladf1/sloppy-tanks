# The Senda large Monterey house, modelled after Rumbo north lot 6, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/large-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/large-monterey.glb and
#   crates/core/data/houses/large-monterey.plan.json)
#
# Drawn in the street frame on the 6.2 x 8 m lot the kit measures, its garage on the right;
# the game builds it with its door in the eave wall (its clay hip's ridge along the street).
# A honey stucco two-storey block under clay barrel tile with a two-storey front gable over
# its entry side, a dark stained box bay of three windows high in it; before that side the
# tract's entry: a brick office under a clay lean-to with a sage-shuttered window to the
# street and, along its outer side, steps and a passage to the front door under a small
# clay gable on a dark post; the garage's plain one-piece door on the right and a
# shuttered pair over it on a bracketed shelf; a block of panels on the street slope. The
# small Monterey (Rumbo north lot 4) is small-monterey.py.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded front (the street's layout reads its door and garage: the passage's front
# door stands for the window, the office's back for the door; no window over the door)
# and the box bay's window over the entry; two windows a
# floor down each flank and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8, -1.91], "garage_bay": GARAGE_BAY},
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
upper = _front["upper"]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
PAIR = [(u - 0.27, u + 0.27) for u in upper[:2]]  # the shuttered pair over the garage
BAY = (entry_window_u - 0.66, entry_window_u + 0.66)  # the box bay high in the gable
BAY_LO, BAY_HI = 2.12, 2.96
BAY_HOLE = (BAY[0] + 0.07, BAY_LO + 0.06, BAY[1] - 0.07, BAY_HI - 0.04)
holes = [GARAGE, door_hole(door_u), BAY_HOLE] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in PAIR]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor=None, boards=False)
lantern(front, GARAGE[2] + 0.17)

# The door from the office's back into the house.
front_door(front, door_u)

# The pair over the garage, sage board shutters outside them, on a shelf on two brackets.
for a, b in PAIR:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=False, sill=False)
board_shutters(front, fx(PAIR[0][0]), fx(PAIR[1][1]), UPPER_GLASS[0] - 0.01, UPPER_GLASS[1] + 0.01)
front.block("FrontWall__Wood", fx(PAIR[0][0]) - 0.05, UPPER_GLASS[0] - 0.05, fx(PAIR[1][1]) + 0.05, UPPER_GLASS[0] - 0.01, 0.0, -0.08)
for u in (fx(PAIR[0][0]) + 0.1, fx(PAIR[1][1]) - 0.1):
    front.block("FrontWall__Wood", u - 0.025, UPPER_GLASS[0] - 0.17, u + 0.025, UPPER_GLASS[0] - 0.05, 0.0, -0.06)

# ---------------------------------------------------------------- the box bay
# Dark stained boards round three windows high in the front gable, over the entry side,
# under a tile hood; it stands clear of the office's roof below.
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
    front.quad("FrontWall__Curtain", wu0, gy0, wu1, gy1, -BAY_DEPTH + 0.03)
    slats = max(2, round((gy1 - gy0) / LOUVRE))
    for j in range(1, slats):
        y = gy0 + (gy1 - gy0) * j / slats
        front.quad("FrontWall__Glass", wu0 + FRAME, y - 0.004, wu1 - FRAME, y + 0.004, -BAY_DEPTH + 0.025)
    for a, c, b, d in ((wu0, gy0, wu1, gy0 + FRAME), (wu0, gy1 - FRAME, wu1, gy1), (wu0, gy0, wu0 + FRAME, gy1), (wu1 - FRAME, gy0, wu1, gy1)):
        front.block("FrontWall__Trim", a, c, b, d, -BAY_DEPTH + 0.03, -BAY_DEPTH + 0.005)
    if k:
        front.block("FrontWall__Wood", wu0 - 0.018, gy0, wu0 + 0.018, gy1, -BAY_DEPTH + 0.02, -BAY_DEPTH - 0.008)
hood = HALF_D + BAY_DEPTH + 0.08
tiled("FrontWall", Slope([(BAY[0] - 0.07, BAY_HI + 0.02, hood), (BAY[1] + 0.07, BAY_HI + 0.02, hood), (BAY[1] + 0.07, BAY_HI + 0.02 + 0.42 * (hood - HALF_D), HALF_D), (BAY[0] - 0.07, BAY_HI + 0.02 + 0.42 * (hood - HALF_D), HALF_D)]))

# ---------------------------------------------------------------- the roof
# The main clay hip, its street slope behind the front gable over the entry side. The
# gable's left slope is the left hip end's plane carried forward, so the flank reads as one
# roof; its ridge runs back until it meets the main roof's street slope.
GABLE = (-ALONG_EAVE[0], -0.25)  # the front gable's eaves
GABLE_EAVE = ALONG_EAVE_Y
GABLE_PITCH = (RIDGE_Y - ALONG_EAVE_Y) / (ALONG_EAVE[0] - ALONG_RIDGE_HALF)
GABLE_APEX = GABLE_EAVE + GABLE_PITCH * (GABLE[1] - GABLE[0]) / 2
# where its ridge meets the main street slope (its slopes run on under it a little)
GABLE_DEPTH = ALONG_EAVE[1] - (GABLE_APEX - ALONG_EAVE_Y) / ALONG_PITCH - 0.1
hip = along_hip()
tiled("FrontRoof", Slope(clip(hip["front"], 0, GABLE[1] - 0.12, True)))
tiled("FrontRoof", Slope(clip(clip(hip["front"], 0, GABLE[1] - 0.12, False), 2, HALF_D - 0.05, False)))
tiled("BackRoof", Slope(hip["back"]))
tiled("RightRoof", Slope(hip["right"]))
tiled("LeftRoof", Slope(clip(hip["left"], 2, GABLE_DEPTH, False)))
ridge_cap("FrontRoof", (-ALONG_RIDGE_HALF, RIDGE_Y, 0.0), (ALONG_RIDGE_HALF, RIDGE_Y, 0.0))
for cx, cz in ((1, 1), (1, -1), (-1, -1)):
    ridge_cap("RightRoof" if cx > 0 else "LeftRoof", (cx * ALONG_EAVE[0], ALONG_EAVE_Y, cz * ALONG_EAVE[1]), (cx * ALONG_RIDGE_HALF, RIDGE_Y, 0.0))

# The front gable: its stucco triangle on the front wall over the bay, its two clay slopes
# from the rake back into the main roof, a clay vent in its peak.
peak = gable_face("FrontWall", HALF_D, -HALF_W, GABLE[1] - OVERHANG, WALL_TOP, GABLE[0], GABLE[1], GABLE_EAVE, GABLE_PITCH, under=DECK * 1.1)
gable_roof("FrontRoof", GABLE[0], GABLE[1], GABLE_DEPTH, EAVE_Z, GABLE_EAVE, GABLE_PITCH)
clay_vent("FrontWall", (GABLE[0] + GABLE[1]) / 2, peak - 0.18, HALF_D)

# The array: four panels on the street slope right of the gable.
solar_array("FrontRoof", Slope(hip["front"]), 2, 2, up=0.3, across=0.74)

# ---------------------------------------------------------------- the entry
office_entry(office_roof="shed", finish="Brick", window_style="shutters", cover="gable", supports="posts")

# ---------------------------------------------------------------- the flanks and back
plain_wall("back", PLAN, half_width={-2.0: 0.4, 0.0: 0.4, 2.0: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

export(PLAN, frame="along")
