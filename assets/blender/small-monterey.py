# The reference house (Rumbo north lot 4), modelled for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/small-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:small-monterey writes public/models/houses/small-monterey.glb and its plan,
#   crates/core/data/houses/small-monterey.plan.json)
#
# The house is authored in the game's build frame for its lot, which is the street frame
# here (metres: front +z, the garage side +x, y up; walls x +-3.1, z +-4.0, wall top 3.2,
# ridge 4.5): the tract's 11.7 x 15 m two-storey box at the level's scale. Every object is
# named <HousePart>__<Surface> after the coded house's pieces (crates/core/src/sim/
# house_layout.rs HousePart, building_kit.rs Surface) so the structural house cuts it the
# way it cuts the coded meshes: walls by side with an inner lining, the four hip slopes,
# the gables' walls in their walls' pieces and their roofs in the hip ends'.
#
# The openings are declared once, in PLAN, in the game's wall-plan form (each wall's
# ground-floor openings by centre along the wall and kind, the upper windows over them):
# the model cuts its windows there and the script writes PLAN for the game, whose
# structure frames its window, door and garage parts from it (HouseDesign::wall_plan).
# That file is gameplay data: it shapes the house's colliders.
#
# Its office and entry stand before the front as the front wall's, so they break with
# it; the loggia behind it is a cover of its own in the street's layout, not this model's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded plan, which the street's layout reads (the
# path and mailbox find the door): the window and the door in the entry's half (the
# passage's front door stands for the window, the office's back for the door), the
# garage's two leaves, the window over the door and the box bay's ganged pair over the
# garage, and the model's own: the second small window over the office and the
# shuttered one on the gable. Two windows a floor down each flank, three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.66, 1.0, 1.8, -1.91, 0.2], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.4, "Window"], [0.55, "Window"], [2.05, "Window"]], "upper": [-1.4, 0.55, 2.05], "garage_bay": 0.0},
        "right": {"ground": [[-2.3, "Window"], [1.9, "Window"]], "upper": [-2.3, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-2.0, "Window"], [1.7, "Window"]], "upper": [-2.0, 1.7], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
garage_u = _front["ground"][2][0]
GABLE_X = garage_u
gable_left = GABLE_X - (HALF_W - GABLE_X)  # the garage gable's wall, from here to the corner

