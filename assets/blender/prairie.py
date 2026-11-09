# The Senda Prairie house (styles 6 and 8), modelled after Rumbo north lot 2, for Sloppy Tanks in
# Blender (headless):
#   blender --background --python assets/blender/prairie.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:prairie writes public/models/houses/prairie.glb and
#   crates/core/data/houses/prairie.plan.json)
#
# Drawn in the street frame on a 5 x 7 m lot, its garage on the right; the game builds it
# with its door in the eave wall (its hip's ridge along the street) and fits it to every lot
# of its styles. A broad two-storey stucco block under a low flat-tile hip, three openings
# across its upper floor (a shuttered pair over the door, a small single and a bracketed
# pair over the garage), an open porch beside the garage under a flat-tile shed roof on
# chunky dark posts over stacked-stone piers, its wall behind clad in board and batten, and
# the garage's one-piece sectional door. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Prairie front (the street's layout reads its door and garage) and its five
# upper windows; two windows down each flank and a pair across the back, lit upstairs.
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

# ---------------------------------------------------------------- the front wall
garage = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
DOOR = (door_u - 0.3, 0.12, door_u + 0.3, 1.45)
UPPER = [(-1.8, -0.66), (0.14, 0.48), (0.98, 2.12)]  # the pair, the single, the pair
holes = [(fx(garage[0]), garage[1], fx(garage[2]), garage[3]), (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3])]
holes += [(fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1]) for a, b in UPPER]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
sectional_garage_door(front, garage_u, garage_bay)
front.quad("FrontWall__Accent", fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3], 0.1)
front.block("FrontWall__Trim", fx(DOOR[0]) - 0.05, DOOR[3], fx(DOOR[2]) + 0.05, DOOR[3] + 0.05, 0.0, -0.03)
for a, b in UPPER:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
board_shutters(front, fx(UPPER[0][0]), fx(UPPER[0][1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02)
board_shutters(front, fx(UPPER[2][0]), fx(UPPER[2][1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02)
for u in (fx(UPPER[2][0]) + 0.15, fx(UPPER[2][1]) - 0.15):
    front.block("FrontWall__Wood", u - 0.04, UPPER_GLASS[0] - 0.24, u + 0.04, UPPER_GLASS[0] - 0.04, 0.0, -0.12)
front.block("FrontWall__Wood", fx(UPPER[2][0]) - 0.06, UPPER_GLASS[0] - 0.06, fx(UPPER[2][1]) + 0.06, UPPER_GLASS[0] - 0.02, 0.0, -0.14)

# The porch's back wall clad in board and batten, beside the door.
CLAD = (-HALF_W + 0.05, garage[0] - 0.12)
front.block("FrontWall__Cladding", fx(CLAD[0]), 0.14, fx(DOOR[0]) - 0.06, 1.66, -0.0, -0.015)
front.block("FrontWall__Cladding", fx(DOOR[2]) + 0.06, 0.14, fx(CLAD[1]), 1.66, -0.0, -0.015)
u = fx(CLAD[0]) + 0.08
while u < fx(CLAD[1]):
    if not (fx(DOOR[0]) - 0.08 < u < fx(DOOR[2]) + 0.08):
        front.block("FrontWall__Cladding", u - 0.015, 0.14, u + 0.015, 1.66, -0.015, -0.04)
    u += 0.2
# A window beside the door under the porch.
PORCH_WINDOW = (-2.2, -1.72)
front.block("FrontWall__Trim", fx(PORCH_WINDOW[0]) - 0.04, 0.6, fx(PORCH_WINDOW[1]) + 0.04, 1.38, -0.015, -0.05)
front.block("FrontWall__Curtain", fx(PORCH_WINDOW[0]), 0.64, fx(PORCH_WINDOW[1]), 1.34, -0.05, -0.055)

# ---------------------------------------------------------------- the porch
PORCH = (-2.75, garage[0] - 0.15, HALF_D, HALF_D + 1.35)
PORCH_TOP = 1.66
box("FrontWall__Concrete", (PORCH[0] + 0.1, 0.0, PORCH[2]), (PORCH[1], 0.1, PORCH[3]))
for x in (PORCH[0] + 0.3, (PORCH[0] + PORCH[1]) / 2 + 0.15, PORCH[1] - 0.25):
    porch_post(x, PORCH[3] - 0.2, PORCH_TOP)
box("FrontWall__Wood", (PORCH[0] + 0.15, PORCH_TOP - 0.14, PORCH[3] - 0.3), (PORCH[1] - 0.1, PORCH_TOP, PORCH[3] - 0.1))
tiled("FrontWall", Slope([(PORCH[0], PORCH_TOP, PORCH[3] + 0.15), (PORCH[1], PORCH_TOP, PORCH[3] + 0.15), (PORCH[1], PORCH_TOP + 0.42, HALF_D), (PORCH[0], PORCH_TOP + 0.42, HALF_D)]), flat=True)

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

# The array: a small block on the street slope.
solar_array("FrontRoof", Slope(along_hip()["front"]), 2, 2)

export(PLAN, frame="along")
