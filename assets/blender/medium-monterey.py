# The Senda medium Monterey, modelled after Neblina's corner house, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/medium-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/medium-monterey.glb and
#   crates/core/data/houses/medium-monterey.plan.json)
#
# The medium plan (the Prairie's, after Rumbo north lot 2) as Neblina's corner house wears it. Drawn in the
# street frame on a 5 x 7 m lot, its garage on the right; the game builds it with its door
# in the eave wall (its hip's ridge along the street). Stucco below a white belt course at
# the floor line and board and batten above it all round, white corner boards and window
# casings, burgundy board shutters on the upper pairs with small shed hoods on brackets
# over the upper windows and bracketed shelves under the pairs; a one-storey porch beside
# the garage, running past the left corner, on white columns under a flat-tile hip of its
# own; the garage's white two-leaf carriage door with a row of small windows across its
# top; a grey flat-tile hip. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The medium plan's openings, as the Prairie has them (the street's layout reads its door
# and garage): the door and garage, five upper windows (a shuttered pair over the porch, a
# small single and a shuttered pair over the garage); two windows down each flank and a
# pair across the back, lit upstairs.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.23, "Door"], [0.93, "Garage"]], "upper": [-1.48, -0.98, 0.31, 1.3, 1.8], "garage_bay": 0.62},
        "back": {"ground": [[-1.2, "Window"], [1.2, "Window"]], "upper": [-1.2, 1.2], "garage_bay": 0.0},
        "right": {"ground": [[-1.5, "Window"], [1.3, "Window"]], "upper": [-1.5, 1.3], "garage_bay": 0.0},
        "left": {"ground": [[-1.4, "Window"], [1.6, "Window"]], "upper": [-1.4, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][0][0]
garage_u, garage_bay = _front["ground"][1][0], _front["garage_bay"]

# The upper floor's dress: a belt course at the floor line, board and batten above it.
BELT = (1.98, 2.08)
BELT_OUT = 0.07
CLAD_OUT, BATTEN_OUT, BATTEN_SPACING = 0.015, 0.04, 0.48
CASING = 0.05
SHUTTER = 0.26
# What a window's casing and shutters cover of the cladding, top and bottom.
DRESSED = (UPPER_GLASS[0] - 0.04, UPPER_GLASS[1] + 0.06)


def slab(key, frame, u0, y0, u1, y1, out, base=0.0):
    """A board standing `out` proud of a surface at depth `base` in a wall's frame (its
    face by default): a box without the back it lies on."""
    d0, d1 = base, base - out
    for points, want in (
        ((frame.p(u0, y0, d1), frame.p(u1, y0, d1), frame.p(u1, y1, d1), frame.p(u0, y1, d1)), frame.out),
        ((frame.p(u0, y0, d0), frame.p(u0, y1, d0), frame.p(u0, y1, d1), frame.p(u0, y0, d1)), -frame.along),
        ((frame.p(u1, y0, d0), frame.p(u1, y1, d0), frame.p(u1, y1, d1), frame.p(u1, y0, d1)), frame.along),
        ((frame.p(u0, y1, d0), frame.p(u1, y1, d0), frame.p(u1, y1, d1), frame.p(u0, y1, d1)), (0, 1, 0)),
        ((frame.p(u0, y0, d0), frame.p(u1, y0, d0), frame.p(u1, y0, d1), frame.p(u0, y0, d1)), (0, -1, 0)),
    ):
        face(key, points, want)


def dress_upper_window(part, frame, u0, u1, shutters=False, shelf=False, hood=True):
    """A white casing round an upper window from u0 to u1, burgundy board shutters, a
    bracketed shelf under it and a small tiled shed hood on brackets over it; returns
    the span of cladding they stand in for."""
    y0, y1 = DRESSED
    for a, c, b, d in ((u0 - CASING, y0, u0, y1), (u1, y0, u1 + CASING, y1), (u0 - CASING, y1 - CASING, u1 + CASING, y1)):
        slab(f"{part}__Trim", frame, a, c, b, d, 0.035)
    lo, hi = u0 - CASING, u1 + CASING
    if shutters:
        for a, b in ((lo - SHUTTER, lo), (hi, hi + SHUTTER)):
            slab(f"{part}__Accent", frame, a, y0, b, y1, 0.035)
            for k in (1, 2):
                y = y0 + (y1 - y0) * k / 3
                slab(f"{part}__Accent", frame, a + 0.02, y - 0.02, b - 0.02, y + 0.02, 0.015, base=-0.035)
        lo, hi = lo - SHUTTER, hi + SHUTTER
    if shelf:
        slab(f"{part}__Trim", frame, u0 - 0.08, y0 - 0.06, u1 + 0.08, y0 - 0.02, 0.15)
        for u in (u0 + 0.06, u1 - 0.06):
            slab(f"{part}__Trim", frame, u - 0.025, BELT[1], u + 0.025, y0 - 0.06, 0.11)
    if hood:
        # a sloped plank roofed in the house's tile, white beneath and at its edges
        h0, h1, out, thick = lo - 0.04, hi + 0.04, 0.26, 0.04
        top = [frame.p(h0, y1 + 0.03, -out), frame.p(h1, y1 + 0.03, -out), frame.p(h1, y1 + 0.11, 0.0), frame.p(h0, y1 + 0.11, 0.0)]
        under = [p - Vector((0, thick, 0)) for p in top]
        face(f"{part}__Roof", top, (0, 1, 0))
        face(f"{part}__Trim", under, (0, -1, 0))
        face(f"{part}__Trim", [under[0], under[1], top[1], top[0]], frame.out)
        face(f"{part}__Trim", [under[0], top[0], top[3], under[3]], -frame.along)
        face(f"{part}__Trim", [under[1], top[1], top[2], under[2]], frame.along)
        for u in (lo + 0.05, hi - 0.05):
            slab(f"{part}__Trim", frame, u - 0.025, y1 - 0.06, u + 0.025, y1 + 0.02, 0.2)
    return lo, hi


def clad_upper(part, frame, u_lo, u_hi, cuts, y_top=WALL_TOP):
    """Board and batten from the belt course to the eave over u_lo..u_hi, proud of the
    stucco, round the windows' `cuts` (u0, y0, u1, y1): boards, then battens on them
    (none in a scrap too short to read)."""
    us = sorted({u_lo, u_hi, *[c[0] for c in cuts], *[c[2] for c in cuts]})
    ys = sorted({BELT[1], y_top, *[c[1] for c in cuts], *[c[3] for c in cuts]})
    for a, b in zip(us, us[1:]):
        run = None
        for c, d in zip(ys, ys[1:]):
            mid = ((a + b) / 2, (c + d) / 2)
            covered = any(k[0] < mid[0] < k[2] and k[1] < mid[1] < k[3] for k in cuts)
            if covered or run is None:
                if run is not None:
                    slab(f"{part}__Cladding", frame, a, run[0], b, run[1], CLAD_OUT)
                run = None if covered else (c, d)
            else:
                run = (run[0], d)
        if run is not None:
            slab(f"{part}__Cladding", frame, a, run[0], b, run[1], CLAD_OUT)
    u = u_lo + BATTEN_SPACING / 2
    while u < u_hi - 0.05:
        pieces = [(BELT[1], y_top)]
        for c0, c1, c2, c3 in cuts:
            if c0 - 0.02 < u < c2 + 0.02:
                pieces = [(BELT[1], c1), (c3, y_top)]
        for p0, p1 in pieces:
            if p1 - p0 < 0.16:
                continue
            # the batten's face and sides; its back lies on the boards, its ends under trim
            for quad, want in (
                ((frame.p(u - 0.015, p0, -BATTEN_OUT), frame.p(u + 0.015, p0, -BATTEN_OUT), frame.p(u + 0.015, p1, -BATTEN_OUT), frame.p(u - 0.015, p1, -BATTEN_OUT)), frame.out),
                ((frame.p(u - 0.015, p0, -CLAD_OUT), frame.p(u - 0.015, p0, -BATTEN_OUT), frame.p(u - 0.015, p1, -BATTEN_OUT), frame.p(u - 0.015, p1, -CLAD_OUT)), -frame.along),
                ((frame.p(u + 0.015, p0, -CLAD_OUT), frame.p(u + 0.015, p0, -BATTEN_OUT), frame.p(u + 0.015, p1, -BATTEN_OUT), frame.p(u + 0.015, p1, -CLAD_OUT)), frame.along),
            ):
                face(f"{part}__Cladding", quad, want)
        u += BATTEN_SPACING


# ---------------------------------------------------------------- the front wall
garage = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
DOOR = (door_u - 0.3, 0.12, door_u + 0.3, 1.45)
UPPER = [(-1.8, -0.66), (0.14, 0.48), (0.98, 2.12)]  # the pair, the single, the pair
holes = [(fx(garage[0]), garage[1], fx(garage[2]), garage[3]), (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3])]
holes += [(fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1]) for a, b in UPPER]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)
front.quad("FrontWall__Accent", fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3], 0.1)
slab("FrontWall__Trim", front, fx(DOOR[0] - 0.06), DOOR[3], fx(DOOR[2] + 0.06), DOOR[3] + 0.06, 0.03)
spans = []
for a, b in UPPER:
    pair = b - a > 0.6
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=pair)
    spans.append(dress_upper_window("FrontWall", front, fx(a), fx(b), shutters=pair, shelf=pair))

