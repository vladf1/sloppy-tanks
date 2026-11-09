# The Senda Prairie house (styles 12 to 14), modelled after Rumbo north lot 2, for Sloppy Tanks in
# Blender (headless):
#   blender --background --python assets/blender/prairie.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:prairie writes public/models/houses/prairie.glb and
#   crates/core/data/houses/prairie.plan.json)
#
# Drawn in the street frame on a 6.2 x 8 m lot (the tract's 11.7 x 15 m box at the level's
# scale), its garage on the right; the game builds it with its door in the eave wall (its
# hip's ridge along the street) and fits it to every lot of its styles. A broad two-storey
# stucco block under a low flat-tile hip, three openings across its upper floor (a
# shuttered pair over the door, a small single and a bracketed pair over the garage), a
# one-storey office beside the garage under a flat-tile shed, the entry passage up its side
# under a small gable on tapered posts over stacked-stone piers, its wall clad in board and
# batten, and the garage's one-piece roll-up door with a row of lites. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Prairie front (the street's layout reads its door and garage) and its five
# upper windows; two windows a floor down each flank and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.96, -0.36, 0.7, 1.7, 2.5], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.7, "Window"], [0.2, "Window"], [1.9, "Window"]], "upper": [-1.7, 0.2, 1.9], "garage_bay": 0.0},
        "right": {"ground": [[-2.0, "Window"], [2.0, "Window"]], "upper": [-2.0, 2.0], "garage_bay": 0.0},
        "left": {"ground": [[-2.3, "Window"], [1.6, "Window"]], "upper": [-2.3, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]
up = _front["upper"]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
PAIR = 0.33  # how far a ganged pair runs past its windows' centres
UPPER = [(up[0] - PAIR, up[1] + PAIR), (up[2] - 0.25, up[2] + 0.25), (up[3] - PAIR, up[4] + PAIR)]  # the pair, the single, the pair
# The passage's front door stands for the coded ground window (its hole spans -1.91); the
# coded door stands behind the office.
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in UPPER]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor="lites", boards=True, header=True)
front_door(front, door_u)
for a, b in UPPER:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
board_shutters(front, fx(UPPER[0][0]), fx(UPPER[0][1]), UPPER_GLASS[0] - 0.01, UPPER_GLASS[1] + 0.01)
# brackets and a shelf under the pair over the garage
for u in (fx(UPPER[2][0]) + 0.12, fx(UPPER[2][1]) - 0.12):
    front.block("FrontWall__Wood", u - 0.03, UPPER_GLASS[0] - 0.18, u + 0.03, UPPER_GLASS[0] - 0.04, 0.0, -0.09)
front.block("FrontWall__Wood", fx(UPPER[2][0]) - 0.05, UPPER_GLASS[0] - 0.05, fx(UPPER[2][1]) + 0.05, UPPER_GLASS[0] - 0.015, 0.0, -0.1)

# The passage's wall clad in board and batten round the front door, under the cover.
CLAD = (PASSAGE_X[0] + 0.05, PASSAGE_X[1])
CLAD_FOOT, CLAD_TOP = PASSAGE_FLOOR + 0.02, COVER_EAVE + 0.2
entry0, _, entry1, entry_top = entry_holes()[0]
boards = [
    (CLAD[0], CLAD_FOOT, entry0 - 0.03, CLAD_TOP),
    (entry0 - 0.03, entry_top + 0.03, entry1 + 0.03, CLAD_TOP),
    (entry1 + 0.03, CLAD_FOOT, CLAD[1], CLAD_TOP),
]
for a, y0, b, y1 in boards:
    front.block("FrontWall__Cladding", fx(a), y0, fx(b), y1, 0.0, -0.012)
    u = a + 0.1
    while u < b - 0.04:
        front.block("FrontWall__Cladding", fx(u) - 0.01, y0, fx(u) + 0.01, y1, -0.012, -0.03)
        u += 0.2

# ---------------------------------------------------------------- the office and entry
# The office projecting before the left half under a flat-tile shed, the entry passage
# up its outer side under a small street gable on tapered posts over stone piers.
office_entry(office_roof="shed", flat=True, cover="gable", supports="tapered", window_style="header")

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
plain_wall("back", PLAN, half_width={-1.7: 0.42, 0.2: 0.42, 1.9: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

# The array: a small block on the street slope.
solar_array("FrontRoof", Slope(along_hip()["front"]), 2, 2, up=0.3, across=0.4)

export(PLAN, frame="along")
