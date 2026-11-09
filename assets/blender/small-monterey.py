# The reference house (Rumbo north lot 4), modelled for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/small-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:small-monterey writes public/models/houses/small-monterey.glb and its plan,
#   crates/core/data/houses/small-monterey.plan.json)
#
# The house is authored in the game's build frame for its lot (metres: front +z, the
# garage side +x, y up; walls x +-2.5, z +-3.5, wall top 3.15, hip ridge 4.0), the lot's
# compressed two storeys. Every object is named <HousePart>__<Surface> after the coded
# house's pieces (crates/core/src/sim/house_layout.rs HousePart, building_kit.rs Surface)
# so the structural house cuts it the way it cuts the coded meshes: walls by side with an
# inner lining, the four hip slopes, and (as the coded house has it) the front gable's
# wall, the wing, porch and bay in the front wall's piece and the gable roof in the front
# slope's.
#
# The openings are declared once, in PLAN, in the game's wall-plan form (each wall's
# ground-floor openings by centre along the wall and kind, the upper windows over them):
# the model cuts its windows there and the script writes PLAN for the game, whose
# structure frames its window, door and garage parts from it (HouseDesign::wall_plan).
# That file is gameplay data: it shapes the house's colliders.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# ---------------------------------------------------------------- the house

front = Frame((-HALF_W, 0, HALF_D), (1, 0, 0), (0, 0, 1))  # u = x + 2.5
FX = lambda x: x + HALF_W  # noqa: E731  front wall u of x

# The reference house's openings. The front keeps the coded plan, which the street's layout reads
# (the path and mailbox find the door): the door bay opening into the wing, the garage's
# two leaves, the upper left window and the box bay's ganged pair over the garage. The
# flanks and back follow the house: two windows down each side, lit upstairs too (the
# east side's toward the street and the stair, the west side's by the wing), and a pair
# across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [-1.23, 0.68, 1.18], "garage_bay": 0.62},
        "back": {"ground": [[-1.2, "Window"], [1.2, "Window"]], "upper": [-1.2, 1.2], "garage_bay": 0.0},
        "right": {"ground": [[-1.7, "Window"], [1.4, "Window"]], "upper": [-1.7, 1.4], "garage_bay": 0.0},
        "left": {"ground": [[-1.6, "Window"], [1.7, "Window"]], "upper": [-1.6, 1.7], "garage_bay": 0.0},
    },
}


# Front: the garage under the gable (the coded structure's two door bays, x -0.61..2.2),
# the box bay over it, the upper left window with its one slate shutter, the wing before
# the lower left (its door bay x -1.85..-0.61 opens into the wing).
_front = PLAN["walls"]["front"]
_door_u = _front["ground"][0][0]
_garage_u, _garage_bay = _front["ground"][1][0], _front["garage_bay"]
GARAGE = (_garage_u - 2 * _garage_bay + 0.14, 0.12, _garage_u + 2 * _garage_bay - 0.12, 1.5)
UPPER_LEFT = (_door_u - 0.32, 2.02, _door_u + 0.32, 2.86)
BAY_OPEN = (_front["upper"][1] - 0.33, 1.86, _front["upper"][2] + 0.33, 2.8)
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP,
     [(FX(GARAGE[0]), GARAGE[1], FX(GARAGE[2]), GARAGE[3]),
      (FX(UPPER_LEFT[0]), UPPER_LEFT[1], FX(UPPER_LEFT[2]), UPPER_LEFT[3]),
      (FX(BAY_OPEN[0]), BAY_OPEN[1], FX(BAY_OPEN[2]), BAY_OPEN[3]),
      (FX(_door_u - DOOR_BAY + 0.1), 0.12, FX(_door_u + DOOR_BAY - 0.1), 1.55)])  # into the wing
window("FrontWall", front, FX(UPPER_LEFT[0]), UPPER_LEFT[1], FX(UPPER_LEFT[2]), UPPER_LEFT[3])
shutter("FrontWall", front, FX(UPPER_LEFT[0]) - 0.36, UPPER_LEFT[1] - 0.02, FX(UPPER_LEFT[0]) - 0.04, UPPER_LEFT[3] + 0.02)

# Carriage garage door: stained boards in two leaves, set back in the opening.
g0, g1 = FX(GARAGE[0]), FX(GARAGE[2])
front.quad("FrontWall__GarageDoor", g0, GARAGE[1], g1, GARAGE[3], 0.12)
u = g0 + 0.16
while u < g1 - 0.1:
    front.block("FrontWall__GarageDoor", u - 0.02, GARAGE[1], u + 0.02, GARAGE[3] - 0.06, 0.12, 0.1)
    u += 0.21
