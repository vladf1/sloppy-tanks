# The Senda small Spanish house, modelled after Rumbo north lot 1, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/small-spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/small-spanish.glb and
#   crates/core/data/houses/small-spanish.plan.json)
#
# Drawn for a 5 x 7 m lot with its garage on the right (the game fits it to every lot of its
# plan: mirrored, stretched to the lot's box, wall top and ridge). A cream two-storey stucco
# house under a clay hip; before its left side a tall one-storey front-gabled wing opens at
# its corner in an open arch (a loggia, arched on its front and its flank, the front door
# inside it) beside a large arched feature window in a tile surround over a tile sill, a
# clay vent in its gable; a tall stucco chimney rises in front where the wing meets the
# house; the garage's one-piece door is recessed under the plain upper floor, whose wall
# stands out over it on a scalloped band and dark corbels, a single and a double window
# above. The paints are the style's (the game's palette).

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

FX = lambda x: x + HALF_W  # noqa: E731  front wall u of x (the front frame starts at the left corner)
front = Frame((-HALF_W, 0, HALF_D), (1, 0, 0), (0, 0, 1))

# The openings. The front keeps the small Spanish plan, which the street's layout reads
# (the path and mailbox find the door): the door bay opening into the wing, the garage's
# two leaves and the windows over them. The flanks and back: two windows each, lit upstairs.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [0.68, 1.18], "garage_bay": 0.62},
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
GARAGE_SET = 0.16  # the door's depth into its opening
OVER_GARAGE = [(0.4, 0.8), (1.0, 1.95)]  # the single and the double window over the garage
holes = [
    (FX(GARAGE[0]), GARAGE[1], FX(GARAGE[2]), GARAGE[3]),
    (FX(door_u - DOOR_BAY + 0.1), 0.12, FX(door_u + DOOR_BAY - 0.1), 1.55),  # into the wing
]
holes += [(FX(a), UPPER_GLASS[0], FX(b), UPPER_GLASS[1]) for a, b in OVER_GARAGE]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
for a, b in OVER_GARAGE:
    window("FrontWall", front, FX(a), UPPER_GLASS[0], FX(b), UPPER_GLASS[1], mullion=b - a > 0.55)

# The garage's one-piece door: stained vertical boards, set back.
g0, g1 = FX(GARAGE[0]), FX(GARAGE[2])
front.quad("FrontWall__GarageDoor", g0, GARAGE[1], g1, GARAGE[3], GARAGE_SET)
u = g0 + 0.1
while u < g1 - 0.05:
    front.block("FrontWall__GarageDoor", u - 0.012, GARAGE[1] + 0.04, u + 0.012, GARAGE[3] - 0.04, GARAGE_SET, GARAGE_SET - 0.015)
    u += 0.14
front.block("FrontWall__Dark", g0, GARAGE[1] + 0.02, g1, GARAGE[1] + 0.05, GARAGE_SET, GARAGE_SET - 0.01)

# The upper floor stands out over the garage on a scalloped stucco band hung on dark
# corbels, as Rumbo north lot 1's does.
BAND = (0.02, HALF_W)  # x
BAND_Y = (1.68, 1.94)  # its corbels' line and its top
BAND_OUT = 0.14
SCALLOP_RISE = 0.11
CORBELS = [0.08, 0.86, 1.64, 2.42]
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
# its ends beyond the outer corbels, its top and its left end
for xa, xb in ((BAND[0], CORBELS[0]), (CORBELS[-1], BAND[1])):
    face("FrontWall__Siding", [(xa, band_low, HALF_D + BAND_OUT), (xb, band_low, HALF_D + BAND_OUT), (xb, band_top, HALF_D + BAND_OUT), (xa, band_top, HALF_D + BAND_OUT)], (0, 0, 1))
    face("FrontWall__Siding", [(xa, band_low, HALF_D), (xb, band_low, HALF_D), (xb, band_low, HALF_D + BAND_OUT), (xa, band_low, HALF_D + BAND_OUT)], (0, -1, 0))
face("FrontWall__Siding", [(BAND[0], band_top, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[0], band_top, HALF_D + BAND_OUT)], (0, 1, 0))
face("FrontWall__Siding", [(BAND[0], band_low, HALF_D), (BAND[0], band_top, HALF_D), (BAND[0], band_top, HALF_D + BAND_OUT), (BAND[0], band_low, HALF_D + BAND_OUT)], (-1, 0, 0))
face("FrontWall__Siding", [(BAND[1], band_low, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[1], band_low, HALF_D + BAND_OUT)], (1, 0, 0))
for cx in CORBELS[1:-1] + [CORBELS[0] + 0.05, CORBELS[-1] - 0.05]:
    box("FrontWall__Wood", (cx - 0.06, band_low - 0.08, HALF_D), (cx + 0.06, band_low + 0.04, HALF_D + BAND_OUT + 0.08), skip=("-z",))

