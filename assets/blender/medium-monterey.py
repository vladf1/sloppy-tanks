# The Senda medium Monterey, modelled after Neblina's corner house, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/medium-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/medium-monterey.glb and
#   crates/core/data/houses/medium-monterey.plan.json)
#
# The medium plan (the Prairie's, after Rumbo north lot 2) as Neblina's corner house wears it. Drawn in the
# street frame on the 6.2 x 8 m lot, its garage on the right; the game builds it with its
# door in the eave wall (its hip's ridge along the street). Stucco below a white belt course
# at the floor line and board and batten above it all round, white corner boards and window
# casings, burgundy board shutters on the upper pair over the office, small shed hoods
# over the upper windows; the office beside
# the garage under a flat-tile lean-to and the entry passage beside it under a small gable
# on posts (the kit's office_entry); the garage's one-piece sectional door with a row of
# small lites across its top; a grey flat-tile hip. The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The medium plan's openings (the street's layout reads its door and garage): the
# coded window (the entry passage's door stands over it) and the door behind the office,
# the garage's two leaves, and the spread front's five upper windows (a shuttered pair
# over the office, a small single over the garage's inner leaf
# and a pair over its outer one); two windows a floor down each flank and three across
# the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.96, -0.36, 0.7, 1.7, 2.5], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.3, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.3, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-1.9, "Window"], [2.0, "Window"]], "upper": [-1.9, 2.0], "garage_bay": 0.0},
        "left": {"ground": [[-2.3, "Window"], [1.6, "Window"]], "upper": [-2.3, 1.6], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# The upper floor's dress: a belt course at the floor line, board and batten above it (a
# real 15 cm belt, battens every half metre), casings, shutters.
BELT = (FLOOR - 0.06, FLOOR + 0.02)
BELT_OUT = 0.04
CLAD_OUT, BATTEN_OUT, BATTEN_SPACING = 0.01, 0.025, 0.3
CASING = 0.035
SHUTTER = SHUTTER_WIDTH
# What a window's casing and shutters cover of the cladding, top and bottom.
DRESSED = (UPPER_GLASS[0] - 0.03, UPPER_GLASS[1] + 0.04)


def slab(key, frame, u0, y0, u1, y1, out, base=0.0, ends=True):
    """A board standing `out` proud of a surface at depth `base` in a wall's frame (its
    face by default): a box without the back it lies on (nor its top and bottom unless
    `ends`)."""
    d0, d1 = base, base - out
    sides = (
        ((frame.p(u0, y0, d1), frame.p(u1, y0, d1), frame.p(u1, y1, d1), frame.p(u0, y1, d1)), frame.out),
        ((frame.p(u0, y0, d0), frame.p(u0, y1, d0), frame.p(u0, y1, d1), frame.p(u0, y0, d1)), -frame.along),
        ((frame.p(u1, y0, d0), frame.p(u1, y1, d0), frame.p(u1, y1, d1), frame.p(u1, y0, d1)), frame.along),
        ((frame.p(u0, y1, d0), frame.p(u1, y1, d0), frame.p(u1, y1, d1), frame.p(u0, y1, d1)), (0, 1, 0)),
        ((frame.p(u0, y0, d0), frame.p(u1, y0, d0), frame.p(u1, y0, d1), frame.p(u0, y0, d1)), (0, -1, 0)),
    )
    for points, want in sides if ends else sides[:3]:
        face(key, points, want)


def dress_upper_window(part, frame, u0, u1, shutters=False, shelf=False, hood=True):
    """A white casing round an upper window from u0 to u1, burgundy board shutters, a
    bracketed shelf under it and a small tiled shed hood over it; returns
    the span of cladding they stand in for."""
    y0, y1 = DRESSED
    for a, c, b, d in ((u0 - CASING, y0, u0, y1), (u1, y0, u1 + CASING, y1), (u0 - CASING, y1 - CASING, u1 + CASING, y1)):
        slab(f"{part}__Trim", frame, a, c, b, d, 0.025, ends=False)
    lo, hi = u0 - CASING, u1 + CASING
    if shutters:
        for a, b in ((lo - SHUTTER, lo), (hi, hi + SHUTTER)):
            slab(f"{part}__Accent", frame, a, y0, b, y1, 0.025, ends=False)
            for k in (1, 2):
                y = y0 + (y1 - y0) * k / 3
                slab(f"{part}__Accent", frame, a + 0.015, y - 0.012, b - 0.015, y + 0.012, 0.01, base=-0.025)
        lo, hi = lo - SHUTTER, hi + SHUTTER
    if shelf:
        slab(f"{part}__Trim", frame, u0 - 0.05, y0 - 0.045, u1 + 0.05, y0 - 0.015, 0.09)
        for u in (u0 + 0.05, u1 - 0.05):
            slab(f"{part}__Trim", frame, u - 0.018, BELT[1], u + 0.018, y0 - 0.045, 0.07)
    if hood:
        # a sloped plank roofed in the house's tile, white beneath and at its edges
        h0, h1, out, thick = lo - 0.03, hi + 0.03, 0.16, 0.03
        top = [frame.p(h0, y1 + 0.02, -out), frame.p(h1, y1 + 0.02, -out), frame.p(h1, y1 + 0.08, 0.0), frame.p(h0, y1 + 0.08, 0.0)]
        under = [p - Vector((0, thick, 0)) for p in top]
        face(f"{part}__Roof", top, (0, 1, 0))
        face(f"{part}__Trim", under, (0, -1, 0))
        face(f"{part}__Trim", [under[0], under[1], top[1], top[0]], frame.out)
        face(f"{part}__Trim", [under[0], top[0], top[3], under[3]], -frame.along)
        face(f"{part}__Trim", [under[1], top[1], top[2], under[2]], frame.along)
    return lo, hi


def clad_upper(part, frame, u_lo, u_hi, cuts, y_top=WALL_TOP):
    """Board and batten from the belt course to the eave over u_lo..u_hi, proud of the
    stucco, round the windows' `cuts` (u0, y0, u1, y1): boards (their face: a centimetre
    proud, their edges do not read), then battens on them (none in a scrap too short to
    read)."""
    us = sorted({u_lo, u_hi, *[c[0] for c in cuts], *[c[2] for c in cuts]})
    ys = sorted({BELT[1], y_top, *[c[1] for c in cuts], *[c[3] for c in cuts]})
    for a, b in zip(us, us[1:]):
        run = None
        for c, d in zip(ys, ys[1:]):
            mid = ((a + b) / 2, (c + d) / 2)
            covered = any(k[0] < mid[0] < k[2] and k[1] < mid[1] < k[3] for k in cuts)
            if covered or run is None:
                if run is not None:
                    frame.quad(f"{part}__Cladding", a, run[0], b, run[1], -CLAD_OUT)
                run = None if covered else (c, d)
            else:
                run = (run[0], d)
        if run is not None:
            frame.quad(f"{part}__Cladding", a, run[0], b, run[1], -CLAD_OUT)
    u = u_lo + BATTEN_SPACING / 2
    while u < u_hi - 0.05:
        pieces = [(BELT[1], y_top)]
        for c0, c1, c2, c3 in cuts:
            if c0 - 0.02 < u < c2 + 0.02:
                pieces = [(BELT[1], c1), (c3, y_top)]
        for p0, p1 in pieces:
            if p1 - p0 < 0.12:
                continue
            # the batten's face: a real batten is 2.5 cm proud, its sides do not read
            frame.quad(f"{part}__Cladding", u - 0.012, p0, u + 0.012, p1, -BATTEN_OUT)
        u += BATTEN_SPACING


# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
HALF = {-0.96: 0.29, -0.36: 0.26, 0.7: 0.24, 1.7: 0.28, 2.5: 0.28}
UPPER = [(u - HALF[u], u + HALF[u]) for u in _front["upper"]]
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in UPPER]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
# the door into the house behind the office (the passage's door, at its end, is the entry's)
front_door(front, door_u, glass=True)
spans = []
# The pairs ganged under one casing, the stucco between them cased as a mullion; the pair
# over the office shuttered.
for (a, b), (c, d), shutters in ((UPPER[0], UPPER[1], True), (UPPER[3], UPPER[4], False)):
    for u0, u1 in ((a, b), (c, d)):
        window("FrontWall", front, fx(u0), UPPER_GLASS[0], fx(u1), UPPER_GLASS[1], mullion=False)
    spans.append(dress_upper_window("FrontWall", front, fx(a), fx(d), shutters=shutters))
    slab("FrontWall__Trim", front, fx(b), DRESSED[0], fx(c), DRESSED[1], 0.025, ends=False)