mid = (g0 + g1) / 2
front.block("FrontWall__Dark", mid - 0.015, GARAGE[1], mid + 0.015, GARAGE[3], 0.12, 0.095)
front.block("FrontWall__GarageDoor", g0, GARAGE[3] - 0.08, g1, GARAGE[3] - 0.02, 0.12, 0.09)
for handle in (mid - 0.18, mid + 0.18):
    front.block("FrontWall__Iron", handle - 0.012, 0.62, handle + 0.012, 0.86, 0.09, 0.07)
front.block("FrontWall__Trim", g0 - 0.06, GARAGE[3], g1 + 0.06, GARAGE[3] + 0.06, 0.0, -0.03)

# The dark box bay over the garage: stained boards, two windows, brackets and a tile hood.
b0, b1 = FX(BAY_OPEN[0]) - 0.08, FX(BAY_OPEN[2]) + 0.08
BAY_OUT = 0.3
bay_lo, bay_hi = 1.72, 2.86
front.block("FrontWall__Wood", b0, bay_lo, b1, bay_lo + 0.08, 0.0, -BAY_OUT)
front.block("FrontWall__Wood", b0, bay_hi - 0.05, b1, bay_hi, 0.0, -BAY_OUT)
front.block("FrontWall__Wood", b0, bay_lo, b0 + 0.06, bay_hi, 0.0, -BAY_OUT)
front.block("FrontWall__Wood", b1 - 0.06, bay_lo, b1, bay_hi, 0.0, -BAY_OUT)
# its lower panel, board and batten
front.block("FrontWall__Wood", b0, bay_lo, b1, 2.12, -BAY_OUT + 0.03, -BAY_OUT)
u = b0 + 0.12
while u < b1 - 0.05:
    front.block("FrontWall__Wood", u - 0.012, bay_lo + 0.02, u + 0.012, 2.12, -BAY_OUT, -BAY_OUT - 0.02)
    u += 0.14
for wu0, wu1 in ((b0 + 0.08, (b0 + b1) / 2 - 0.03), ((b0 + b1) / 2 + 0.03, b1 - 0.08)):
    face("FrontWall__Curtain", [front.p(wu0, 2.14, -BAY_OUT + 0.03), front.p(wu1, 2.14, -BAY_OUT + 0.03), front.p(wu1, bay_hi - 0.06, -BAY_OUT + 0.03), front.p(wu0, bay_hi - 0.06, -BAY_OUT + 0.03)], (0, 0, 1))
    window_frame = Frame(front.p(0, 0, -BAY_OUT + 0.14), front.along, front.out)
    for (a, c, b, d) in ((wu0, 2.14, wu1, 2.18), (wu0, bay_hi - 0.1, wu1, bay_hi - 0.06), (wu0, 2.14, wu0 + 0.04, bay_hi - 0.06), (wu1 - 0.04, 2.14, wu1, bay_hi - 0.06)):
        front.block("FrontWall__Trim", a, c, b, d, -BAY_OUT + 0.03, -BAY_OUT)
    front.block("FrontWall__Wood", (b0 + b1) / 2 - 0.03, 2.12, (b0 + b1) / 2 + 0.03, bay_hi, -BAY_OUT + 0.02, -BAY_OUT - 0.01)
# its sides close the box
for su in (b0, b1):
    face("FrontWall__Wood", [front.p(su, bay_lo, 0), front.p(su, bay_hi, 0), front.p(su, bay_hi, -BAY_OUT), front.p(su, bay_lo, -BAY_OUT)], front.along * (1 if su == b1 else -1))
for bu in (b0 + 0.12, b1 - 0.12):
    face("FrontWall__Wood", [front.p(bu - 0.04, bay_lo, 0), front.p(bu - 0.04, bay_lo, -BAY_OUT + 0.02), front.p(bu - 0.04, bay_lo - 0.22, 0)], -front.along)
    face("FrontWall__Wood", [front.p(bu + 0.04, bay_lo, 0), front.p(bu + 0.04, bay_lo - 0.22, 0), front.p(bu + 0.04, bay_lo, -BAY_OUT + 0.02)], front.along)
    face("FrontWall__Wood", [front.p(bu - 0.04, bay_lo, -BAY_OUT + 0.02), front.p(bu + 0.04, bay_lo, -BAY_OUT + 0.02), front.p(bu + 0.04, bay_lo - 0.22, 0), front.p(bu - 0.04, bay_lo - 0.22, 0)], (0, -1, 1))
hood_z = HALF_D + BAY_OUT + 0.12
tiled("FrontWall", Slope([(b0 - 0.1 - HALF_W, bay_hi + 0.02, hood_z), (b1 + 0.1 - HALF_W, bay_hi + 0.02, hood_z), (b1 + 0.1 - HALF_W, bay_hi + 0.22, HALF_D), (b0 - 0.1 - HALF_W, bay_hi + 0.22, HALF_D)]))

