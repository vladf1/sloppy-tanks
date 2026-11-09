# The Senda Craftsman house whose hip runs along the street (styles 1 and 2), modelled
# after Rumbo north lot 9, for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:craftsman writes public/models/houses/craftsman.glb and
#   crates/core/data/houses/craftsman.plan.json)
#
# Drawn in the street frame on a 5 x 7 m lot, its garage on the right; the game builds it
# with its door in the eave wall and fits it to every lot of its styles. A two-storey stucco
# block under a low flat-tile hip, a dark stained box bay over the garage under a tile
# hood, a shuttered window over the entry, and before the entry a one-storey porch under a
# flat-tile hip of its own on dark posts over stacked-stone piers; the garage's one-piece
# sectional door in raised panels. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Craftsman front (door bay, garage leaves and the bay's pair over the garage)
# with a window over the entry; two windows down each flank and a pair at the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [-1.23, 0.68, 1.18], "garage_bay": 0.62},
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
UPPER_ENTRY = (door_u - 0.3, door_u + 0.3)
BAY = (_front["upper"][1] - 0.42, _front["upper"][2] + 0.42)
BAY_OPEN = (BAY[0] + 0.08, 1.86, BAY[1] - 0.08, 2.8)
holes = [
    (fx(garage[0]), garage[1], fx(garage[2]), garage[3]),
    (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3]),
    (fx(UPPER_ENTRY[0]), UPPER_GLASS[0], fx(UPPER_ENTRY[1]), UPPER_GLASS[1]),
    (fx(BAY_OPEN[0]), BAY_OPEN[1], fx(BAY_OPEN[2]), BAY_OPEN[3]),
    # a window beside the door, under the porch
    (fx(-2.2), 0.72, fx(-1.9), 1.5),
]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
sectional_garage_door(front, garage_u, garage_bay, panels=3)
front.quad("FrontWall__Accent", fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3], 0.1)
front.block("FrontWall__Trim", fx(DOOR[0]) - 0.05, DOOR[3], fx(DOOR[2]) + 0.05, DOOR[3] + 0.05, 0.0, -0.03)
window("FrontWall", front, fx(UPPER_ENTRY[0]), UPPER_GLASS[0], fx(UPPER_ENTRY[1]), UPPER_GLASS[1])
board_shutters(front, fx(UPPER_ENTRY[0]), fx(UPPER_ENTRY[1]), UPPER_GLASS[0] - 0.02, UPPER_GLASS[1] + 0.02, width=0.24)
window("FrontWall", front, fx(-2.2), 0.72, fx(-1.9), 1.5, mullion=False)
box_bay(front, BAY[0], BAY[1], flat=True)

# ---------------------------------------------------------------- the porch
PORCH = (-2.75, garage[0] - 0.12, HALF_D, HALF_D + 1.45)
PORCH_TOP = 1.62
box("FrontWall__Concrete", (PORCH[0] + 0.1, 0.0, PORCH[2]), (PORCH[1], 0.1, PORCH[3]))
for x in (PORCH[0] + 0.3, PORCH[1] - 0.25):
    porch_post(x, PORCH[3] - 0.2, PORCH_TOP)
box("FrontWall__Wood", (PORCH[0] + 0.15, PORCH_TOP - 0.14, PORCH[3] - 0.3), (PORCH[1] - 0.1, PORCH_TOP, PORCH[3] - 0.1))
porch_roof = hip_slopes(PORCH[0] - 0.1, PORCH[1] + 0.15, HALF_D - 0.6, PORCH[3] + 0.25, PORCH_TOP - 0.04, 0.42, along_x=True)
for name in ("front", "back", "left", "right"):
    polygon = clip_z(porch_roof[name], HALF_D)
    if len(polygon) >= 3:
        tiled("FrontWall", Slope(polygon), flat=True)
ridge_cap("FrontWall", *porch_roof["ridge"])

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

# The array: a block on the street slope.
solar_array("FrontRoof", Slope(along_hip()["front"]), 3, 2)

export(PLAN, frame="along")
