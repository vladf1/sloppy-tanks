# The Senda Spanish house (styles 3, 4 and 5), modelled after Rumbo north lot 5, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:spanish writes public/models/houses/spanish.glb and
#   crates/core/data/houses/spanish.plan.json)
#
# Drawn for the 6.2 x 8 m lot with its garage on the right (the game fits it to every lot of
# its styles: mirrored, stretched to the lot's box, wall top and ridge), at the level's scale
# (real sizes x 0.533). A two-storey stucco front gable stands over the entry side with twin
# arched windows high in it and a clay vent in its peak, under a clay hip whose ridge runs
# front to back; under it the tract's office projects under a clay lean-to with an arched
# street window, the entry passage beside it climbing to the front door under a little
# street gable on stucco arches; the garage's one-piece roll-up door stands under a clay
# pent and the plain upper floor, two windows over it. The paints are the style's
# (the game's palette).

import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded Spanish plan, which the street's layout reads
# (the path and mailbox find the door): the window (the entry passage's front door stands
# for it) and the door behind the office, the garage's two leaves and the windows over
# them; the twin arched windows in the gable, over the passage, are one upper window of
# the structure. The flanks and back: two or three
# windows a floor.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8, -1.91], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.4, "Window"], [2.1, "Window"]], "upper": [-1.6, 0.4, 2.1], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.9, "Window"]], "upper": [-2.3, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-2.5, "Window"], [0.0, "Window"], [2.3, "Window"]], "upper": [-2.5, 0.0, 2.3], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
window_u = _front["ground"][0][0]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- the roof
# The main hip with the entry side's gable at the front: the kit builds a gable over the
# garage side, so it is built there and turned across the middle.
GABLE_X = -1.6
hip_with_gables(-GABLE_X, front=True, back=False)
_SWAP = {"RightRoof": "LeftRoof", "LeftRoof": "RightRoof"}
_mirrored = defaultdict(lambda: ([], []))
for _key, (_vertices, _faces) in geometry.items():
    _part, _surface = _key.split("__")
    _target = _mirrored[f"{_SWAP.get(_part, _part)}__{_surface}"]
    _base = len(_target[0])
    _target[0].extend(Vector((-v.x, v.y, v.z)) for v in _vertices)
    _target[1].extend([i + _base for i in reversed(f)] for f in _faces)
geometry.clear()
geometry.update(_mirrored)

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
TWIN = (window_u - 0.34, window_u + 0.34)  # the gable's two arched windows, in one hole
TWIN_SPRING, TWIN_RISE = 2.62, 0.17
OVER_GARAGE = [(0.76, 1.24), (1.38, 2.3)]  # a single and a double window over the garage
holes = [GARAGE, door_hole(door_u), (TWIN[0], UPPER_GLASS[0], TWIN[1], TWIN_SPRING + TWIN_RISE)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in OVER_GARAGE]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
twin_mid = (TWIN[0] + TWIN[1]) / 2
for a, b in ((TWIN[0], twin_mid - 0.02), (twin_mid + 0.02, TWIN[1])):
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), TWIN_SPRING + TWIN_RISE, mullion=False)
    arch_head("FrontWall", front, fx(a), fx(b), TWIN_SPRING, (b - a) / 2, depth=WALL)
front.block("FrontWall__Trim", fx(twin_mid - 0.02), UPPER_GLASS[0], fx(twin_mid + 0.02), TWIN_SPRING + 0.04, 0.1, 0.03)
for a, b in OVER_GARAGE:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
# the door behind the office (the passage's front door stands for the coded window)
front_door(front, door_u)

# The garage's one-piece roll-up door in stained boards, under a clay pent.
sectional_garage_door(front, garage_u, decor=None, boards=True, header=False)
PENT = (GARAGE[0] - 0.15, GARAGE[2] + 0.2)
PENT_OUT, PENT_Y = 0.32, 1.42
tiled("FrontWall", Slope([(PENT[0], PENT_Y, HALF_D + PENT_OUT), (PENT[1], PENT_Y, HALF_D + PENT_OUT), (PENT[1], PENT_Y + PITCH * PENT_OUT, HALF_D), (PENT[0], PENT_Y + PITCH * PENT_OUT, HALF_D)]))

# The gable's stucco over the entry side, a clay vent in its peak.
def gable_y(x):
    """The gable's stucco top under its roof (the kit's gable end, turned across)."""
    return EAVE_Y + PITCH * (EAVE_X + GABLE_X - abs(x - GABLE_X)) - DECK * 1.1


_x0, _x1 = -HALF_W, 2 * GABLE_X + HALF_W
_outline = [(_x0, WALL_TOP), (_x1, WALL_TOP), (_x1, max(WALL_TOP, gable_y(_x1))), (GABLE_X, gable_y(GABLE_X)), (_x0, max(WALL_TOP, gable_y(_x0)))]
face("FrontWall__Siding", [(x, y, HALF_D) for x, y in _outline], (0, 0, 1))
face("FrontWall__Lining", [(x, y, HALF_D - WALL) for x, y in _outline], (0, 0, -1))
peak = gable_y(GABLE_X)
clay_vent("FrontWall", GABLE_X, peak - 0.25, HALF_D, spacing=0.05)

# ---------------------------------------------------------------- the entry
# Under the gable's twin windows, the office beside the garage under a clay lean-to, its
# arched street window, and the entry passage up its outer side to the front door under a
# small street gable on stucco arches.
office_entry(office_roof="shed", window_style="arched", cover="gable", supports="arches")

# ---------------------------------------------------------------- the flanks and back
plain_wall("back", PLAN, half_width={-1.6: 0.42, 0.4: 0.42, 2.1: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.3)

# The array: a block on the garage side's slope.
solar_array("RightRoof", Slope(main_hip()["right"]), 3, 2, up=0.15, across=0.45)

export(PLAN)