# The roof, as the aerial and Earth's 3D show it: the main clay hip's ridge runs along the
# street (the game builds the reference house so, its door in the eave wall), a broad street slope
# carrying the array and a back slope, steep short hip ends at the sides; a two-storey
# front gable over the garage and bay, its wall dropping clean to the garage door; a
# two-storey back gable left of the middle. In the street frame here (front +z): the game's
# build frame for this house is a quarter turn of it (`export(..., frame="along")`).
MAIN_EAVE = (2.88, 3.88)  # the main hip's eaves, either way from the middle (x, z)
MAIN_EAVE_Y = 3.0577
RIDGE_HALF_X = 1.43  # the ridge along x from -1.43 to 1.43
front_slope = [(-MAIN_EAVE[0], MAIN_EAVE_Y, MAIN_EAVE[1]), (MAIN_EAVE[0], MAIN_EAVE_Y, MAIN_EAVE[1]), (RIDGE_HALF_X, RIDGE_Y, 0.0), (-RIDGE_HALF_X, RIDGE_Y, 0.0)]
back_slope = [(MAIN_EAVE[0], MAIN_EAVE_Y, -MAIN_EAVE[1]), (-MAIN_EAVE[0], MAIN_EAVE_Y, -MAIN_EAVE[1]), (-RIDGE_HALF_X, RIDGE_Y, 0.0), (RIDGE_HALF_X, RIDGE_Y, 0.0)]
right_end = [(MAIN_EAVE[0], MAIN_EAVE_Y, MAIN_EAVE[1]), (MAIN_EAVE[0], MAIN_EAVE_Y, -MAIN_EAVE[1]), (RIDGE_HALF_X, RIDGE_Y, 0.0)]
left_end = [(-MAIN_EAVE[0], MAIN_EAVE_Y, -MAIN_EAVE[1]), (-MAIN_EAVE[0], MAIN_EAVE_Y, MAIN_EAVE[1]), (-RIDGE_HALF_X, RIDGE_Y, 0.0)]


def street_gable(part, z_wall, outward, x0, x1, wall_x0, wall_x1, eave, pitch, depth_to):
    """A two-storey gable on the wall at z_wall (outward +1 front, -1 back): its stucco
    triangle over wall_x0..wall_x1, a clay vent in its peak and its roof's two slopes
    from its rake past the wall back to depth_to, where they run into the main roof."""
    xc = (x0 + x1) / 2
    half = (x1 - x0) / 2
    apex = eave + pitch * half
    y_at = lambda x: eave + pitch * (half - abs(x - xc)) - 0.12  # noqa: E731
    outline = [(wall_x0, WALL_TOP), (wall_x1, WALL_TOP), (wall_x1, max(WALL_TOP, y_at(wall_x1))), (xc, y_at(xc)), (wall_x0, max(WALL_TOP, y_at(wall_x0)))]
    face(f"{part}__Siding", [(x, y, z_wall) for x, y in outline], (0, 0, outward))
    face(f"{part}__Lining", [(x, y, z_wall - outward * WALL) for x, y in outline], (0, 0, -outward))
    clay_vent(part, xc, apex - 0.45, z_wall + outward * 0.0)
    rake = z_wall + outward * 0.36
    roof = "FrontRoof" if outward > 0 else "BackRoof"
    tiled(roof, Slope([(x0, eave, depth_to), (x0, eave, rake), (xc, apex, rake), (xc, apex, depth_to)]) if outward > 0 else Slope([(x0, eave, rake), (x0, eave, depth_to), (xc, apex, depth_to), (xc, apex, rake)]))
    tiled(roof, Slope([(x1, eave, rake), (x1, eave, depth_to), (xc, apex, depth_to), (xc, apex, rake)]) if outward > 0 else Slope([(x1, eave, depth_to), (x1, eave, rake), (xc, apex, rake), (xc, apex, depth_to)]))
    ridge_cap(roof, (xc, apex, rake), (xc, apex, depth_to))
    return apex


# The front gable over the garage and the bay.
GABLE = (-0.95, 2.85)
street_gable("FrontWall", HALF_D, 1, GABLE[0], GABLE[1], -0.62, HALF_W, 3.0, 0.5, 0.2)
# The back gable, left of the middle.
BACK_GABLE = (-2.25, 0.35)
street_gable("BackWall", -HALF_D, -1, BACK_GABLE[0], BACK_GABLE[1], -HALF_W + 0.05, 0.05, 3.0, 0.62, -0.3)