a, b = UPPER[2]
window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=False)
spans.append(dress_upper_window("FrontWall", front, fx(a), fx(b)))

# The garage's one-piece sectional door in boards, a row of small lites across its top.
sectional_garage_door(front, garage_u, decor="lites", boards=True)
lantern(front, (GARAGE[2] + HALF_W) / 2 + 0.04)

# ---------------------------------------------------------------- the office and entry
# The office beside the garage under a flat-tile lean-to, its street window shuttered; the
# entry passage up its outer side to the front door under a small gable on dark posts.
office_entry(office_roof="shed", flat=True, cover="gable", supports="posts", window_style="shutters")

# ---------------------------------------------------------------- the upper floor all round
SIDE_HALF = 0.27
for side in ("front", "back", "right", "left"):
    frame, part, half = WALL_FRAMES[side], WALL_PARTS[side], WALL_HALF[side]
    if side == "front":
        # the dressed windows' spans, in WALL_FRAMES' u from the middle
        cuts = [(a - fx(0), DRESSED[0], b - fx(0), DRESSED[1]) for a, b in spans]
    else:
        cuts = [(u - SIDE_HALF - 0.01, UPPER_GLASS[0] - 0.01, u + SIDE_HALF + 0.01, UPPER_GLASS[1] + 0.01) for u in PLAN["walls"][side]["upper"]]
    clad_upper(part, frame, -half, half, cuts)
    slab(f"{part}__Trim", frame, -half - BELT_OUT, BELT[0], half + BELT_OUT, BELT[1], BELT_OUT)
# White corner boards over the upper floor (real 0.15 m).
CORNER = 0.08
for cx in (-1, 1):
    for cz in (-1, 1):
        x, z = cx * HALF_W, cz * HALF_D
        key = "FrontWall__Trim" if cz > 0 else "BackWall__Trim"
        xs = sorted((x - cx * CORNER, x + cx * 0.035))
        zs = sorted((z - cz * CORNER, z + cz * 0.035))
        box(key, (xs[0], BELT[1], zs[0]), (xs[1], WALL_TOP - 0.05, zs[1]))

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
plain_wall("back", PLAN, half_width=0.3, upper_half=SIDE_HALF)
for side in ("right", "left"):
    plain_wall(side, PLAN, half_width=SIDE_HALF)

# The array: a block on the back slope.
solar_array("BackRoof", Slope(along_hip()["back"]), 4, 2)

export(PLAN, frame="along")
