# The Senda Spanish house (styles 3, 4 and 5), modelled after Rumbo north lot 5, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:spanish writes public/models/houses/spanish.glb and
#   crates/core/data/houses/spanish.plan.json)
#
# Drawn for a 5 x 7 m lot with its garage on the right (the game fits it to every lot of
# its styles: mirrored, stretched to the lot's box, wall top and ridge). A two-storey stucco
# front gable stands over the entry side with twin arched windows high in it and a clay
# vent in its peak; before it a one-storey front-gabled wing opens in an arch onto the
# entry, a window beside it, a clay vent in its gable; the garage's one-piece door stands
# under a clay pent and the plain upper floor, two windows over it, under the main clay hip.
# The paints are the style's (the game's palette).

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

FX = lambda x: x + HALF_W  # noqa: E731  front wall u of x (the front frame starts at the left corner)
front = Frame((-HALF_W, 0, HALF_D), (1, 0, 0), (0, 0, 1))

# The openings. The front keeps the coded Spanish plan, which the street's layout reads
# (the path and mailbox find the door): the door bay opening into the wing, the garage's
# two leaves and the windows over them; the twin arched windows over the door bay are one
# upper window of the structure. The flanks and back: two windows each, lit upstairs.
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
_front = PLAN["walls"]["front"]
door_u = _front["ground"][0][0]
garage_u, garage_bay = _front["ground"][1][0], _front["garage_bay"]

# ---------------------------------------------------------------- the front wall
GARAGE = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
TWIN = [(-1.52, -1.1), (-1.0, -0.58)]  # the gable's arched windows (x), within the door bay
TWIN_SPRING, TWIN_RISE, TWIN_SILL = 2.66, 0.2, 2.18
OVER_GARAGE = [(0.36, 0.86), (1.02, 1.62)]  # a single and a double window over the garage
holes = [
    (FX(GARAGE[0]), GARAGE[1], FX(GARAGE[2]), GARAGE[3]),
    (FX(door_u - DOOR_BAY + 0.1), 0.12, FX(door_u + DOOR_BAY - 0.1), 1.55),
]
holes += [(FX(a), TWIN_SILL, FX(b), TWIN_SPRING + TWIN_RISE) for a, b in TWIN]
holes += [(FX(a), UPPER_GLASS[0], FX(b), UPPER_GLASS[1]) for a, b in OVER_GARAGE]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
for a, b in TWIN:
    window("FrontWall", front, FX(a), TWIN_SILL, FX(b), TWIN_SPRING + TWIN_RISE, mullion=False)
    arch_head("FrontWall", front, FX(a), FX(b), TWIN_SPRING, TWIN_RISE, depth=WALL)
for a, b in OVER_GARAGE:
    window("FrontWall", front, FX(a), UPPER_GLASS[0], FX(b), UPPER_GLASS[1], mullion=b - a > 0.55)

# The garage's one-piece door: stained vertical boards, set back, under a clay pent.
g0, g1 = FX(GARAGE[0]), FX(GARAGE[2])
front.quad("FrontWall__GarageDoor", g0, GARAGE[1], g1, GARAGE[3], 0.12)
u = g0 + 0.1
while u < g1 - 0.05:
    front.block("FrontWall__GarageDoor", u - 0.012, GARAGE[1] + 0.04, u + 0.012, GARAGE[3] - 0.04, 0.12, 0.105)
    u += 0.14
front.block("FrontWall__Trim", g0 - 0.05, GARAGE[3], g1 + 0.05, GARAGE[3] + 0.05, 0.0, -0.03)
PENT = (GARAGE[0] - 0.25, GARAGE[2] + 0.3)
tiled("FrontWall", Slope([(PENT[0], 1.62, HALF_D + 0.5), (PENT[1], 1.62, HALF_D + 0.5), (PENT[1], 1.82, HALF_D), (PENT[0], 1.82, HALF_D)]))

# The door at the back of the entry, in the door bay.
front.quad("FrontWall__Accent", FX(door_u - 0.3), 0.12, FX(door_u + 0.3), 1.45, 0.1)
front.block("FrontWall__Trim", FX(door_u - 0.36), 1.45, FX(door_u + 0.36), 1.5, 0.08, 0.04)

