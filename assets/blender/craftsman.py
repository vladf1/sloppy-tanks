# The Senda Craftsman house whose hip runs along the street (styles 1 and 2), modelled
# after Rumbo north lot 9, for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:craftsman writes public/models/houses/craftsman.glb and
#   crates/core/data/houses/craftsman.plan.json)
#
# Drawn in the street frame on a 6.2 x 8 m lot (the tract's 11.7 x 15 m box at the level's
# scale), its garage on the right; the game builds it with its door in the eave wall and
# fits it to every lot of its styles. A two-storey stucco block under a low flat-tile hip,
# a dark stained box bay over the garage under a tile hood, a shuttered window over the
# entry, and before it a one-storey office under a flat-tile hip, the entry passage up its
# side under a shed on tapered posts over stacked-stone piers; the garage's one-piece
# roll-up door with a row of lites.
# The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Craftsman front (a window and the door in the entry's half, the garage's leaves
# and the bay's pair over the garage) with a window over the entry; two windows a floor
# down each flank and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8, -0.66], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.3, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.3, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.2, "Window"], [1.9, "Window"]], "upper": [-2.2, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-1.9, "Window"], [2.1, "Window"]], "upper": [-1.9, 2.1], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
UPPER_ENTRY = (door_u - 0.3, door_u + 0.3)
BAY = (garage_u - 0.85, garage_u + 0.85)
BAY_HOLE = (BAY[0] + 0.05, UPPER_GLASS[0] - 0.08, BAY[1] - 0.05, BAY_HI - 0.05)
# The passage's front door stands for the coded ground window (its hole spans -1.91); the
# coded door stands behind the office.
holes = [
    GARAGE,
    door_hole(door_u),
    (UPPER_ENTRY[0], UPPER_GLASS[0], UPPER_ENTRY[1], UPPER_GLASS[1]),
    BAY_HOLE,
] + entry_holes()
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor="lites", boards=False, header=True)
front_door(front, door_u, glass=True)
window("FrontWall", front, fx(UPPER_ENTRY[0]), UPPER_GLASS[0], fx(UPPER_ENTRY[1]), UPPER_GLASS[1], mullion=False)
board_shutters(front, fx(UPPER_ENTRY[0]), fx(UPPER_ENTRY[1]), UPPER_GLASS[0] - 0.01, UPPER_GLASS[1] + 0.01)
box_bay(front, BAY[0], BAY[1], flat=True)

# ---------------------------------------------------------------- the office and entry
# The office projecting before the left half under a flat-tile hip, the entry passage up
# its outer side under a shed on tapered posts over stone piers.
office_entry(office_roof="hip", flat=True, cover="shed", supports="tapered", window_style="shutters")

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
plain_wall("back", PLAN, half_width={-1.6: 0.4, 0.3: 0.4, 2.0: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

# The array: a block on the street slope.
solar_array("FrontRoof", Slope(along_hip()["front"]), 3, 2, up=0.3)

export(PLAN, frame="along")