# The white two-leaf carriage door: framed leaves set back in the opening, a row of small
# windows across the top, black strap hinges and handles.
g0, g1 = fx(garage[0]), fx(garage[2])
gy0, gy1 = garage[1], garage[3]
front.quad("FrontWall__GarageDoor", g0, gy0, g1, gy1, 0.12)
mid = (g0 + g1) / 2
front.block("FrontWall__Dark", mid - 0.012, gy0, mid + 0.012, gy1, 0.12, 0.112)
LITES = (gy1 - 0.34, gy1 - 0.1)
for l0, l1 in ((g0, mid - 0.012), (mid + 0.012, g1)):
    rail = 0.07
    for a, c, b, d in ((l0, gy0, l1, gy0 + rail), (l0, LITES[0] - rail, l1, LITES[0]), (l0, LITES[1], l1, gy1), (l0, gy0, l0 + rail, gy1), (l1 - rail, gy0, l1, gy1)):
        slab("FrontWall__GarageDoor", front, a, c, b, d, 0.025, base=0.12)
    # an X-less carriage panel: a middle stile down the lower panel
    m = (l0 + l1) / 2
    slab("FrontWall__GarageDoor", front, m - 0.025, gy0 + rail, m + 0.025, LITES[0] - rail, 0.015, base=0.12)
    lites = 4
    width = (l1 - l0 - 2 * rail) / lites
    for k in range(lites):
        a = l0 + rail + k * width
        front.quad("FrontWall__Glass", a + 0.02, LITES[0] + 0.02, a + width - 0.02, LITES[1] - 0.02, 0.115)
    for y in (gy0 + 0.22, LITES[0] - 0.2):
        hinge = (l0 + 0.04, l0 + 0.32) if l0 == g0 else (l1 - 0.32, l1 - 0.04)
        slab("FrontWall__Iron", front, hinge[0], y - 0.02, hinge[1], y + 0.02, 0.01, base=0.095)
