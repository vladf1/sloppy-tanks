# The Senda Craftsman house whose hip runs along the street (styles 1 and 2), modelled
# after Rumbo north lot 9, for Sloppy Tanks in Blender (headless):
#   blender --background --python assets/blender/craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (bun run house:craftsman writes public/models/houses/craftsman.glb and
#   crates/core/data/houses/craftsman.plan.json)
#
# Drawn in the street frame on a 6.2 x 8 m lot (the tract's 11.7 x 15 m box at the level's
# scale), its garage on the right; the game builds it with its door in the eave wall and
# fits it to every lot of its styles. A two-storey stucco block under a low flat-tile hip
# whose ridge runs from the street to the back (as the drone sees it from above: long side
# slopes, a hip end to the street clipped by a small gablet at its top), the half over the
# porch stepping its eave out under a little hip of its own; a dark stained box bay over the
# garage under a tile hood, a pair of shuttered windows over the porch and a small window
# beside the bay; before the porch half an open porch under a flat-tile hip leaning on the
# wall, on tapered posts over stacked-stone piers, the front door and a window behind it; the
# garage's carriage door with a pair of pulls and a lantern by it; a block of six panels on
# the slope over the porch side.
# The game's coded roof for this design is the hip along the street; the drawn roof's faces
# stay in the part whose coded slope lies under them ([`coded_roof`]), so each falls with it.
# The paints are the style's.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded Craftsman front (a window and the door in the entry's half, the garage's leaves
# and the bay's pair over the garage, the window over the door) with the second of the pair
# over the porch; two windows a floor down each flank and three across the back. The small
# window beside the bay is drawn only: planned, it would land on the garage's offset, off
# its hole on the narrower lots.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [1.0, 1.8, -0.66, -1.65], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.3, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.3, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.2, "Window"], [1.9, "Window"]], "upper": [-2.2, 1.9], "garage_bay": 0.0},
        "left": {"ground": [[-1.9, "Window"], [2.1, "Window"]], "upper": [-1.9, 2.1], "garage_bay": 0.0},
    },
}
front = front_frame()
_front = PLAN["walls"]["front"]
window_u = _front["ground"][0][0]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
BAY = (garage_u - 0.8, garage_u + 1.0)
BAY_HOLE = (BAY[0] + 0.05, UPPER_GLASS[0] - 0.08, BAY[1] - 0.05, BAY_HI - 0.05)
# Under the porch: the coded ground window and the door beside it.
GROUND_WINDOW = (window_u - WINDOW_HALF, GROUND_GLASS[0], window_u + WINDOW_HALF, GROUND_GLASS[1])
# Over the porch a pair of windows, the right one over the door (the coded column), and a
# small one between the pair and the bay.
UPPER_PAIR = [(-2.0, -1.3), (-1.22, -0.52)]
SMALL = (0.1, 0.44)
uppers = [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in UPPER_PAIR + [SMALL]]
holes = [GARAGE, door_hole(door_u), GROUND_WINDOW, BAY_HOLE] + uppers
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
sectional_garage_door(front, garage_u, decor="handles", boards=False, header=True)
front_door(front, door_u, glass=True)
# The window behind the porch: its sill barely proud, as the porch stands right before it.
window("FrontWall", front, fx(GROUND_WINDOW[0]), GROUND_WINDOW[1], fx(GROUND_WINDOW[2]), GROUND_WINDOW[3], sill=False)
front.block("FrontWall__Trim", fx(GROUND_WINDOW[0]) - 0.03, GROUND_WINDOW[1] - 0.035, fx(GROUND_WINDOW[2]) + 0.03, GROUND_WINDOW[1], 0.0, -0.025, faces=("front", "+y", "-y", "+u", "-u"))
for a, y0, b, y1 in uppers:
    window("FrontWall", front, fx(a), y0, fx(b), y1, mullion=False)
board_shutters(front, fx(UPPER_PAIR[0][0]), fx(UPPER_PAIR[1][1]), UPPER_GLASS[0] - 0.01, UPPER_GLASS[1] + 0.01)
box_bay(front, BAY[0], BAY[1], flat=True)
lantern(front, 2.9)

