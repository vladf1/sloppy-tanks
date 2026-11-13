# The Senda medium Spanish, for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/medium-spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/medium-spanish.glb and
#   crates/core/data/houses/medium-spanish.plan.json)
#
# The medium plan (the Prairie's, after Rumbo north lot 2) in the tract's Spanish dress; no house of
# the street is this one, so it borrows Rumbo north lots 5 and 1's vocabulary. Drawn in the street
# frame on the 6.2 x 8 m lot, its garage on the right; the game builds it with its door in
# the eave wall (its hip's ridge along the street). A broad two-storey warm stucco block
# under a clay barrel-tile hip; beside the garage the office under a clay hip, its street
# window arched, and the entry passage beside it under a clay shed on stucco arches (the
# kit's office_entry); twin arched windows over the office, a small single and a bare
# pair over the garage across the upper floor, clay pipe vents, and the garage's one-piece
# sectional board door with strap hinges under a clay pent. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The medium plan's openings (the street's layout reads its door and garage): the coded
# window (the entry passage's door stands over it) and the door behind the office, the garage's two leaves, and the spread front's five upper
# windows (the twin arched windows over the office, the single and the pair over the
# garage); two windows a floor down each flank and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.96, -0.36, 0.7, 1.7, 2.5], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.9, "Window"], [0.0, "Window"], [1.9, "Window"]], "upper": [-1.9, 0.0, 1.9], "garage_bay": 0.0},
        "right": {"ground": [[-2.4, "Window"], [1.5, "Window"]], "upper": [-2.4, 1.5], "garage_bay": 0.0},
        "left": {"ground": [[-1.8, "Window"], [2.2, "Window"]], "upper": [-1.8, 2.2], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]


def vent(part, frame, u, y, spacing=0.06):
    """Three clay pipe ends (real 10 cm) set in a wall, in a triangle (`clay_vent` on any
    wall)."""
    for du, dy in ((-spacing / 2, 0), (spacing / 2, 0), (0, spacing * 0.85)):
        frame.block(f"{part}__Terracotta", u + du - 0.02, y + dy - 0.02, u + du + 0.02, y + dy + 0.02, 0.02, -0.025)
        frame.quad(f"{part}__Dark", u + du - 0.011, y + dy - 0.011, u + du + 0.011, y + dy + 0.011, -0.027)


# ---------------------------------------------------------------- the front wall
# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
# the twin arched windows over the office, their arches rising over the upper glass
TWIN = [(-1.27, -0.69), (-0.61, -0.09)]
TWIN_TOP, TWIN_RISE = UPPER_GLASS[1] + 0.08, 0.16
SINGLE, PAIR = (0.7 - 0.24, 0.7 + 0.24), (1.7 - 0.27, 2.5 + 0.27)
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, TWIN_TOP) for a, b in TWIN]
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in (SINGLE, PAIR)]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
# the door into the house behind the office (the passage's door, at its end, is the entry's)
front_door(front, door_u)

for a, b in TWIN:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), TWIN_TOP, mullion=False)
    arch_head("FrontWall", front, fx(a), fx(b), TWIN_TOP - TWIN_RISE, TWIN_RISE, depth=WALL)
window("FrontWall", front, fx(SINGLE[0]), UPPER_GLASS[0], fx(SINGLE[1]), UPPER_GLASS[1], mullion=False)
# bare, like every window of the tract's Spanish fronts: board shutters are the Monterey's
window("FrontWall", front, fx(PAIR[0]), UPPER_GLASS[0], fx(PAIR[1]), UPPER_GLASS[1])
vent("FrontWall", front, fx(0.22), 2.96)

# The garage's one-piece sectional door in boards, strap hinges on its sections, under a
# clay pent.
sectional_garage_door(front, garage_u, decor="straps", boards=True, header=False)
PENT = (GARAGE[0] - 0.1, HALF_W + 0.05)
PENT_EAVE, PENT_OUT = GARAGE_TOP + 0.16, 0.38
tiled("FrontWall", Slope([(PENT[0], PENT_EAVE, HALF_D + PENT_OUT), (PENT[1], PENT_EAVE, HALF_D + PENT_OUT), (PENT[1], PENT_EAVE + 0.42 * PENT_OUT, HALF_D), (PENT[0], PENT_EAVE + 0.42 * PENT_OUT, HALF_D)]))
lantern(front, (GARAGE[2] + HALF_W) / 2 + 0.04, y=0.75)

# ---------------------------------------------------------------- the office and entry
# The office beside the garage under a clay hip, its street window arched; the entry
# passage up its outer side to the front door under a clay shed on stucco arches.
office_entry(office_roof="hip", window_style="arched", cover="shed", supports="arches")

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=False)
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-1.9: 0.4, 0.0: 0.3, 1.9: 0.4})
for side in ("right", "left"):
    plain_wall(side, PLAN, half_width=0.27)
for side in ("back", "right", "left"):
    vent(WALL_PARTS[side], WALL_FRAMES[side], 1.0 if side == "back" else -0.4, 2.62)

# The array where the tract's clay hips carry theirs: a squarish block high on the street
# slope, its top just under the ridge, off centre toward the garage.
solar_array("FrontRoof", Slope(along_hip()["front"]), 3, 2, up=0.5, across=0.6)

export(PLAN, frame="along")