for handle in (mid - 0.12, mid + 0.12):
    front.block("FrontWall__Iron", handle - 0.012, 0.62, handle + 0.012, 0.86, 0.095, 0.075)
for a, c, b, d in ((g0 - 0.07, gy1, g1 + 0.07, gy1 + 0.07), (g0 - 0.07, 0.0, g0, gy1), (g1, 0.0, g1 + 0.07, gy1)):
    slab("FrontWall__Trim", front, a, c, b, d, 0.03)
# A coach lantern beside the garage.
for x in (garage[0] - 0.18,):
    front.block("FrontWall__Iron", fx(x) - 0.06, 1.0, fx(x) + 0.06, 1.28, 0.0, -0.11)
    front.quad("FrontWall__Glass", fx(x) - 0.045, 1.04, fx(x) + 0.045, 1.22, -0.115)

# ---------------------------------------------------------------- the porch
# One storey beside the garage, running past the left corner, on white columns under a
# flat-tile hip of its own.
PORCH = (-3.1, garage[0] - 0.15, HALF_D, HALF_D + 1.45)
PORCH_TOP = 1.62
COLUMN_Z = PORCH[3] - 0.18
box("FrontWall__Concrete", (PORCH[0], 0.0, PORCH[2]), (PORCH[1], 0.1, PORCH[3]))