# ---------------------------------------------------------------- roofs, drawn and coded
UP = Vector((0, 1, 0))


class AlignedSlope(Slope):
    """A planar roof polygon whose u runs level and v up the slope from its eave height
    `eave_y`, both measured in the world (not from its first point), so the pieces one
    plane is cut into lay their ribs and courses in line."""

    def __init__(self, points, eave_y):
        self.points = [Vector(p) for p in points]
        n = Vector((0.0, 0.0, 0.0))
        for a, b in zip(self.points, self.points[1:] + self.points[:1]):
            n += Vector(((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y)))
        n.normalize()
        if n.y < 0:
            n = -n
        self.N = n
        self.U = n.cross(UP).normalized()
        v = n.cross(self.U)
        if v.y < 0:
            v = -v
        self.V = v.normalized()
        p0 = self.points[0]
        self.origin = p0 - self.U * p0.dot(self.U) - self.V * ((p0.y - eave_y) / self.V.y)
        self.eave_y = eave_y
        self.uv = [((p - self.origin).dot(self.U), (p - self.origin).dot(self.V)) for p in self.points]


RIB_EVERY = 2


def tiled_aligned(part, slope, rakes=()):
    """The kit's flat-tile slope ([`tiled`]) with its ribs and courses in line across the
    pieces of one plane, and a fascia only along its eaves and the edges in `rakes`
    (a test on an edge's two ends): hips, ridges and cuts are capped or hidden."""
    roof, under = f"{part}__Roof", f"{part}__Siding"
    face(roof, [slope.at(u, v, 0) for u, v in slope.uv], slope.N)
    face(under, [slope.at(u, v, -DECK) for u, v in slope.uv], -slope.N)
    n = len(slope.uv)
    centroid = sum(slope.points, Vector()) / n
    for i in range(n):
        a, b = slope.points[i], slope.points[(i + 1) % n]
        eave = a.y < slope.eave_y + 1e-6 and b.y < slope.eave_y + 1e-6
        if not (eave or any(rake(a, b) for rake in rakes)):
            continue
        (ua, va), (ub, vb) = slope.uv[i], slope.uv[(i + 1) % n]
        out = (b - a).cross(slope.N)
        if out.dot((a + b) / 2 - centroid) < 0:
            out = -out
        face(under, [slope.at(ua, va, 0), slope.at(ub, vb, 0), slope.at(ub, vb, -DECK), slope.at(ua, va, -DECK)], out)
    us = [p[0] for p in slope.uv]
    # a rib every other tile's width (a whole roof of them took more than the house's
    # budget), its open end at the eave left out: flat tile shows it only as a sliver
    width, height = FLAT_RIB
    spacing = RIB_EVERY * width
    u = math.floor(min(us) / spacing) * spacing + spacing / 2
    while u < max(us) - width / 2:
        left, right = slope.span(0, u - width / 2), slope.span(0, u + width / 2)
        if left and right:
            lo, hi = max(left[0], right[0]), min(left[1], right[1])
            if hi - lo > 0.05:
                a, m, b = u - width / 2, u, u + width / 2
                face(roof, [slope.at(a, lo), slope.at(m, lo, height), slope.at(m, hi, height), slope.at(a, hi)], slope.N - slope.U)
                face(roof, [slope.at(m, lo, height), slope.at(b, lo), slope.at(b, hi), slope.at(m, hi, height)], slope.N + slope.U)
        u += spacing
    vs = [p[1] for p in slope.uv]
    v = math.floor(min(vs) / FLAT_COURSE) * FLAT_COURSE + FLAT_COURSE
    while v < max(vs) - 0.1:
        span = slope.span(1, v)
        if span and span[1] - span[0] > 0.1:
            lo, hi = span
            face(roof, [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, 0.03), slope.at(lo, v, 0.03)], -slope.V)
            face(roof, [slope.at(lo, v, 0.03), slope.at(hi, v, 0.03), slope.at(hi, v + LIP, height * 0.5), slope.at(lo, v + LIP, height * 0.5)], slope.N)
        v += FLAT_COURSE


