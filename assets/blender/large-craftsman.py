# The Senda large Craftsman house, modelled after Rumbo north lots 3, 7 and 11, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/large-craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/large-craftsman.glb and
#   crates/core/data/houses/large-craftsman.plan.json)
#
# Drawn in the street frame on the 6.2 x 8 m lot the kit measures, its garage on the right;
# the game builds it with its door in the eave wall (its hip's ridge along the street) and
# fits it to every lot of its styles. A broad two-storey stucco block under a low flat-tile
# hip; at the front left the tract's entry: a brick office under a flat-tile front gable
# with one headed window to the street and, along its outer side, steps and a passage to the
# front door under a shed on a tapered Craftsman post over a stone pier; the garage on the
# right with a one-piece board door hung with strap hinges; over it a balcony between
# stucco cheeks with a dark iron railing and French doors behind; across the upper floor a
# wide window over the entry and a shuttered window over the office. The paints are the
# style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded large Craftsman front (the window and door in the entry's half: the
# passage's front door stands for the window, the office's back for the door; the
# garage's leaves, the window over the door and the two over the garage, here the
# balcony's doors) and the wide window over the passage; two windows a floor down each flank and
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
WIDE_UPPER = (entry_window_u - 0.42, entry_window_u + 0.42)  # the wide window over the entry
MIDDLE = (door_u - 0.27, door_u + 0.27)  # the shuttered window over the office
BAL_FLOOR_TOP = 1.6  # the balcony's floor, clear of the garage door's head trim
BALCONY_DOOR = (garage_u - 0.78, BAL_FLOOR_TOP, garage_u + 0.78, 2.86)  # French doors over 1.0 and 1.8
holes = [
    GARAGE,
    door_hole(door_u),
    (MIDDLE[0], UPPER_GLASS[0], MIDDLE[1], UPPER_GLASS[1]),
    (WIDE_UPPER[0], UPPER_GLASS[0], WIDE_UPPER[1], UPPER_GLASS[1]),
    BALCONY_DOOR,
] + entry_holes()
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])

window("FrontWall", front, fx(WIDE_UPPER[0]), UPPER_GLASS[0], fx(WIDE_UPPER[1]), UPPER_GLASS[1])
window("FrontWall", front, fx(MIDDLE[0]), UPPER_GLASS[0], fx(MIDDLE[1]), UPPER_GLASS[1], mullion=False)
board_shutters(front, fx(MIDDLE[0]), fx(MIDDLE[1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02)

# The door from the office's back into the house.
front_door(front, door_u)

# The garage's one-piece door: boards, four sections, strap hinges, under a dark header.
sectional_garage_door(front, garage_u, decor="straps", boards=True, header=True)
lantern(front, GARAGE[2] + 0.17)

# ---------------------------------------------------------------- the balcony
BAL = (BALCONY_DOOR[0] - 0.2, BALCONY_DOOR[2] + 0.2)  # cheeks' outer edges, round the doors
BAL_OUT = 0.32
BAL_FLOOR = (BAL_FLOOR_TOP - 0.1, BAL_FLOOR_TOP)
BAL_HEAD = (2.9, 3.0)
CHEEK = (0.14, 0.16)  # left and right cheek widths
zb = HALF_D + BAL_OUT
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[0], HALF_D), (BAL[1], BAL_FLOOR[1], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[1], HALF_D), (BAL[0] + CHEEK[0], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[1] - CHEEK[1], BAL_FLOOR[1], HALF_D), (BAL[1], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_HEAD[0], HALF_D), (BAL[1], BAL_HEAD[1], zb), skip=("-z",))
# a dark stained frame inside the opening and a trim band under the floor
box("FrontWall__Wood", (BAL[0] + CHEEK[0], BAL_HEAD[0] - 0.06, zb - 0.08), (BAL[1] - CHEEK[1], BAL_HEAD[0], zb), skip=("+y", "-z"))
box("FrontWall__Trim", (BAL[0] - 0.02, BAL_FLOOR[0] - 0.04, zb - 0.02), (BAL[1], BAL_FLOOR[0] + 0.02, zb + 0.02), skip=("-z", "+y"))
# the iron railing, a real metre high
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
front.quad("FrontWall__Curtain", b0, by0, b1, by1, 0.12)
bm = (b0 + b1) / 2
t = 0.04
for a, c, b, d in ((b0, by0, b1, by0 + 0.09), (b0, by1 - t, b1, by1), (b0, by0, b0 + t, by1), (b1 - t, by0, b1, by1), (bm - t, by0, bm + t, by1)):
    front.block("FrontWall__Wood", a, c, b, d, 0.12, 0.09)
for u in ((b0 + bm) / 2, (bm + b1) / 2):
    front.block("FrontWall__Wood", u - 0.01, by0 + 0.09, u + 0.01, by1 - t, 0.12, 0.105)
front.block("FrontWall__Wood", b0, 2.16, b1, 2.18, 0.12, 0.105)

# ---------------------------------------------------------------- the entry
office_entry(office_roof="gable", flat=True, finish="Brick", window_style="header", cover="shed", supports="tapered")

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
plain_wall("back", PLAN, half_width={-2.0: 0.4, 0.0: 0.4, 2.0: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

# The array: three blocks on the back slope.
back_slope = Slope(along_hip()["back"])
for across in (0.3, 0.5, 0.7):
    solar_array("BackRoof", back_slope, 2, 2, up=0.2, across=across)

export(PLAN, frame="along")