# The main hip: its street slope left of the front gable whole, behind the gable's wall
# under it; its back slope likewise round the back gable; the hip ends.
tiled("FrontRoof", Slope(clip(front_slope, 0, GABLE[0] + 0.15, False)))
tiled("FrontRoof", Slope(clip(clip(front_slope, 0, GABLE[0] + 0.15, True), 2, HALF_D - 0.05, False)))
tiled("BackRoof", Slope(clip(back_slope, 0, BACK_GABLE[1] - 0.15, True)))
tiled("BackRoof", Slope(clip(clip(back_slope, 0, BACK_GABLE[1] - 0.15, False), 2, -HALF_D + 0.05, True)))
tiled("RightRoof", Slope(right_end))
tiled("LeftRoof", Slope(left_end))
ridge_cap("FrontRoof", (-RIDGE_HALF_X, RIDGE_Y, 0.0), (RIDGE_HALF_X, RIDGE_Y, 0.0))
for corner, end in (((MAIN_EAVE[0], MAIN_EAVE[1]), RIDGE_HALF_X), ((MAIN_EAVE[0], -MAIN_EAVE[1]), RIDGE_HALF_X), ((-MAIN_EAVE[0], MAIN_EAVE[1]), -RIDGE_HALF_X), ((-MAIN_EAVE[0], -MAIN_EAVE[1]), -RIDGE_HALF_X)):
    ridge_cap("RightRoof" if corner[0] > 0 else "LeftRoof", (corner[0], MAIN_EAVE_Y, corner[1]), (end, RIDGE_Y, 0.0))

# The array: one block of panels on the street slope left of the front gable, between the
# ridge and the wing's roof, as the aerial has it.
street = Slope(front_slope)
for pu, pv in ((0.72, 1.62), (1.36, 1.62), (0.72, 2.24), (1.36, 2.24)):
    face("FrontRoof__Solar", [street.at(pu, pv, 0.08), street.at(pu + 0.6, pv, 0.08), street.at(pu + 0.6, pv + 0.58, 0.08), street.at(pu, pv + 0.58, 0.08)], street.N)

# The one-storey wing before the lower left and the porch beside it, under one clay hip
# whose left end runs out over the porch on dark posts over brick bases.
WING = (-2.2, -0.55, HALF_D, 5.15)
WING_TOP = 1.62
wing_front = Frame((WING[0], 0, WING[3]), (1, 0, 0), (0, 0, 1))
wing_left = Frame((WING[0], 0, HALF_D), (0, 0, 1), (-1, 0, 0))
wing_right = Frame((WING[1], 0, WING[3]), (0, 0, -1), (1, 0, 0))
wl = WING[1] - WING[0]
wd = WING[3] - WING[2]
wall("FrontWall", wing_front, 0, wl, 0, WING_TOP, [(0.42, 0.55, wl - 0.42, 1.38)])
window("FrontWall", wing_front, 0.42, 0.55, wl - 0.42, 1.38)
wall("FrontWall", wing_left, 0, wd, 0, WING_TOP, [(0.55, 0.12, 1.15, 1.35)])
wing_left.quad("FrontWall__Accent", 0.55, 0.12, 1.15, 1.35, 0.1)
wing_left.block("FrontWall__Trim", 0.5, 1.35, 1.2, 1.41, 0.0, -0.03)
wall("FrontWall", wing_right, 0, wd, 0, WING_TOP, [(0.55, 0.62, 0.85, 1.38)])
window("FrontWall", wing_right, 0.55, 0.62, 0.85, 1.38, mullion=False)
# porch floor, posts and bases, the beam under the eave
box("FrontWall__Concrete", (-3.3, 0.0, HALF_D), (WING[0], 0.1, WING[3] + 0.1))
for pz in (WING[3] - 0.12, HALF_D + 0.25):
    box("FrontWall__Brick", (-3.28, 0.1, pz - 0.17), (-2.94, 0.62, pz + 0.17))
    box("FrontWall__Concrete", (-3.31, 0.62, pz - 0.2), (-2.91, 0.68, pz + 0.2))
    box("FrontWall__Wood", (-3.18, 0.68, pz - 0.07), (-3.04, WING_TOP, pz + 0.07))
box("FrontWall__Wood", (-3.2, WING_TOP - 0.12, HALF_D), (-3.02, WING_TOP, WING[3] + 0.05))
wing_roof = hip_slopes(-3.45, WING[1] + 0.3, HALF_D - 0.6, WING[3] + 0.32, WING_TOP - 0.04, 0.5, along_x=True)
for name in ("front", "back", "left", "right"):
    polygon = clip_z(wing_roof[name], HALF_D)
    if len(polygon) >= 3:
        tiled("FrontWall", Slope(polygon), fascia=True)
ridge_cap("FrontWall", *wing_roof["ridge"])
for a, b in wing_roof["hips"]:
    if a[2] > HALF_D:
        ridge_cap("FrontWall", a, b)

# The sides and back, from the plan.
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

export(PLAN, frame="along")