# The coded roof the game's structure stands: the hip along the street (the kit's
# along_hip), its four slopes over the plan as half-planes a*x + b*z + c >= 0.
_K = (ALONG_EAVE[0] - ALONG_RIDGE_HALF) / ALONG_EAVE[1]
_R = ALONG_RIDGE_HALF
CODED = {
    "FrontRoof": [(0, 1, 0), (-1, _K, _R), (1, _K, _R)],
    "BackRoof": [(0, -1, 0), (-1, -_K, _R), (1, -_K, _R)],
    "RightRoof": [(1, -_K, -_R), (1, _K, -_R)],
    "LeftRoof": [(-1, -_K, -_R), (-1, _K, -_R)],
}


def clip_plan(polygon, a, b, c):
    """A convex polygon (build-frame points) cut to a*x + b*z + c >= 0."""
    out = []
    n = len(polygon)
    for i in range(n):
        p, q = Vector(polygon[i]), Vector(polygon[(i + 1) % n])
        fp, fq = a * p.x + b * p.z + c, a * q.x + b * q.z + c
        if fp >= 0:
            out.append(p)
        if (fp > 1e-9 and fq < -1e-9) or (fp < -1e-9 and fq > 1e-9):
            out.append(p.lerp(q, fp / (fp - fq)))
    kept = []
    for p in out:
        if not kept or (p - kept[-1]).length > 1e-6:
            kept.append(p)
    if len(kept) > 1 and (kept[0] - kept[-1]).length < 1e-6:
        kept.pop()
    return kept


def coded_part(point):
    """The coded slope over a plan point."""
    for part, planes in CODED.items():
        if all(a * point[0] + b * point[2] + c >= -1e-9 for a, b, c in planes):
            return part
    return "FrontRoof"


def coded_roof(points, eave_y, rakes=()):
    """A convex piece of the drawn roof, tiled in the coded parts it lies over."""
    for part, planes in CODED.items():
        piece = [Vector(p) for p in points]
        for plane in planes:
            piece = clip_plan(piece, *plane)
            if len(piece) < 3:
                break
        if len(piece) < 3:
            continue
        area = sum(((piece[i] - piece[0]).cross(piece[i + 1] - piece[0])).length for i in range(1, len(piece) - 1)) / 2
        if area > 1e-3:
            tiled_aligned(part, AlignedSlope(piece, eave_y), rakes)


def coded_cap(a, b, **cap):
    """A ridge cap from a to b, cut where it crosses from one coded slope to the next."""
    a, b = Vector(a), Vector(b)
    d = b - a
    for part, planes in CODED.items():
        t0, t1 = 0.0, 1.0
        for pa, pb, pc in planes:
            f0 = pa * a.x + pb * a.z + pc
            df = pa * d.x + pb * d.z
            if abs(df) < 1e-12:
                if f0 < 0:
                    t0, t1 = 1.0, 0.0
                continue
            t = -f0 / df
            if df > 0:
                t0 = max(t0, t)
            else:
                t1 = min(t1, t)
        if t1 - t0 > 1e-4:
            ridge_cap(part, a + d * t0, a + d * t1, **cap)


# ---------------------------------------------------------------- the main roof
# The hip whose ridge runs from the street to the back at the coded ridge's height, its long
# slopes to the flanks at the coded pitch; the hip end to the street stops under a small
# gablet; the half over the porch steps its eave out (STEP) under a little hip of its own,
# whose ridge runs back into the main hip end.
EX, EZ, EY = EAVE_X, EAVE_Z, EAVE_Y
STEP = 0.3
GABLET = 0.9  # the gablet's half width
RIDGE_BACK = -(EZ - EX)
Z_GABLET = (EZ - EX) + GABLET
Y_GABLET = EY + PITCH * (EX - GABLET)
WING_X = (-EX, -0.3 + OVERHANG)  # the porch half's eaves
WING_HALF = (WING_X[1] - WING_X[0]) / 2
WING_Y = EY + PITCH * WING_HALF
WING_RIDGE_X = (WING_X[0] + WING_X[1]) / 2
B = (WING_RIDGE_X, WING_Y, EZ + STEP - WING_HALF)  # the porch half's hip apex
R = (WING_RIDGE_X, WING_Y, EZ - WING_HALF)  # where its ridge meets the main hip end
assert abs(R[2] - (EZ - EX - R[0])) < 1e-9, "the step's ridge meets the main hip line"
K = (0.0, RIDGE_Y, RIDGE_BACK)
F = (0.0, RIDGE_Y, Z_GABLET)
G_L, G_R = (-GABLET, Y_GABLET, Z_GABLET), (GABLET, Y_GABLET, Z_GABLET)
FWL = (-EX, EY, EZ + STEP)  # the stepped eave's corners
FWR = (WING_X[1], EY, EZ + STEP)
V_FOOT = (WING_X[1], EY, EZ)  # the valley's foot
FR, BR, BL = (EX, EY, EZ), (EX, EY, -EZ), (-EX, EY, -EZ)


