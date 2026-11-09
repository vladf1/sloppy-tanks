# The Senda medium Spanish, for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/medium-spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/medium-spanish.glb and
#   crates/core/data/houses/medium-spanish.plan.json)
#
# The medium plan (the Prairie's, after Rumbo north lot 2) in the tract's Spanish dress; no house of
# the street is this one, so it borrows Rumbo north lots 5 and 1's vocabulary. Drawn in the street
# frame on a 5 x 7 m lot, its garage on the right; the game builds it with its door in the
# eave wall (its hip's ridge along the street). A broad two-storey warm stucco block under a
# clay barrel-tile hip; the porch beside the garage is a stucco arcade, one broad arch on
# stucco piers and an arch at either end, under a clay shed roof, the door arched behind
# it; twin arched windows over the porch, a small single and a shuttered pair over the
# garage across the upper floor, clay pipe vents, and the garage's one-piece board door
# under a clay pent. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The medium plan's openings, as the Prairie has them (the street's layout reads its door
# and garage): the door and garage, five upper windows (the twin arched windows over the
# porch, the single and the pair over the garage); two windows down each flank and a pair
# across the back, lit upstairs.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [-1.48, -0.98, 0.31, 1.3, 1.8], "garage_bay": 0.62},
        "back": {"ground": [[-1.2, "Window"], [1.2, "Window"]], "upper": [-1.2, 1.2], "garage_bay": 0.0},
        "right": {"ground": [[-1.5, "Window"], [1.3, "Window"]], "upper": [-1.5, 1.3], "garage_bay": 0.0},
        "left": {"ground": [[-1.4, "Window"], [1.6, "Window"]], "upper": [-1.4, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][0][0]
garage_u, garage_bay = _front["ground"][1][0], _front["garage_bay"]


def vent(part, frame, u, y, spacing=0.07):
    """Three clay pipe ends set in a wall, in a triangle (`clay_vent` on any wall)."""
    for du, dy in ((-spacing / 2, 0), (spacing / 2, 0), (0, spacing * 0.85)):
        frame.block(f"{part}__Terracotta", u + du - 0.022, y + dy - 0.022, u + du + 0.022, y + dy + 0.022, 0.02, -0.03)
        frame.quad(f"{part}__Dark", u + du - 0.012, y + dy - 0.012, u + du + 0.012, y + dy + 0.012, -0.032)


# ---------------------------------------------------------------- the front wall
garage = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
DOOR = (door_u - 0.3, 0.12, door_u + 0.3, 1.6)
DOOR_SPRING = 1.42
TWIN = [(-1.69, -1.27), (-1.19, -0.77)]  # the arched pair over the porch
TWIN_TOP, TWIN_RISE = UPPER_WINDOW[1], 0.18
SINGLE, PAIR = (0.14, 0.48), (0.98, 2.12)
holes = [(fx(garage[0]), garage[1], fx(garage[2]), garage[3]), (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3])]
holes += [(fx(a), UPPER_GLASS[0], fx(b), TWIN_TOP) for a, b in TWIN]
holes += [(fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1]) for a, b in (SINGLE, PAIR)]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)

# The door, arched in its hole, at the back of the porch.
front.quad("FrontWall__Accent", fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3], 0.1)
arch_head("FrontWall", front, fx(DOOR[0]), fx(DOOR[2]), DOOR_SPRING, DOOR[3] - DOOR_SPRING, depth=WALL)

for a, b in TWIN:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), TWIN_TOP, mullion=False)
    arch_head("FrontWall", front, fx(a), fx(b), TWIN_TOP - TWIN_RISE, TWIN_RISE, depth=WALL)
