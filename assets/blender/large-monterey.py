# The Senda large Monterey house, modelled after Rumbo north lot 6, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/large-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/large-monterey.glb and
#   crates/core/data/houses/large-monterey.plan.json)
#
# Drawn in the street frame on a 5 x 7 m lot, its garage on the right; the game builds it
# with its door in the eave wall (its clay hip's ridge along the street). A honey stucco
# two-storey block under clay barrel tile with a two-storey front gable over its entry side,
# a dark stained box bay of three windows high in it; before that side a broad one-storey
# dark brick wing, front-gabled, a sage-shuttered window in its gable face and its roof's
# long right slope running on over the recessed entry to a stucco pier; the garage's
# stained board door on the right and a shuttered pair over it on a bracketed shelf; a
# block of panels on the street slope. The small Monterey (Rumbo north lot 4) is small-monterey.py.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded front (the street's layout reads its door and garage) and the pair over the
# garage; two windows down each flank and a pair across the back, lit upstairs.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [0.68, 1.18], "garage_bay": 0.62},
        "back": {"ground": [[-1.2, "Window"], [1.2, "Window"]], "upper": [-1.2, 1.2], "garage_bay": 0.0},
        "right": {"ground": [[-1.5, "Window"], [1.3, "Window"]], "upper": [-1.5, 1.3], "garage_bay": 0.0},
        "left": {"ground": [[-1.4, "Window"], [1.6, "Window"]], "upper": [-1.4, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][0][0]
garage_u, garage_bay = _front["ground"][1][0], _front["garage_bay"]
upper = _front["upper"]

# ---------------------------------------------------------------- the front wall
garage = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
DOOR = (door_u - 0.3, 0.12, door_u + 0.3, 1.45)
PAIR = (upper[0] - 0.3, upper[1] + 0.3)  # the shuttered pair over the garage
holes = [
    (fx(garage[0]), garage[1], fx(garage[2]), garage[3]),
    (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3]),
    (fx(PAIR[0]), UPPER_GLASS[0], fx(PAIR[1]), UPPER_GLASS[1]),
]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
sectional_garage_door(front, garage_u, garage_bay)
# the lantern beside the garage
front.block("FrontWall__Iron", fx(garage[2]) + 0.12, 1.12, fx(garage[2]) + 0.22, 1.36, 0.0, -0.1)
front.block("FrontWall__Curtain", fx(garage[2]) + 0.135, 1.16, fx(garage[2]) + 0.205, 1.3, -0.1, -0.102)

# The front door at the back of the covered entry.
front.quad("FrontWall__Accent", fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3], 0.1)
front.block("FrontWall__Trim", fx(DOOR[0]) - 0.05, DOOR[3], fx(DOOR[2]) + 0.05, DOOR[3] + 0.05, 0.0, -0.03)
front.block("FrontWall__Iron", fx(DOOR[2]) - 0.1, 0.66, fx(DOOR[2]) - 0.07, 0.8, 0.1, 0.07)

