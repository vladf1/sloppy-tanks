# The Senda small Spanish house, modelled after Rumbo north lot 1, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/small-spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/small-spanish.glb and
#   crates/core/data/houses/small-spanish.plan.json)
#
# Drawn for the 6.2 x 8 m lot with its garage on the right (the game fits it to every lot of
# its plan: mirrored, stretched to the lot's box, wall top and ridge), at the level's scale
# (real sizes x 0.533). A cream two-storey stucco house under a clay hip whose ridge runs
# front to back; before its left side the tract's office projects under a small clay gable
# with a vent and an arched street window, the entry passage beside it climbing to the
# front door under a little street gable on stucco arches; a tall stucco chimney rises in
# front over the entry; the garage's one-piece roll-up door, strap-hinged, is recessed under
# the plain upper floor, whose wall stands out over it on a scalloped band and dark corbels,
# a single and a double window above. The paints are the style's (the game's palette).

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded small Spanish plan, which the street's layout
# reads (the path and mailbox find the door): the window (the entry passage's front door
# stands for it), the door behind the office, the garage's two leaves and the windows over
# them (none over the door: the office's gable stands there). Two windows a floor down
# each flank, three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.4, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.4, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.4, "Window"], [1.5, "Window"]], "upper": [-2.4, 1.5], "garage_bay": 0.0},
        "left": {"ground": [[-1.8, "Window"], [1.6, "Window"]], "upper": [-1.8, 1.6], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
OVER_GARAGE = [(0.76, 1.24), (1.38, 2.3)]  # the single and the double window over the garage
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in OVER_GARAGE]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
for a, b in OVER_GARAGE:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
# the door behind the office (the passage's front door stands for the coded window)
front_door(front, door_u)

# The garage's one-piece roll-up door: stained boards, strap hinges.
sectional_garage_door(front, garage_u, decor="straps", boards=True)

# The upper floor stands out over the garage on a scalloped stucco band hung on dark
# corbels, as Rumbo north lot 1's does.
BAND = (0.0, HALF_W)  # x
BAND_Y = (1.4, 1.56)  # its corbels' line and its top
BAND_OUT = 0.09
SCALLOP_RISE = 0.07
CORBELS = [0.06, 1.0, 1.94, 2.88]
band_top, band_low = BAND_Y[1], BAND_Y[0]
for x0, x1 in zip(CORBELS, CORBELS[1:]):
    # a shallow arc between two corbels: the band's face and its soffit
    segments = 5
    arc = [
        (x0 + (x1 - x0) * k / segments, band_low + SCALLOP_RISE * math.sin(math.pi * k / segments))
        for k in range(segments + 1)
    ]
    for (xa, ya), (xb, yb) in zip(arc, arc[1:]):
        face("FrontWall__Siding", [(xa, ya, HALF_D + BAND_OUT), (xb, yb, HALF_D + BAND_OUT), (xb, band_top, HALF_D + BAND_OUT), (xa, band_top, HALF_D + BAND_OUT)], (0, 0, 1))
        face("FrontWall__Siding", [(xa, ya, HALF_D), (xb, yb, HALF_D), (xb, yb, HALF_D + BAND_OUT), (xa, ya, HALF_D + BAND_OUT)], (0, -1, 0))
# its ends beyond the outer corbels, its top and its ends
for xa, xb in ((BAND[0], CORBELS[0]), (CORBELS[-1], BAND[1])):
    face("FrontWall__Siding", [(xa, band_low, HALF_D + BAND_OUT), (xb, band_low, HALF_D + BAND_OUT), (xb, band_top, HALF_D + BAND_OUT), (xa, band_top, HALF_D + BAND_OUT)], (0, 0, 1))
    face("FrontWall__Siding", [(xa, band_low, HALF_D), (xb, band_low, HALF_D), (xb, band_low, HALF_D + BAND_OUT), (xa, band_low, HALF_D + BAND_OUT)], (0, -1, 0))