def column(x, z):
    """A white porch column: a plinth on the floor, the shaft, a capital under the beam."""
    box("FrontWall__Trim", (x - 0.13, 0.1, z - 0.13), (x + 0.13, 0.22, z + 0.13), skip=("-y",))
    box("FrontWall__Trim", (x - 0.08, 0.22, z - 0.08), (x + 0.08, PORCH_TOP - 0.24, z + 0.08), skip=("-y", "+y"))
    box("FrontWall__Trim", (x - 0.12, PORCH_TOP - 0.24, z - 0.12), (x + 0.12, PORCH_TOP - 0.16, z + 0.12), skip=("+y",))


COLUMNS_X = (PORCH[0] + 0.18, -1.85, PORCH[1] - 0.2)
for x in COLUMNS_X:
    column(x, COLUMN_Z)
column(COLUMNS_X[0], HALF_D + 0.2)
box("FrontWall__Trim", (PORCH[0] + 0.08, PORCH_TOP - 0.16, COLUMN_Z - 0.09), (PORCH[1] - 0.1, PORCH_TOP, COLUMN_Z + 0.09))
box("FrontWall__Trim", (COLUMNS_X[0] - 0.09, PORCH_TOP - 0.16, HALF_D + 0.1), (COLUMNS_X[0] + 0.09, PORCH_TOP, COLUMN_Z - 0.09))
porch_roof = hip_slopes(PORCH[0] - 0.15, PORCH[1] + 0.2, HALF_D - 0.6, PORCH[3] + 0.3, PORCH_TOP - 0.04, 0.42, along_x=True)
for name in ("front", "back", "left", "right"):
    polygon = clip_z(porch_roof[name], HALF_D)
    if len(polygon) >= 3:
        tiled("FrontWall", Slope(polygon), flat=True)
ridge_cap("FrontWall", *porch_roof["ridge"])
for a, b in porch_roof["hips"]:
    if a[2] > HALF_D:
        ridge_cap("FrontWall", a, b)

# ---------------------------------------------------------------- the upper floor all round
for side in ("front", "back", "right", "left"):
    frame, part, half = WALL_FRAMES[side], WALL_PARTS[side], WALL_HALF[side]
    if side == "front":
        # the dressed windows' spans, in WALL_FRAMES' u from the middle
        cuts = [(a - fx(0), DRESSED[0], b - fx(0), DRESSED[1]) for a, b in spans]
    else:
        cuts = [(u - WINDOW_HALF, UPPER_GLASS[0], u + WINDOW_HALF, UPPER_GLASS[1]) for u in PLAN["walls"][side]["upper"]]
    clad_upper(part, frame, -half, half, cuts)
    slab(f"{part}__Trim", frame, -half - BELT_OUT, BELT[0], half + BELT_OUT, BELT[1], BELT_OUT)
# White corner boards over the upper floor.
for cx in (-1, 1):
    for cz in (-1, 1):
        x, z = cx * HALF_W, cz * HALF_D
        key = "FrontWall__Trim" if cz > 0 else "BackWall__Trim"
        box(key, (min(x, x + cx * 0.05) - (0.1 if cx > 0 else 0), BELT[1], min(z, z + cz * 0.05) - (0.1 if cz > 0 else 0)),
            (max(x, x + cx * 0.05) + (0.1 if cx < 0 else 0), WALL_TOP - 0.06, max(z, z + cz * 0.05) + (0.1 if cz < 0 else 0)))

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

export(PLAN, frame="along")