# The pair over the garage, sage board shutters either side, on a shelf on two brackets.
window("FrontWall", front, fx(PAIR[0]), UPPER_GLASS[0], fx(PAIR[1]), UPPER_GLASS[1], sill=False)
board_shutters(front, fx(PAIR[0]), fx(PAIR[1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02, width=0.24)
front.block("FrontWall__Wood", fx(PAIR[0]) - 0.08, UPPER_GLASS[0] - 0.08, fx(PAIR[1]) + 0.08, UPPER_GLASS[0] - 0.02, 0.0, -0.13)
for u in (fx(PAIR[0]) + 0.16, fx(PAIR[1]) - 0.16):
    front.block("FrontWall__Wood", u - 0.04, UPPER_GLASS[0] - 0.26, u + 0.04, UPPER_GLASS[0] - 0.08, 0.0, -0.1)

# ---------------------------------------------------------------- the box bay
# Dark stained boards round three windows high in the front gable, over the entry side,
# under a tile hood; it stands clear of the wing's roof below.
BAY = (-2.08, -0.72)
BAY_OUT, BAY_LO, BAY_HI = 0.3, 2.2, 2.98
b0, b1 = fx(BAY[0]), fx(BAY[1])
for a, c, b, d in ((b0, BAY_LO, b1, BAY_LO + 0.1), (b0, BAY_HI - 0.06, b1, BAY_HI), (b0, BAY_LO, b0 + 0.07, BAY_HI), (b1 - 0.07, BAY_LO, b1, BAY_HI)):
    front.block("FrontWall__Wood", a, c, b, d, 0.0, -BAY_OUT)
for su, sign in ((b0, -1), (b1, 1)):
    face("FrontWall__Wood", [front.p(su, BAY_LO, 0), front.p(su, BAY_HI, 0), front.p(su, BAY_HI, -BAY_OUT), front.p(su, BAY_LO, -BAY_OUT)], front.along * sign)
lights = 3
pane = (b1 - b0 - 0.14) / lights
for k in range(lights):
    wu0, wu1 = b0 + 0.07 + pane * k, b0 + 0.07 + pane * (k + 1)
    front.quad("FrontWall__Curtain", wu0, BAY_LO + 0.1, wu1, BAY_HI - 0.06, -BAY_OUT + 0.04)
    for a, c, b, d in ((wu0, BAY_LO + 0.1, wu1, BAY_LO + 0.14), (wu0, BAY_HI - 0.1, wu1, BAY_HI - 0.06), (wu0, BAY_LO + 0.1, wu0 + 0.035, BAY_HI - 0.06), (wu1 - 0.035, BAY_LO + 0.1, wu1, BAY_HI - 0.06)):
        front.block("FrontWall__Trim", a, c, b, d, -BAY_OUT + 0.04, -BAY_OUT + 0.01)
    if k:
        front.block("FrontWall__Wood", wu0 - 0.025, BAY_LO + 0.1, wu0 + 0.025, BAY_HI - 0.06, -BAY_OUT + 0.02, -BAY_OUT - 0.01)
hood = HALF_D + BAY_OUT + 0.12
tiled("FrontWall", Slope([(BAY[0] - 0.1, BAY_HI + 0.02, hood), (BAY[1] + 0.1, BAY_HI + 0.02, hood), (BAY[1] + 0.1, BAY_HI + 0.2, HALF_D), (BAY[0] - 0.1, BAY_HI + 0.2, HALF_D)]))

# ---------------------------------------------------------------- the roof
# The main clay hip, its street slope behind the front gable over the entry side. The
# gable's left slope is the left hip end's plane carried forward, so the flank reads as one
# roof; its ridge runs back until it meets the main roof's street slope.
GABLE = (-ALONG_EAVE[0], -0.1)  # the front gable's eaves
GABLE_EAVE = ALONG_EAVE_Y
GABLE_PITCH = (RIDGE_Y - ALONG_EAVE_Y) / (ALONG_EAVE[0] - ALONG_RIDGE_HALF)
GABLE_DEPTH = 0.1  # where its slopes run into the main roof
hip = along_hip()
tiled("FrontRoof", Slope(clip(hip["front"], 0, GABLE[1] - 0.15, True)))
tiled("FrontRoof", Slope(clip(clip(hip["front"], 0, GABLE[1] - 0.15, False), 2, HALF_D - 0.05, False)))
tiled("BackRoof", Slope(hip["back"]))
tiled("RightRoof", Slope(hip["right"]))
tiled("LeftRoof", Slope(clip(hip["left"], 2, GABLE_DEPTH, False)))
ridge_cap("FrontRoof", (-ALONG_RIDGE_HALF, RIDGE_Y, 0.0), (ALONG_RIDGE_HALF, RIDGE_Y, 0.0))
for cx, cz in ((1, 1), (1, -1), (-1, -1)):
    ridge_cap("RightRoof" if cx > 0 else "LeftRoof", (cx * ALONG_EAVE[0], ALONG_EAVE_Y, cz * ALONG_EAVE[1]), (cx * ALONG_RIDGE_HALF, RIDGE_Y, 0.0))

# The front gable: its stucco triangle on the front wall over the bay, its two clay slopes
# from the rake back into the main roof.
gable_face("FrontWall", HALF_D, -HALF_W, -0.3, WALL_TOP, GABLE[0], GABLE[1], GABLE_EAVE, GABLE_PITCH)
gable_roof("FrontRoof", GABLE[0], GABLE[1], GABLE_DEPTH, HALF_D + 0.36, GABLE_EAVE, GABLE_PITCH)

# The array: four panels on the street slope right of the gable.
street = Slope(hip["front"])
for pu, pv in ((3.3, 1.75), (3.92, 1.75), (3.3, 2.37), (3.92, 2.37)):
    face("FrontRoof__Solar", [street.at(pu, pv, 0.08), street.at(pu + 0.58, pv, 0.08), street.at(pu + 0.58, pv + 0.56, 0.08), street.at(pu, pv + 0.56, 0.08)], street.N)

# ---------------------------------------------------------------- the brick wing
# One storey of dark brick before the entry side, gabled to the street; its roof's right
# slope runs long and low over the recessed entry beside it to a stucco pier.
WING = (-3.1, -1.6, HALF_D, 5.1)  # brick x0, x1, back and front faces
WING_TOP = 1.7
ENTRY_X = -0.42  # the pier's outer face
ENTRY_HEAD = 1.42
APEX_X, APEX_Y = (WING[0] + WING[1]) / 2, 2.2
EAVE_LEFT, EAVE_RIGHT = WING[0] - 0.2, ENTRY_X + 0.12
PITCH_LEFT, PITCH_RIGHT = 0.45, 0.25
ROOF_BACK, ROOF_FRONT = HALF_D - 0.2, WING[3] + 0.25


def wing_roof_y(x):
    """The wing roof's top at x (its ridge along z)."""
    return APEX_Y - (PITCH_LEFT if x < APEX_X else PITCH_RIGHT) * abs(x - APEX_X)


def brick_wall(frame, u0, u1, y0, y1, openings):
    """A brick slab: the kit's stucco wall, its face and reveals in brick."""
    wall("Wing", frame, u0, u1, y0, y1, openings)
    for surface, target in (("Siding", "FrontWall__Brick"), ("Lining", "FrontWall__Lining")):
        vertices, faces = geometry.pop(f"Wing__{surface}")
        into = geometry[target]
        base = len(into[0])
        into[0].extend(vertices)
        into[1].extend([index + base for index in f] for f in faces)


wl, wd = WING[1] - WING[0], WING[3] - WING[2]
wing_front = Frame((WING[0], 0, WING[3]), (1, 0, 0), (0, 0, 1))
wing_left = Frame((WING[0], 0, HALF_D), (0, 0, 1), (-1, 0, 0))
wing_right = Frame((WING[1], 0, WING[3]), (0, 0, -1), (1, 0, 0))
wing_back = Frame((-HALF_W, 0, HALF_D), (-1, 0, 0), (0, 0, -1))  # beyond the house's flank
WING_WINDOW = (wl / 2 - 0.3, 0.55, wl / 2 + 0.3, 1.38)
brick_wall(wing_front, 0, wl, 0, WING_TOP, [WING_WINDOW])
window("FrontWall", wing_front, *WING_WINDOW)
board_shutters(wing_front, WING_WINDOW[0], WING_WINDOW[2], WING_WINDOW[1] - 0.03, WING_WINDOW[3] + 0.03, width=0.22)
brick_wall(wing_left, 0, wd, 0, WING_TOP, [])
brick_wall(wing_right, 0, wd, 0, WING_TOP, [])
brick_wall(wing_back, 0, -HALF_W - WING[0], 0, WING_TOP, [])

# The gable face over the brick, a vent in it; brick over the entry's opening, on the pier.
under = 0.1
gable = [(WING[0], WING_TOP), (WING[1], WING_TOP), (WING[1], wing_roof_y(WING[1]) - under), (APEX_X, APEX_Y - under), (WING[0], max(WING_TOP, wing_roof_y(WING[0]) - under))]
face("FrontWall__Brick", [(x, y, WING[3]) for x, y in gable], (0, 0, 1))
face("FrontWall__Lining", [(x, y, WING[3] - WALL) for x, y in gable], (0, 0, -1))
back = [(WING[0], WING_TOP), (-HALF_W, WING_TOP), (-HALF_W, wing_roof_y(-HALF_W) - under), (WING[0], max(WING_TOP, wing_roof_y(WING[0]) - under))]
face("FrontWall__Brick", [(x, y, HALF_D) for x, y in back], (0, 0, -1))
box("FrontWall__Dark", (APEX_X - 0.07, 1.84, WING[3]), (APEX_X + 0.07, 1.94, WING[3] + 0.02))
box("FrontWall__Brick", (WING[1], ENTRY_HEAD, WING[3] - WALL), (ENTRY_X, WING_TOP, WING[3]))
face("FrontWall__Brick", [(WING[1], WING_TOP, WING[3]), (ENTRY_X, WING_TOP, WING[3]), (ENTRY_X, wing_roof_y(ENTRY_X) - under, WING[3]), (WING[1], wing_roof_y(WING[1]) - under, WING[3])], (0, 0, 1))
box("FrontWall__Siding", (ENTRY_X - 0.22, 0.1, WING[3] - 0.22), (ENTRY_X, ENTRY_HEAD, WING[3]))
box("FrontWall__Trim", (ENTRY_X - 0.25, 0.1, WING[3] - 0.25), (ENTRY_X + 0.03, 0.22, WING[3] + 0.03))
# the entry's floor and step
box("FrontWall__Concrete", (WING[1], 0.0, HALF_D), (EAVE_RIGHT, 0.1, WING[3] + 0.05))
box("FrontWall__Concrete", (WING[1] + 0.1, 0.0, WING[3] + 0.05), (ENTRY_X, 0.05, WING[3] + 0.35))

# Its two clay slopes and ridge, from inside the house's wall to past the gable.
tiled("FrontWall", Slope([(EAVE_LEFT, wing_roof_y(EAVE_LEFT), ROOF_BACK), (EAVE_LEFT, wing_roof_y(EAVE_LEFT), ROOF_FRONT), (APEX_X, APEX_Y, ROOF_FRONT), (APEX_X, APEX_Y, ROOF_BACK)]))
tiled("FrontWall", Slope([(EAVE_RIGHT, wing_roof_y(EAVE_RIGHT), ROOF_FRONT), (EAVE_RIGHT, wing_roof_y(EAVE_RIGHT), ROOF_BACK), (APEX_X, APEX_Y, ROOF_BACK), (APEX_X, APEX_Y, ROOF_FRONT)]))
ridge_cap("FrontWall", (APEX_X, APEX_Y, ROOF_FRONT), (APEX_X, APEX_Y, ROOF_BACK))

# ---------------------------------------------------------------- the flanks and back
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

export(PLAN, frame="along")