window("FrontWall", front, fx(SINGLE[0]), UPPER_GLASS[0], fx(SINGLE[1]), UPPER_GLASS[1], mullion=False)
window("FrontWall", front, fx(PAIR[0]), UPPER_GLASS[0], fx(PAIR[1]), UPPER_GLASS[1])
board_shutters(front, fx(PAIR[0]), fx(PAIR[1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02, width=0.24)
vent("FrontWall", front, fx(-0.33), 2.5)

# The garage's one-piece door: stained vertical boards, set back, under a clay pent.
g0, g1 = fx(garage[0]), fx(garage[2])
front.quad("FrontWall__GarageDoor", g0, garage[1], g1, garage[3], 0.12)
u = g0 + 0.1
while u < g1 - 0.05:
    front.block("FrontWall__GarageDoor", u - 0.012, garage[1] + 0.04, u + 0.012, garage[3] - 0.04, 0.12, 0.105)
    u += 0.14
front.block("FrontWall__Trim", g0 - 0.05, garage[3], g1 + 0.05, garage[3] + 0.05, 0.0, -0.03)
PENT = (garage[0] - 0.14, garage[2] + 0.3)
tiled("FrontWall", Slope([(PENT[0], 1.62, HALF_D + 0.5), (PENT[1], 1.62, HALF_D + 0.5), (PENT[1], 1.84, HALF_D), (PENT[0], 1.84, HALF_D)]))
# A wrought-iron lantern beside the garage.
front.block("FrontWall__Iron", fx(2.2), 1.02, fx(2.34), 1.3, 0.0, -0.12)
front.block("FrontWall__Glass", fx(2.22), 1.06, fx(2.32), 1.24, -0.02, -0.125)

# ---------------------------------------------------------------- the arcade porch
# A stucco arcade beside the garage, flush with the house's left corner: one broad arch on
# stucco piers before the door and a smaller arch at either end, under a clay shed roof.
# Its walls are built as their own part and then poured into the front wall's stucco: the
# arcade stands outdoors, so the faces `wall` lines as indoors are stucco too.
ARCADE = "Arcade"


def pour(part, key):
    """Move every surface of the scratch `part` into the object `key`."""
    for name in [k for k in geometry if k.startswith(f"{part}__")]:
        vertices, faces = geometry.pop(name)
        target = geometry[key]
        base = len(target[0])
        target[0].extend(vertices)
        target[1].extend([index + base for index in f] for f in faces)


PORCH = (-HALF_W, garage[0] - 0.15, HALF_D, HALF_D + 1.35)
PORCH_TOP = 1.75
length, depth = PORCH[1] - PORCH[0], PORCH[3] - PORCH[2]
arcade = Frame((PORCH[0], 0, PORCH[3]), (1, 0, 0), (0, 0, 1))
arcade_left = Frame((PORCH[0], 0, PORCH[2]), (0, 0, 1), (-1, 0, 0))
arcade_right = Frame((PORCH[1], 0, PORCH[3]), (0, 0, -1), (1, 0, 0))
PIER = 0.36
BROAD = (PIER, length - PIER, 1.2, 0.42)  # u0, u1, spring, rise
wall(ARCADE, arcade, 0, length, 0, PORCH_TOP, [(BROAD[0], 0.0, BROAD[1], BROAD[2] + BROAD[3])])
arch_head(ARCADE, arcade, BROAD[0], BROAD[1], BROAD[2], BROAD[3], depth=WALL, segments=8)
END = (0.3, depth - 0.34, 1.16, 0.26)
for side in (arcade_left, arcade_right):
    u0, u1 = (END[0], END[1]) if side is arcade_left else (depth - END[1], depth - END[0])
    wall(ARCADE, side, 0, depth, 0, PORCH_TOP, [(u0, 0.0, u1, END[2] + END[3])])
    arch_head(ARCADE, side, u0, u1, END[2], END[3], depth=WALL)
# Its floor and a step down to the walk under the broad arch.
box("FrontWall__Concrete", (PORCH[0] + WALL, 0.0, PORCH[2]), (PORCH[1] - WALL, 0.1, PORCH[3]))
box("FrontWall__Concrete", (PORCH[0] + BROAD[0], 0.0, PORCH[3]), (PORCH[0] + BROAD[1], 0.05, PORCH[3] + 0.3))

# Its clay shed roof, from the arcade's eave up to the house under the upper windows, and
# the stucco closing each end between the arcade's top and the roof.
EAVE_OUT, OVERHANG, RISE = 0.28, 0.12, 0.32
eave_y = PORCH_TOP + 0.1
roof_z = PORCH[3] + EAVE_OUT
tiled("FrontWall", Slope([(PORCH[0] - OVERHANG, eave_y, roof_z), (PORCH[1] + OVERHANG, eave_y, roof_z), (PORCH[1] + OVERHANG, eave_y + RISE, HALF_D), (PORCH[0] - OVERHANG, eave_y + RISE, HALF_D)]))


def under_roof(z):
    return eave_y + RISE * (roof_z - z) / (roof_z - HALF_D) - DECK


for x, out in ((PORCH[0], -1), (PORCH[1], 1)):
    for inset in (0.0, WALL):
        xi = x - out * inset
        face("FrontWall__Siding", [(xi, PORCH_TOP, PORCH[3]), (xi, PORCH_TOP, HALF_D), (xi, under_roof(HALF_D), HALF_D), (xi, under_roof(PORCH[3]), PORCH[3])], (out if inset == 0 else -out, 0, 0))
box("FrontWall__Siding", (PORCH[0], PORCH_TOP, PORCH[3] - WALL), (PORCH[1], under_roof(PORCH[3] - WALL), PORCH[3]))
pour(ARCADE, "FrontWall__Siding")

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=False)
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)
    vent(WALL_PARTS[side], WALL_FRAMES[side], 0.0 if side == "back" else -0.1, 2.62)

export(PLAN, frame="along")