# ---------------------------------------------------------------- the garage gable's face
# Over 60% of the front: the sectional garage door under a dark header, a lantern and the
# house number beside it; upstairs one window with one teal shutter on its left, and the
# dark box bay of two tall windows over a board-and-batten apron on three corbels.
GARAGE = garage_opening(garage_u)
BAY = (garage_u - 0.78, garage_u + 0.78)
BAY_HOLE = (BAY[0] + 0.05, UPPER_GLASS[0] - 0.08, BAY[1] - 0.05, BAY_HI - 0.05)
SINGLE = (0.2 - 0.21, 0.2 + 0.21)
holes = [GARAGE, BAY_HOLE, (SINGLE[0], UPPER_GLASS[0], SINGLE[1], UPPER_GLASS[1])]
wall("FrontWall", front, fx(gable_left), 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
window("FrontWall", front, fx(SINGLE[0]), UPPER_GLASS[0], fx(SINGLE[1]), UPPER_GLASS[1], mullion=False)
front.block("FrontWall__Trim", fx(SINGLE[0]), 2.45, fx(SINGLE[1]), 2.48, 0.1, 0.07)
front.block("FrontWall__Wood", fx(SINGLE[0]) - 0.08, UPPER_GLASS[1] + 0.05, fx(SINGLE[1]) + 0.08, UPPER_GLASS[1] + 0.11, 0.0, -0.025)
shutter("FrontWall", front, fx(SINGLE[0]) - SHUTTER_WIDTH - 0.02, UPPER_GLASS[0] - 0.01, fx(SINGLE[0]) - 0.02, UPPER_GLASS[1] + 0.01)
sectional_garage_door(front, garage_u, decor="handles")
lantern(front, GARAGE[2] + 0.17)
front.block("FrontWall__Trim", fx(GARAGE[2]) + 0.11, 0.78, fx(GARAGE[2]) + 0.23, 0.86, 0.0, -0.02)
box_bay(front, BAY[0], BAY[1], flat=False)

# ---------------------------------------------------------------- the entry's half
# The upper storey over the office stands back from the gable's face under the main
# roof's eave, two small square windows in it; the office projects from it beside the
# gable under a lean-to.
# The office's lean-to runs on over an open porch bay beside it to one dark post; straight
# steps rise to the bay between two stone piers, a low stone planter curving before the
# office to the third, and the way in runs back past the office's slit windows and round
# the front corner to the door in the side wall.
SET_BACK = 0.45
back_z = HALF_D - SET_BACK
upper_wall = Frame((-HALF_W, 0, back_z), (1, 0, 0), (0, 0, 1))
SMALL = [(-1.91 - 0.21, -1.91 + 0.21), (-0.66 - 0.21, -0.66 + 0.21)]
SMALL_Y = (2.3, 2.72)
holes = [(a, SMALL_Y[0], b, SMALL_Y[1]) for a, b in SMALL]
wall("FrontWall", upper_wall, 0, fx(gable_left), 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
for a, b in SMALL:
    window("FrontWall", upper_wall, fx(a), SMALL_Y[0], fx(b), SMALL_Y[1], mullion=False)
    upper_wall.block("FrontWall__Wood", fx(a) - 0.06, SMALL_Y[1] + 0.05, fx(b) + 0.06, SMALL_Y[1] + 0.1, 0.0, -0.025)
# the wall up under the main roof's hip end over it, and the gable's return to it
hip_y = RIDGE_Y - PITCH * (back_z - RIDGE_HALF) - DECK * 1.1
hip_x = -EAVE_X + (EAVE_Z - back_z) / (EAVE_Z - RIDGE_HALF) * EAVE_X
face("FrontWall__Siding", [(-HALF_W, WALL_TOP, back_z), (gable_left, WALL_TOP, back_z), (gable_left, hip_y, back_z), (hip_x, hip_y, back_z)], (0, 0, 1))
face("FrontWall__Siding", [(gable_left, 0, back_z), (gable_left, 0, HALF_D), (gable_left, WALL_TOP, HALF_D), (gable_left, WALL_TOP, back_z)], (-1, 0, 0))
face("FrontWall__Lining", [(gable_left + WALL, 0, back_z), (gable_left + WALL, 0, HALF_D), (gable_left + WALL, WALL_TOP, HALF_D), (gable_left + WALL, WALL_TOP, back_z)], (1, 0, 0))
office_entry(wall_z=back_z, office_roof="shed", window_style="header", popout=True, slits=2,
             inner_slit=True, steps_piers=True, planter=True, top=2.2, bay=True)
# "The home entrance is off the side": past the bay, round the front corner, a door in the
# side wall under its own little shed.
SIDE_DOOR = 2.9
side_entrance(SIDE_DOOR, z_walk=(SIDE_DOOR - 0.6, back_z + 0.6))

# ---------------------------------------------------------------- the roof
# A clay hip whose ridge runs front to back, as the 3D views show it from the side; a
# two-storey gable over the garage, its wall dropping clean to the garage door, and one
# over the back above the family room, both on the garage's side, a small lattice vent
# near each one's peak.
hip_with_gables(GABLE_X, front=True, back=True)
peak = gable_end_wall("FrontWall", HALF_D, gable_left, HALF_W, GABLE_X, 1)
lattice_vent("FrontWall", front, fx(GABLE_X), peak - 0.32)
peak = gable_end_wall("BackWall", -HALF_D, gable_left, HALF_W, GABLE_X, -1)
lattice_vent("BackWall", WALL_FRAMES["back"], -GABLE_X, peak - 0.32)

# The array: one block on the left slope, toward the front, as the aerial has it.
solar_array("LeftRoof", Slope(main_hip()["left"]), 3, 2, up=0.22, across=0.6)

# ---------------------------------------------------------------- the sides and back
plain_wall("back", PLAN, half_width={-1.4: 0.42, 0.55: 0.42, 2.05: 0.3}, upper_half=0.3)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27, span=(-HALF_D, back_z), extra_holes=[side_entrance_hole(SIDE_DOOR)])

export(PLAN)