def gablet_rake(a, b):
    return abs(a.z - Z_GABLET) < 1e-6 and abs(b.z - Z_GABLET) < 1e-6


# the long slopes, each in convex pieces
coded_roof([BL, FWL, B, R], EY)
coded_roof([BL, R, G_L, K], EY)
coded_roof([G_L, F, K], EY, rakes=[gablet_rake])
coded_roof([FR, BR, K, G_R], EY)
coded_roof([G_R, K, F], EY, rakes=[gablet_rake])
# the back hip end, the street's under the gablet, the step's hip end and its side
coded_roof([BR, BL, K], EY)
coded_roof([V_FOOT, FR, G_R, G_L, R], EY)
coded_roof([FWL, FWR, B], EY)
coded_roof([V_FOOT, FWR, B, R], EY)
# the gablet's stucco face
face("FrontRoof__Siding", [(-GABLET, Y_GABLET, Z_GABLET), (GABLET, Y_GABLET, Z_GABLET), (0.0, RIDGE_Y - DECK, Z_GABLET)], (0, 0, 1))
lattice_vent("FrontRoof", Frame((0, 0, Z_GABLET), (1, 0, 0), (0, 0, 1)), 0.0, Y_GABLET + 0.14, size=0.12)
# ridge, hips, the gablet's rakes and the valley
coded_cap(K, F)
for a, b in ((BL, K), (BR, K), (FR, G_R), (R, G_L), (G_L, F), (G_R, F), (FWL, B), (FWR, B), (B, R)):
    coded_cap(a, b)
coded_cap(V_FOOT, R, half=0.04, lift=0.01)

# The array: two panels laid landscape along the slope over the porch side, three rows up
# it, from the middle of the house toward the street.
PANEL = (1.7 * SCALE, 1.0 * SCALE)  # along the eave, up the slope
SLOPE_RUN = math.hypot(1.0, PITCH)


def west_point(z, run, lift=SOLAR_LIFT):
    """A point on the long slope over the porch side, `run` up it from its eave."""
    x = -EX + run / SLOPE_RUN
    y = EY + PITCH * run / SLOPE_RUN
    n = Vector((-PITCH, 1.0, 0.0)).normalized()
    return Vector((x, y, z)) + n * lift


ARRAY_MIDDLE, ARRAY_FOOT = 0.55, 1.75
for column in range(2):
    for row in range(3):
        z0 = ARRAY_MIDDLE - PANEL[0] - SOLAR_GAP / 2 + column * (PANEL[0] + SOLAR_GAP)
        r0 = ARRAY_FOOT + row * (PANEL[1] + SOLAR_GAP)
        corners = [west_point(z0, r0), west_point(z0 + PANEL[0], r0), west_point(z0 + PANEL[0], r0 + PANEL[1]), west_point(z0, r0 + PANEL[1])]
        middle = sum(corners, Vector()) / 4
        face(coded_part(middle) + "__Solar", corners, Vector((-PITCH, 1.0, 0.0)))