# A lantern beside the garage.
box("FrontWall__Iron", (2.2, 1.12, HALF_D), (2.34, 1.42, HALF_D + 0.12), skip=("-z",))
box("FrontWall__Iron", (2.24, 1.42, HALF_D + 0.02), (2.3, 1.48, HALF_D + 0.08), skip=("-z", "-y"))
# The downpipe at the house's right corner.
box("FrontWall__Metal", (HALF_W - 0.1, 0.0, HALF_D + 0.02), (HALF_W - 0.04, EAVE_Y - 0.05, HALF_D + 0.08), skip=("-z", "-y", "+y"))

# ---------------------------------------------------------------- the main hip
main = hip_slopes(-EAVE_X, EAVE_X, -EAVE_Z, EAVE_Z, EAVE_Y, (RIDGE_Y - EAVE_Y) / EAVE_X, along_x=False)
tiled("RightRoof", Slope(main["right"]))
tiled("LeftRoof", Slope(main["left"]))
tiled("FrontRoof", Slope(main["front"]))
tiled("BackRoof", Slope(main["back"]))
ridge_cap("LeftRoof", *main["ridge"])
for a, b in main["hips"]:
    ridge_cap("BackRoof" if a[2] < 0 else "FrontRoof", a, b)

# ---------------------------------------------------------------- the wing
WING = (-2.55, -0.3, HALF_D, 5.2)  # x0, x1, z0, z1
WING_TOP = 2.0
WING_PITCH = 0.45
wing_front = Frame((WING[0], 0, WING[3]), (1, 0, 0), (0, 0, 1))
wing_left = Frame((WING[0], 0, HALF_D), (0, 0, 1), (-1, 0, 0))
wing_right = Frame((WING[1], 0, WING[3]), (0, 0, -1), (1, 0, 0))
wl, wd = WING[1] - WING[0], WING[3] - WING[2]
FLOOR = 0.1
# the loggia's open arches, on the wing's front at its corner and on its flank
LOGGIA_X = -1.7  # the partition between the loggia and the wing's room
ARCH_HALF, ARCH_SPRING = 0.3, 1.3
FRONT_ARCH = (0.14, 0.14 + 2 * ARCH_HALF)
SIDE_ARCH = (wd - 0.2 - 2 * ARCH_HALF - 0.12, wd - 0.2 - 0.12)
# the feature window
FEATURE = (-1.42 - WING[0], -0.62 - WING[0])  # u along the wing's front
FEATURE_SILL, FEATURE_SPRING = 0.62, 1.42
feature_half = (FEATURE[1] - FEATURE[0]) / 2
wall("FrontWall", wing_front, 0, wl, 0, WING_TOP, [
    (FRONT_ARCH[0], FLOOR, FRONT_ARCH[1], ARCH_SPRING + ARCH_HALF),
    (FEATURE[0], FEATURE_SILL, FEATURE[1], FEATURE_SPRING + feature_half),
])
arch_head("FrontWall", wing_front, *FRONT_ARCH, ARCH_SPRING, ARCH_HALF, depth=WALL)
arch_head("FrontWall", wing_front, *FEATURE, FEATURE_SPRING, feature_half, depth=WALL)
wall("FrontWall", wing_left, 0, wd, 0, WING_TOP, [(SIDE_ARCH[0], FLOOR, SIDE_ARCH[1], ARCH_SPRING + ARCH_HALF)])
arch_head("FrontWall", wing_left, *SIDE_ARCH, ARCH_SPRING, ARCH_HALF, depth=WALL)
wall("FrontWall", wing_right, 0, wd, 0, WING_TOP, [])

# The feature window: glass under muntins, a tile surround round its arch and a tile sill.
f0, f1 = FEATURE
fm = (f0 + f1) / 2
wing_front.quad("FrontWall__Glass", f0, FEATURE_SILL, f1, FEATURE_SPRING + feature_half, 0.1)


def arch_y(u, half, spring, mid):
    """The arch's height over u (u within it)."""
    return spring + math.sqrt(max(0.0, half * half - (u - mid) ** 2))


bar = 0.022
for mu in (f0 + (f1 - f0) / 3, f0 + 2 * (f1 - f0) / 3):
    top = arch_y(mu, feature_half, FEATURE_SPRING, fm)
    wing_front.block("FrontWall__Trim", mu - bar, FEATURE_SILL, mu + bar, top, 0.1, 0.075)
for my in (FEATURE_SILL + 0.4, FEATURE_SPRING):
    wing_front.block("FrontWall__Trim", f0, my - bar, f1, my + bar, 0.1, 0.075)
TILE = 0.1  # the surround's width
segments = 8
for k in range(segments):
    t0, t1 = math.pi * k / segments, math.pi * (k + 1) / segments
    inner = [(fm - feature_half * math.cos(t), FEATURE_SPRING + feature_half * math.sin(t)) for t in (t0, t1)]
    outer = [(fm - (feature_half + TILE) * math.cos(t), FEATURE_SPRING + (feature_half + TILE) * math.sin(t)) for t in (t0, t1)]
    key = "FrontWall__Terracotta" if k % 2 == 0 else "FrontWall__Accent"
    d = -0.025
    face(key, [wing_front.p(*inner[0], d), wing_front.p(*inner[1], d), wing_front.p(*outer[1], d), wing_front.p(*outer[0], d)], wing_front.out)
    face(key, [wing_front.p(*outer[0], d), wing_front.p(*outer[1], d), wing_front.p(*outer[1], 0), wing_front.p(*outer[0], 0)], None)
