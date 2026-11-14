# The Senda medium Monterey, modelled after Neblina's corner house, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/medium-monterey.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes assets/models/houses/medium-monterey.glb and
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
# small lites across its top; a grey flat-tile hip whose ridge runs front to back, as the
# drone saw it (the game's structure keeps its coded hip along the street, see the roof
# below), a cross gable over the garage flank and the arrays on the other flank's slope.
# The paints are the style's.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The medium plan's openings (the street's layout reads its door and garage): the
# coded window (the entry passage's door stands over it) and the door behind the office,
# the garage's two leaves, and the spread front's five upper windows (a shuttered pair
# over the office, a small single over the garage's inner leaf
# and a pair over its outer one); two windows a floor down each flank, an upper window over
# each (the left flank draws two more, see left_wall), and three across the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-0.96, -0.36, 0.7, 1.7, 2.5], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.3, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.3, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-1.9, "Window"], [2.0, "Window"]], "upper": [-1.9, 2.0], "garage_bay": 0.0},
        "left": {"ground": [[-3.2, "Window"], [1.6, "Window"]], "upper": [-3.2, 1.3], "garage_bay": 0.0},
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
# The flank the arrays face (the street's left) has, from the back: a pair over a small
# window a floor down, then a wide pair, a single and a small single across the upper
# floor, as the drone sees it. The structure frames an upper window only over a ground
# opening's column, so the wide pair and the small single toward the front, over plain
# stucco, are drawn but not planned: the structure keeps a panel behind them.
LEFT_GROUND_HALF = {-3.2: 0.24, 1.6: SIDE_HALF}
LEFT_UPPER_HALF = {-3.2: 0.45, -0.6: 0.55, 1.3: 0.27, 3.3: 0.24}
LEFT_DRAWN_UPPER = PLAN["walls"]["left"]["upper"] + [-0.6, 3.3]
# the back's upper windows: twin windows over the two wider ground ones
BACK_UPPER_HALF = {-1.6: 0.42, 0.3: 0.42, 2.0: SIDE_HALF}
for side in ("front", "back", "right", "left"):
    frame, part, half = WALL_FRAMES[side], WALL_PARTS[side], WALL_HALF[side]
    if side == "front":
        # the dressed windows' spans, in WALL_FRAMES' u from the middle
        cuts = [(a - fx(0), DRESSED[0], b - fx(0), DRESSED[1]) for a, b in spans]
    else:
        halves = {"left": LEFT_UPPER_HALF, "back": BACK_UPPER_HALF}.get(side, {})
        cuts = [(u - halves.get(u, SIDE_HALF) - 0.01, UPPER_GLASS[0] - 0.01, u + halves.get(u, SIDE_HALF) + 0.01, UPPER_GLASS[1] + 0.01) for u in (LEFT_DRAWN_UPPER if side == "left" else PLAN["walls"][side]["upper"])]
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

# ---------------------------------------------------------------- the roof
# Seen from the drone the corner house's hip runs front to back, not along the street:
# long slopes over the flanks, hip ends over the front and back, a cross gable on the
# garage flank's slope and the arrays on the other flank's. The game's structure keeps the
# coded hip along the street (house_layout.rs), so every piece drawn here is cut to the
# coded slope it lies over in plan and named after it, and falls with that slope.
DRAWN_RIDGE_HALF = ALONG_RIDGE_HALF  # the drawn ridge's half length, front to back
SIDE_EAVE_Y = EAVE_Y  # the flank slopes at the house's pitch reach RIDGE_Y at x = 0


def _coded_regions():
    """The coded slopes' plans (x, z), as along_hip lays them, reaching past the eaves so
    fascias and caps on the outline still fall inside one."""
    reach = 1.6
    rh, ex, ez = ALONG_RIDGE_HALF, ALONG_EAVE[0], ALONG_EAVE[1]

    def out(sx, sz):
        return (sx * (rh + (ex - rh) * reach), sz * ez * reach)

    return {
        "Front": [out(-1, 1), out(1, 1), (rh, 0.0), (-rh, 0.0)],
        "Back": [out(1, -1), out(-1, -1), (-rh, 0.0), (rh, 0.0)],
        "Right": [out(1, 1), out(1, -1), (rh, 0.0)],
        "Left": [out(-1, -1), out(-1, 1), (-rh, 0.0)],
    }


CODED_REGIONS = _coded_regions()