# ---------------------------------------------------------------- the porch
# Open, before the porch half of the front: a slab, a tapered post at each front corner on stacked-stone
# piers carrying a beam, and a flat-tile hip leaning on the wall under the upper sills, its
# ends steeper than its street slope. Drawn as the `Entry` model in the kit's entry box.
PORCH_X = (-HALF_W, -0.3)
PORCH_FRONT = HALF_D + 1.5
PORCH_EAVE, PORCH_TOP = 1.55, 2.0
PORCH_OVER = 0.2
PORCH_END = 0.7  # how far in from each end its hips meet the wall
post_z = PORCH_FRONT - 0.14
beam_y = PORCH_EAVE - 0.06
# the slab stops short of the piers' outer faces, which would otherwise lie on its edges
box("Entry__Concrete", (PORCH_X[0] + ENTRY_INSET, 0.0, HALF_D), (PORCH_X[1] - ENTRY_INSET, SLAB, PORCH_FRONT - ENTRY_INSET))
box("Entry__Concrete", (PORCH_X[0] + 0.9, 0.0, PORCH_FRONT), (PORCH_X[1] - 0.2, SLAB / 2, PORCH_FRONT + 0.2))


def tapered_post(x, z):
    """A Craftsman post tapering up from a stacked-stone pier under a pale cap."""
    box("Entry__Stone", (x - 0.14, 0.0, z - 0.14), (x + 0.14, 0.5, z + 0.14))
    box("Entry__Concrete", (x - 0.16, 0.5, z - 0.16), (x + 0.16, 0.54, z + 0.16))
    lo, hi = 0.1, 0.06
    corners = lambda h, y: [(x - h, y, z - h), (x + h, y, z - h), (x + h, y, z + h), (x - h, y, z + h)]  # noqa: E731
    foot, head = corners(lo, 0.54), corners(hi, beam_y - 0.12)
    for k in range(4):
        mid = (Vector(foot[k]) + Vector(foot[(k + 1) % 4])) / 2
        face("Entry__Wood", [foot[k], foot[(k + 1) % 4], head[(k + 1) % 4], head[k]], (mid.x - x, 0, mid.z - z))


for x in (PORCH_X[0] + 0.14, PORCH_X[1] - 0.14):
    tapered_post(x, post_z)
box("Entry__Wood", (PORCH_X[0] + 0.04, beam_y - 0.12, post_z - 0.07), (PORCH_X[1] - 0.04, beam_y, post_z + 0.07))
for x in (PORCH_X[0] + 0.14, PORCH_X[1] - 0.14):
    box("Entry__Wood", (x - 0.06, beam_y - 0.12, HALF_D), (x + 0.06, beam_y, post_z - 0.07))
xa, xb, ze = PORCH_X[0] - PORCH_OVER, PORCH_X[1] + PORCH_OVER, PORCH_FRONT + PORCH_OVER
top_a, top_b = (xa + PORCH_END, PORCH_TOP, HALF_D), (xb - PORCH_END, PORCH_TOP, HALF_D)
tiled("Entry", Slope([(xa, PORCH_EAVE, ze), (xb, PORCH_EAVE, ze), top_b, top_a]), flat=True)
tiled("Entry", Slope([(xa, PORCH_EAVE, HALF_D), (xa, PORCH_EAVE, ze), top_a]), flat=True)
tiled("Entry", Slope([(xb, PORCH_EAVE, ze), (xb, PORCH_EAVE, HALF_D), top_b]), flat=True)
ridge_cap("Entry", (xa, PORCH_EAVE, ze), top_a)
ridge_cap("Entry", (xb, PORCH_EAVE, ze), top_b)
box("Entry__Wood", (xa, PORCH_EAVE - DECK - 0.05, ze - 0.035), (xb, PORCH_EAVE - DECK + 0.01, ze + 0.005))
porch_top = max(v[1] for key, (vertices, _) in geometry.items() if key.startswith("Entry__") for v in vertices)
# the kit's entry box (the office and passage's): the porch fills it
ENTRY_BOX.update(x=(PORCH_X[0] + PORCH_X[1]) / 2, z=(HALF_D + PORCH_FRONT) / 2, w=PORCH_X[1] - PORCH_X[0], d=PORCH_FRONT - HALF_D, h=round(porch_top, 3))

# ---------------------------------------------------------------- the flanks and back
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-1.6: 0.4, 0.3: 0.4, 2.0: 0.3})
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.27)

export(PLAN, frame="along")