# ---------------------------------------------------------------- the front gable
GABLE = (-2.8, -0.3)  # its roof's eaves, over the entry side's two storeys
GABLE_PITCH = 0.78
gable_eave = 3.0
gable_apex = gable_eave + GABLE_PITCH * (GABLE[1] - GABLE[0]) / 2
peak = gable_face("FrontWall", HALF_D, -HALF_W, -0.55, WALL_TOP, GABLE[0], GABLE[1], gable_eave, GABLE_PITCH)
clay_vent("FrontWall", (GABLE[0] + GABLE[1]) / 2, peak - 0.32, HALF_D)
gable_roof("FrontRoof", GABLE[0], GABLE[1], 0.3, HALF_D + 0.36, gable_eave, GABLE_PITCH)

# ---------------------------------------------------------------- the main hip
main = hip_slopes(-EAVE_X, EAVE_X, -EAVE_Z, EAVE_Z, EAVE_Y, (RIDGE_Y - EAVE_Y) / EAVE_X, along_x=False)
tiled("RightRoof", Slope(main["right"]))
tiled("LeftRoof", Slope(main["left"]))
# The front hip end only right of the gable, and behind its wall beneath it.
tiled("FrontRoof", Slope(clip(main["front"], 0, GABLE[1] - 0.15, True)))
tiled("FrontRoof", Slope(clip(clip(main["front"], 0, GABLE[1] - 0.15, False), 2, HALF_D - 0.05, False)))
tiled("BackRoof", Slope(main["back"]))
ridge_cap("LeftRoof", *main["ridge"])
for a, b in main["hips"]:
    ridge_cap("BackRoof" if a[2] < 0 else "FrontRoof", a, b)

# ---------------------------------------------------------------- the entry wing
WING = (-2.3, -0.7, HALF_D, 5.15)
WING_TOP = 1.6
WING_PITCH = 0.78
wing_front = Frame((WING[0], 0, WING[3]), (1, 0, 0), (0, 0, 1))
wing_left = Frame((WING[0], 0, HALF_D), (0, 0, 1), (-1, 0, 0))
wing_right = Frame((WING[1], 0, WING[3]), (0, 0, -1), (1, 0, 0))
wl, wd = WING[1] - WING[0], WING[3] - WING[2]
ARCH = (wl - 0.62, wl - 0.18)  # the open arch, toward the garage
WING_WINDOW = (0.2, 0.68)
wall("FrontWall", wing_front, 0, wl, 0, WING_TOP, [(ARCH[0], 0.12, ARCH[1], 1.38), (WING_WINDOW[0], 0.55, WING_WINDOW[1], 1.32)])
arch_head("FrontWall", wing_front, ARCH[0], ARCH[1], 1.16, 0.22, depth=WALL)
window("FrontWall", wing_front, WING_WINDOW[0], 0.55, WING_WINDOW[1], 1.32, mullion=False)
wall("FrontWall", wing_left, 0, wd, 0, WING_TOP, [(0.55, 0.62, 0.95, 1.3)])
window("FrontWall", wing_left, 0.55, 0.62, 0.95, 1.3, mullion=False)
wall("FrontWall", wing_right, 0, wd, 0, WING_TOP, [])
# Its entry's floor and steps up to the arch.
box("FrontWall__Concrete", (WING[0] + ARCH[0] - 0.1, 0.0, WING[3] - 1.2), (WING[0] + ARCH[1] + 0.1, 0.1, WING[3]))
for k, depth in enumerate((0.2, 0.4)):
    box("FrontWall__Concrete", (WING[0] + ARCH[0] - 0.12, 0.0, WING[3]), (WING[0] + ARCH[1] + 0.12, 0.1 - 0.04 * (k + 1) + 0.04, WING[3] + depth))
wing_apex = gable_roof("FrontWall", WING[0] - 0.22, WING[1] + 0.22, HALF_D - 0.02, WING[3] + 0.3, WING_TOP - 0.04, WING_PITCH)
wing_peak = gable_face("FrontWall", WING[3], WING[0], WING[1], WING_TOP, WING[0] - 0.22, WING[1] + 0.22, WING_TOP - 0.04, WING_PITCH)
clay_vent("FrontWall", (WING[0] + WING[1]) / 2, wing_peak - 0.24, WING[3])

# ---------------------------------------------------------------- the flanks and back
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

# The array: a block on the back slope.
solar_array("BackRoof", Slope(main["back"]), 3, 2, up=0.06)

export(PLAN)