face("FrontWall__Siding", [(BAND[0], band_top, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[0], band_top, HALF_D + BAND_OUT)], (0, 1, 0))
face("FrontWall__Siding", [(BAND[0], band_low, HALF_D), (BAND[0], band_top, HALF_D), (BAND[0], band_top, HALF_D + BAND_OUT), (BAND[0], band_low, HALF_D + BAND_OUT)], (-1, 0, 0))
face("FrontWall__Siding", [(BAND[1], band_low, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[1], band_low, HALF_D + BAND_OUT)], (1, 0, 0))
for cx in CORBELS[1:-1] + [CORBELS[0] + 0.04, CORBELS[-1] - 0.04]:
    box("FrontWall__Wood", (cx - 0.04, band_low - 0.06, HALF_D), (cx + 0.04, band_low + 0.03, HALF_D + BAND_OUT + 0.05), skip=("-z",))

# A lantern beside the garage.
box("FrontWall__Iron", (2.8, 0.78, HALF_D), (2.88, 0.95, HALF_D + 0.07), skip=("-z",))
box("FrontWall__Iron", (2.82, 0.95, HALF_D + 0.01), (2.86, 0.99, HALF_D + 0.05), skip=("-z", "-y"))
# The downpipe at the house's right corner.
box("FrontWall__Metal", (HALF_W - 0.07, 0.0, HALF_D + 0.01), (HALF_W - 0.03, EAVE_Y - 0.05, HALF_D + 0.05), skip=("-z", "-y", "+y"))

# ---------------------------------------------------------------- the main hip
main = main_hip()
tiled("RightRoof", Slope(main["right"]))
tiled("LeftRoof", Slope(main["left"]))
tiled("FrontRoof", Slope(main["front"]))
tiled("BackRoof", Slope(main["back"]))
ridge_cap("LeftRoof", *main["ridge"])
for a, b in main["hips"]:
    ridge_cap("BackRoof" if a[2] < 0 else "FrontRoof", a, b)

# ---------------------------------------------------------------- the entry
# The office beside the garage under a small clay gable with a vent, its arched street
# window, and the entry passage up its outer side to the front door under a small street
# gable on stucco arches.
office_entry(office_roof="gable", vent=True, window_style="arched", cover="gable", supports="arches")

# ---------------------------------------------------------------- the chimney
# Tall stucco (a real 0.75 m stack), standing in front of the house over the entry's
# cover and past the main ridge, a shoulder stepped in near the top.
CHIMNEY = (-2.45, -2.05, HALF_D - 0.18, HALF_D + 0.22)
CHIMNEY_TOP = 4.8
box("FrontWall__Siding", (CHIMNEY[0], 2.0, CHIMNEY[2]), (CHIMNEY[1], 4.05, CHIMNEY[3]), skip=("-y",))
box("FrontWall__Siding", (CHIMNEY[0] + 0.08, 4.05, CHIMNEY[2] + 0.02), (CHIMNEY[1], CHIMNEY_TOP - 0.04, CHIMNEY[3]), skip=("-y",))
box("FrontWall__Siding", (CHIMNEY[0] + 0.06, CHIMNEY_TOP - 0.04, CHIMNEY[2]), (CHIMNEY[1] + 0.02, CHIMNEY_TOP, CHIMNEY[3] + 0.02), skip=())
box("FrontWall__Dark", (CHIMNEY[0] + 0.2, CHIMNEY_TOP - 0.13, CHIMNEY[3]), (CHIMNEY[1] - 0.04, CHIMNEY_TOP - 0.08, CHIMNEY[3] + 0.004), skip=("-z",))

# ---------------------------------------------------------------- the flanks and back
plain_wall("back", PLAN, half_width={-1.6: 0.42, 0.4: 0.35, 2.0: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.3)

# The array: a small block on the right slope, toward the street.
solar_array("RightRoof", Slope(main["right"]), 2, 2, up=0.2, across=0.35)

export(PLAN)