for a, b in ((f0 - TILE, f0), (f1, f1 + TILE)):
    wing_front.block("FrontWall__Terracotta", a, FEATURE_SILL, b, FEATURE_SPRING, 0.0, -0.025)
wing_front.block("FrontWall__Terracotta", f0 - TILE - 0.06, FEATURE_SILL - 0.16, f1 + TILE + 0.06, FEATURE_SILL, 0.0, -0.1)
wing_front.block("FrontWall__Terracotta", f0 - TILE - 0.03, FEATURE_SILL - 0.22, f1 + TILE + 0.03, FEATURE_SILL - 0.16, 0.0, -0.06)

# The loggia: the partition to the wing's room, the front door in it facing the arches, a
# floor and a step out of the front arch.
partition = Frame((LOGGIA_X, 0, HALF_D), (0, 0, 1), (-1, 0, 0))
wall("FrontWall", partition, 0, wd - WALL, 0, WING_TOP, [])
DOOR = (0.3, 1.0)
partition.block("FrontWall__Accent", DOOR[0], FLOOR, DOOR[1], 1.5, 0.0, -0.04)
partition.block("FrontWall__Trim", DOOR[0] - 0.06, 1.5, DOOR[1] + 0.06, 1.56, 0.0, -0.05)
for a, b in ((DOOR[0] - 0.06, DOOR[0]), (DOOR[1], DOOR[1] + 0.06)):
    partition.block("FrontWall__Trim", a, FLOOR, b, 1.5, 0.0, -0.05)
partition.block("FrontWall__Iron", DOOR[1] - 0.12, 0.8, DOOR[1] - 0.09, 0.95, -0.04, -0.07)
box("FrontWall__Concrete", (WING[0] + WALL, 0.0, HALF_D), (LOGGIA_X, FLOOR, WING[3] - WALL))
box("FrontWall__Concrete", (WING[0] + FRONT_ARCH[0], 0.0, WING[3] - WALL), (WING[0] + FRONT_ARCH[1], FLOOR, WING[3]), skip=("-y", "-z"))
box("FrontWall__Concrete", (WING[0] + FRONT_ARCH[0] - 0.08, 0.0, WING[3]), (WING[0] + FRONT_ARCH[1] + 0.08, 0.06, WING[3] + 0.32), skip=("-y", "-z"))
box("FrontWall__Concrete", (WING[0] - WALL, 0.0, HALF_D + SIDE_ARCH[0]), (WING[0], FLOOR, HALF_D + SIDE_ARCH[1]), skip=("-y", "+x"))

# Its gable roof, the stucco gable over its front with a clay vent, and the downpipe where
# it meets the garage.
WING_EAVES = (WING[0] - 0.2, WING[1] + 0.2)
gable_roof("FrontWall", *WING_EAVES, HALF_D - 0.02, WING[3] + 0.3, WING_TOP - 0.04, WING_PITCH)
wing_peak = gable_face("FrontWall", WING[3], WING[0], WING[1], WING_TOP, *WING_EAVES, WING_TOP - 0.04, WING_PITCH)
clay_vent("FrontWall", (WING[0] + WING[1]) / 2, wing_peak - 0.24, WING[3])
box("FrontWall__Metal", (WING[1] + 0.02, 0.0, WING[3] - 0.08), (WING[1] + 0.08, WING_TOP - 0.06, WING[3] - 0.02), skip=("-x", "-y", "+y"))

# ---------------------------------------------------------------- the chimney
# Tall stucco, standing in front of the house where the wing meets it, out of the wing's
# roof and past the main ridge, a shoulder stepped in on its left near the top.
CHIMNEY = (-0.95, -0.4, HALF_D - 0.3, HALF_D + 0.28)
CHIMNEY_TOP = 4.42
box("FrontWall__Siding", (CHIMNEY[0], 2.0, CHIMNEY[2]), (CHIMNEY[1], 3.75, CHIMNEY[3]), skip=("-y",))
box("FrontWall__Siding", (CHIMNEY[0] + 0.14, 3.75, CHIMNEY[2] + 0.03), (CHIMNEY[1], CHIMNEY_TOP - 0.06, CHIMNEY[3]), skip=("-y",))
box("FrontWall__Siding", (CHIMNEY[0] + 0.11, CHIMNEY_TOP - 0.06, CHIMNEY[2]), (CHIMNEY[1] + 0.03, CHIMNEY_TOP, CHIMNEY[3] + 0.03), skip=())
box("FrontWall__Dark", (CHIMNEY[0] + 0.3, CHIMNEY_TOP - 0.2, CHIMNEY[3]), (CHIMNEY[1] - 0.06, CHIMNEY_TOP - 0.12, CHIMNEY[3] + 0.005), skip=("-z",))

# ---------------------------------------------------------------- the flanks and back
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

export(PLAN)