def _clip_to_region(points, region):
    """A convex 3D polygon cut by the vertical half-spaces of a convex plan region."""
    cx = sum(p[0] for p in region) / len(region)
    cz = sum(p[1] for p in region) / len(region)
    poly = [Vector(p) for p in points]
    n = len(region)
    for i in range(n):
        (ax, az), (bx, bz) = region[i], region[(i + 1) % n]

        def side(p):
            return (bx - ax) * (p[2] - az) - (bz - az) * (p[0] - ax)

        sign = 1 if side((cx, 0, cz)) > 0 else -1
        kept = []
        for k in range(len(poly)):
            a, b = poly[k], poly[(k + 1) % len(poly)]
            da, db = sign * side(a), sign * side(b)
            if da >= -1e-9:
                kept.append(a)
            if (da > 1e-9 and db < -1e-9) or (da < -1e-9 and db > 1e-9):
                kept.append(a.lerp(b, da / (da - db)))
        poly = kept
        if len(poly) < 3:
            return []
    cleaned = []
    for p in poly:
        if not cleaned or (p - cleaned[-1]).length > 1e-6:
            cleaned.append(p)
    if len(cleaned) > 1 and (cleaned[0] - cleaned[-1]).length < 1e-6:
        cleaned.pop()
    return cleaned


def _newell(points):
    normal = Vector()
    for k in range(len(points)):
        a, b = points[k], points[(k + 1) % len(points)]
        normal += Vector(((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y)))
    return normal


def on_coded_slopes(surface, points, want):
    """A roof face cut to the coded slopes under it, each piece `<Side>Roof__<surface>`."""
    for side, region in CODED_REGIONS.items():
        piece = _clip_to_region(points, region)
        if len(piece) < 3:
            continue
        normal = _newell(piece)
        if normal.length < 1e-7:
            continue
        if normal.dot(Vector(want)) < 0:
            piece.reverse()
        face(f"{side}Roof__{surface}", piece)


def tiled_on_coded(points, eave_edges=(0,)):
    """A flat concrete tile slope (the kit's `tiled`, flat) cut to the coded slopes: its
    deck, underside, fascias on its eave edges and the courses' lips. The ribs between
    tiles, a centimetre high, are left out: they do not read from the game's camera and
    every one would be cut in two at a coded hip."""
    slope = Slope(points)
    on_coded_slopes("Roof", [slope.at(u, v, 0) for u, v in slope.uv], slope.N)
    on_coded_slopes("Siding", [slope.at(u, v, -DECK) for u, v in slope.uv], -slope.N)
    n = len(slope.uv)
    centroid = sum(slope.points, Vector()) / n
    for i in eave_edges:
        (ua, va), (ub, vb) = slope.uv[i], slope.uv[(i + 1) % n]
        a, b = slope.at(ua, va), slope.at(ub, vb)
        out = (b - a).cross(slope.N)
        if out.dot((a + b) / 2 - centroid) < 0:
            out = -out
        on_coded_slopes("Siding", [a, b, slope.at(ub, vb, -DECK), slope.at(ua, va, -DECK)], out)
    height = FLAT_RIB[1]
    vs = [p[1] for p in slope.uv]
    v = min(vs) + FLAT_COURSE
    while v < max(vs) - 0.1:
        span = slope.span(1, v)
        if span and span[1] - span[0] > 0.1:
            lo, hi = span
            on_coded_slopes("Roof", [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, 0.03), slope.at(lo, v, 0.03)], -slope.V)
            on_coded_slopes("Roof", [slope.at(lo, v, 0.03), slope.at(hi, v, 0.03), slope.at(hi, v + LIP, height * 0.5), slope.at(lo, v + LIP, height * 0.5)], slope.N)
        v += FLAT_COURSE
    return slope


def cap_on_coded(a, b, half=MAIN_CAP[0], lift=MAIN_CAP[1]):
    """The kit's main ridge cap (two flanks and a flat crown), cut to the coded slopes."""
    a, b = Vector(a), Vector(b)
    side = (b - a).normalized().cross(Vector((0, 1, 0))).normalized() * half
    up = Vector((0, lift, 0))
    crown = side * CAP_CROWN
    on_coded_slopes("Roof", [a + side, b + side, b + crown + up, a + crown + up], side + up * 2)
    on_coded_slopes("Roof", [a - side, a - crown + up, b - crown + up, b - side], -side + up * 2)
    on_coded_slopes("Roof", [a + crown + up, b + crown + up, b - crown + up, a - crown + up], Vector((0, 1, 0)))


EX, EZ, R = EAVE_X, EAVE_Z, DRAWN_RIDGE_HALF
MAIN = {
    "left": [(-EX, SIDE_EAVE_Y, -EZ), (-EX, SIDE_EAVE_Y, EZ), (0.0, RIDGE_Y, R), (0.0, RIDGE_Y, -R)],
    "right": [(EX, SIDE_EAVE_Y, EZ), (EX, SIDE_EAVE_Y, -EZ), (0.0, RIDGE_Y, -R), (0.0, RIDGE_Y, R)],
    "front": [(-EX, SIDE_EAVE_Y, EZ), (EX, SIDE_EAVE_Y, EZ), (0.0, RIDGE_Y, R)],
    "back": [(EX, SIDE_EAVE_Y, -EZ), (-EX, SIDE_EAVE_Y, -EZ), (0.0, RIDGE_Y, -R)],
}
SLOPES = {side: tiled_on_coded(points) for side, points in MAIN.items()}
cap_on_coded((0.0, RIDGE_Y, -R), (0.0, RIDGE_Y, R))
for cx in (-1, 1):
    for cz in (-1, 1):
        cap_on_coded((cx * EX, SIDE_EAVE_Y, cz * EZ), (0.0, RIDGE_Y, cz * R))

# The cross gable on the garage flank's slope, a little behind the middle: its ridge runs
# out to a gable over the flank wall, its slopes meet the main slope in valleys.
GABLE_Z, GABLE_HALF, GABLE_PITCH = -0.3, 1.1, 0.7
GABLE_RIDGE = SIDE_EAVE_Y + GABLE_PITCH * GABLE_HALF
GABLE_FOOT = (RIDGE_Y - GABLE_RIDGE) / PITCH  # where its ridge dies into the main slope
for s in (1, -1):
    tiled_on_coded(
        [(GABLE_FOOT, GABLE_RIDGE, GABLE_Z), (EX, GABLE_RIDGE, GABLE_Z), (EX, SIDE_EAVE_Y, GABLE_Z + s * GABLE_HALF)],
        eave_edges=(1,),
    )
cap_on_coded((GABLE_FOOT, GABLE_RIDGE, GABLE_Z), (EX, GABLE_RIDGE, GABLE_Z))
# its gable in board and batten over the flank's wall top, under the deck
_peak = GABLE_RIDGE - DECK - 0.02
_spread = (_peak - WALL_TOP) / GABLE_PITCH
_right = WALL_FRAMES["right"]
_gable = [_right.p(-GABLE_Z - _spread, WALL_TOP, -CLAD_OUT), _right.p(-GABLE_Z + _spread, WALL_TOP, -CLAD_OUT), _right.p(-GABLE_Z, _peak, -CLAD_OUT)]
face("RightWall__Cladding", _gable, _right.out)
for k in (-1, 0, 1):
    u = -GABLE_Z + k * 0.3
    top = _peak - GABLE_PITCH * abs(k * 0.3) - 0.03
    _right.quad("RightWall__Cladding", u - 0.012, WALL_TOP, u + 0.012, top, -BATTEN_OUT)

# ---------------------------------------------------------------- the flanks and back


def left_wall():
    """The kit's plain_wall for the left flank, its upper windows sized one by one and
    not all over a ground window."""
    frame, part = WALL_FRAMES["left"], WALL_PARTS["left"]
    plan = PLAN["walls"]["left"]
    holes = [(u - LEFT_GROUND_HALF[u], GROUND_GLASS[0], u + LEFT_GROUND_HALF[u], GROUND_GLASS[1]) for u, _ in plan["ground"]]
    holes += [(u - LEFT_UPPER_HALF[u], UPPER_GLASS[0], u + LEFT_UPPER_HALF[u], UPPER_GLASS[1]) for u in LEFT_DRAWN_UPPER]
    wall(part, frame, -HALF_D, HALF_D, 0, WALL_TOP, holes)
    for hole in holes:
        window(part, frame, *hole, mullion=hole[2] - hole[0] > 0.6)


plain_wall("back", PLAN, half_width=0.3, upper_half=BACK_UPPER_HALF)
plain_wall("right", PLAN, half_width=SIDE_HALF)
left_wall()

# The arrays: on the left flank's slope a single panel toward the back, a block of six
# laid landscape in the middle and a square of four toward the front; three over the
# garage flank's slope toward the back.
LANDSCAPE = (SOLAR_PANEL[1], SOLAR_PANEL[0])


def panels(slope, z_mid, cols, rows, up, size=SOLAR_PANEL):
    """A block of panels on a flank slope (u along z from the back eave), its middle at
    z_mid and its foot `up` of the slope, cut to the coded slopes under it."""
    vs = [v for _, v in slope.uv]
    across = slope.U.z  # +1 when u runs to the front
    width = cols * size[0] + (cols - 1) * SOLAR_GAP
    u_mid = (z_mid - slope.origin.z) * across
    u0 = u_mid - width / 2
    v0 = min(vs) + (max(vs) - min(vs)) * up
    for c in range(cols):
        for r in range(rows):
            pu, pv = u0 + c * (size[0] + SOLAR_GAP), v0 + r * (size[1] + SOLAR_GAP)
            corners = [(pu, pv), (pu + size[0], pv), (pu + size[0], pv + size[1]), (pu, pv + size[1])]
            if not all((span := slope.span(1, v)) and span[0] + 0.05 <= u <= span[1] - 0.05 for u, v in corners):
                continue
            on_coded_slopes("Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


panels(SLOPES["left"], -2.3, 1, 1, 0.25)
panels(SLOPES["left"], -0.65, 2, 3, 0.08, LANDSCAPE)
panels(SLOPES["left"], 1.55, 2, 2, 0.06)
panels(SLOPES["right"], -2.15, 3, 1, 0.15)

export(PLAN, frame="along")
