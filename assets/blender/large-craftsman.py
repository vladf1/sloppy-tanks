# The Senda large Craftsman house, modelled after Rumbo north lots 3, 7 and 11, for Sloppy Tanks
# in Blender (headless):
#   blender --background --python assets/blender/large-craftsman.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/large-craftsman.glb and
#   crates/core/data/houses/large-craftsman.plan.json)
#
# Drawn in the street frame on a 5 x 7 m lot, its garage on the right; the game builds it
# with its door in the eave wall (its hip's ridge along the street) and fits it to every lot
# of its styles. A broad two-storey stucco block under a low flat-tile hip; at the front
# left a one-storey dark brick porch box under a flat-tile shed roof, a wide window in its
# front and brick piers framing an arched entry with the door recessed behind it; the
# garage on the right with an X-braced carriage door; over it a balcony between stucco
# cheeks with a dark iron railing and French doors behind; across the upper floor a pair of
# windows over the porch and a shuttered window in the middle. The paints are the style's.

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

# The coded large Craftsman front (door bay, garage leaves and the two uppers over the
# garage, here the balcony's doors) with the pair over the entry; two windows down each
# flank and a pair across the back, lit upstairs.
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
front = front_frame()
_front = PLAN["walls"]["front"]
door_u = _front["ground"][0][0]
garage_u, garage_bay = _front["ground"][1][0], _front["garage_bay"]


def slab(key, back_key, frame, u0, u1, y0, y1, holes, thick=WALL):
    """A slab of `key` (brick) with rectangular holes, its inside face `back_key`, its
    reveals `key`."""
    us = sorted({u0, u1, *[o[0] for o in holes], *[o[2] for o in holes]})
    ys = sorted({y0, y1, *[o[1] for o in holes], *[o[3] for o in holes]})
    for a, b in zip(us, us[1:]):
        for c, d in zip(ys, ys[1:]):
            mid = ((a + b) / 2, (c + d) / 2)
            if any(o[0] < mid[0] < o[2] and o[1] < mid[1] < o[3] for o in holes):
                continue
            frame.quad(key, a, c, b, d)
            frame.quad(back_key, a, c, b, d, thick, facing=-1)
    for ou0, oy0, ou1, oy1 in holes:
        face(key, [frame.p(ou0, oy0, 0), frame.p(ou0, oy1, 0), frame.p(ou0, oy1, thick), frame.p(ou0, oy0, thick)], frame.along)
        face(key, [frame.p(ou1, oy0, 0), frame.p(ou1, oy0, thick), frame.p(ou1, oy1, thick), frame.p(ou1, oy1, 0)], -frame.along)
        face(key, [frame.p(ou0, oy1, 0), frame.p(ou1, oy1, 0), frame.p(ou1, oy1, thick), frame.p(ou0, oy1, thick)], (0, -1, 0))
        if oy0 > y0:
            face(key, [frame.p(ou0, oy0, 0), frame.p(ou0, oy0, thick), frame.p(ou1, oy0, thick), frame.p(ou1, oy0, 0)], (0, 1, 0))


def strip(key, frame, a, b, width, depth0, depth1):
    """A board from (u, y) a to b standing proud between depth0 and depth1: its face and
    long edges."""
    a, b = Vector((a[0], a[1])), Vector((b[0], b[1]))
    side = Vector((-(b - a).y, (b - a).x)).normalized() * (width / 2)
    corners = [a - side, b - side, b + side, a + side]
    face(key, [frame.p(c.x, c.y, depth1) for c in corners], frame.out)
    for p, q in ((corners[0], corners[1]), (corners[2], corners[3])):
        outward = (p + q) / 2 - (a + b) / 2
        face(key, [frame.p(p.x, p.y, depth0), frame.p(q.x, q.y, depth0), frame.p(q.x, q.y, depth1), frame.p(p.x, p.y, depth1)], frame.along * outward.x + Vector((0, outward.y, 0)))


# ---------------------------------------------------------------- the front wall
GARAGE = (garage_u - 2 * garage_bay + 0.14, 0.12, garage_u + 2 * garage_bay - 0.12, 1.5)
DOOR = (door_u - 0.3, 0.12, door_u + 0.3, 1.5)
PAIR = [(-1.86, -1.42), (-1.3, -0.86)]  # the pair over the porch; the second holds -1.23
MIDDLE = (-0.28, 0.1)  # the shuttered window in the middle
BALCONY_DOOR = (0.56, 1.92, 1.8, 2.8)  # French doors over 0.68 and 1.18
holes = [
    (fx(GARAGE[0]), GARAGE[1], fx(GARAGE[2]), GARAGE[3]),
    (fx(DOOR[0]), DOOR[1], fx(DOOR[2]), DOOR[3]),
    (fx(MIDDLE[0]), UPPER_GLASS[0], fx(MIDDLE[1]), UPPER_GLASS[1]),
    (fx(BALCONY_DOOR[0]), BALCONY_DOOR[1], fx(BALCONY_DOOR[2]), BALCONY_DOOR[3]),
]
holes += [(fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1]) for a, b in PAIR]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, holes)

for a, b in PAIR:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=False)
window("FrontWall", front, fx(MIDDLE[0]), UPPER_GLASS[0], fx(MIDDLE[1]), UPPER_GLASS[1], mullion=False)
board_shutters(front, fx(MIDDLE[0]), fx(MIDDLE[1]), UPPER_GLASS[0] - 0.03, UPPER_GLASS[1] + 0.03, width=0.2)

# The entry door, recessed in the porch: a dark stained door in raised panels.
d0, d1 = fx(DOOR[0]), fx(DOOR[2])
front.quad("FrontWall__Wood", d0, DOOR[1], d1, DOOR[3], 0.1)
for c, d in ((0.3, 0.75), (0.88, 1.38)):
    front.block("FrontWall__Wood", d0 + 0.08, c, d1 - 0.08, d, 0.1, 0.08)
front.block("FrontWall__Metal", d1 - 0.12, 0.82, d1 - 0.09, 0.9, 0.08, 0.05)

# The garage's carriage door: two stained leaves, each framed in boards and X-braced.
g0, g1 = fx(GARAGE[0]), fx(GARAGE[2])
gy0, gy1 = GARAGE[1], GARAGE[3]
front.quad("FrontWall__GarageDoor", g0, gy0, g1, gy1, 0.12)
mid = (g0 + g1) / 2
board = 0.09
for l0, l1 in ((g0, mid), (mid, g1)):
    for a, c, b, d in ((l0, gy0, l1, gy0 + board), (l0, gy1 - board, l1, gy1), (l0, gy0 + board, l0 + board, gy1 - board), (l1 - board, gy0 + board, l1, gy1 - board)):
        front.block("FrontWall__GarageDoor", a, c, b, d, 0.12, 0.095)
    lo, hi = (l0 + board, gy0 + board), (l1 - board, gy1 - board)
    strip("FrontWall__GarageDoor", front, lo, hi, 0.08, 0.12, 0.1)
    strip("FrontWall__GarageDoor", front, (hi[0], lo[1]), (lo[0], hi[1]), 0.08, 0.12, 0.1)
front.block("FrontWall__Dark", mid - 0.012, gy0, mid + 0.012, gy1, 0.12, 0.09)
for handle in (mid - 0.14, mid + 0.14):
    front.block("FrontWall__Iron", handle - 0.012, 0.66, handle + 0.012, 0.88, 0.095, 0.07)
front.block("FrontWall__Trim", g0 - 0.06, gy1, g1 + 0.06, gy1 + 0.06, 0.0, -0.03)
# a lantern by the garage
front.block("FrontWall__Dark", fx(2.2) - 0.06, 1.06, fx(2.2) + 0.06, 1.3, 0.0, -0.12)
front.block("FrontWall__Curtain", fx(2.2) - 0.045, 1.1, fx(2.2) + 0.045, 1.26, -0.02, -0.125)

# ---------------------------------------------------------------- the balcony
BAL = (0.4, HALF_W)  # cheeks' outer edges
BAL_OUT = 0.48
BAL_FLOOR = (1.74, 1.88)
BAL_HEAD = (2.8, 2.95)
CHEEK = (0.16, 0.2)  # left and right cheek widths
zb = HALF_D + BAL_OUT
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[0], HALF_D), (BAL[1], BAL_FLOOR[1], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_FLOOR[1], HALF_D), (BAL[0] + CHEEK[0], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[1] - CHEEK[1], BAL_FLOOR[1], HALF_D), (BAL[1], BAL_HEAD[0], zb), skip=("-z",))
box("FrontWall__Siding", (BAL[0], BAL_HEAD[0], HALF_D), (BAL[1], BAL_HEAD[1], zb), skip=("-z",))
# a dark stained frame inside the opening and a trim band under the floor
box("FrontWall__Wood", (BAL[0] + CHEEK[0], BAL_HEAD[0] - 0.08, zb - 0.1), (BAL[1] - CHEEK[1], BAL_HEAD[0], zb), skip=("+y", "-z"))
box("FrontWall__Trim", (BAL[0] - 0.02, BAL_FLOOR[0] - 0.06, zb - 0.02), (BAL[1], BAL_FLOOR[0] + 0.02, zb + 0.03), skip=("-z", "+y"))
# the iron railing
r0, r1 = BAL[0] + CHEEK[0], BAL[1] - CHEEK[1]
rz0, rz1 = zb - 0.08, zb - 0.05
box("FrontWall__Iron", (r0, 2.4, rz0 - 0.01), (r1, 2.45, rz1 + 0.01))
box("FrontWall__Iron", (r0, BAL_FLOOR[1] + 0.06, rz0), (r1, BAL_FLOOR[1] + 0.09, rz1))
x = r0 + 0.1
while x < r1 - 0.05:
    box("FrontWall__Iron", (x - 0.012, BAL_FLOOR[1] + 0.09, rz0), (x + 0.012, 2.4, rz1), skip=("+y", "-y"))
    x += 0.115
# the French doors behind
b0, b1 = fx(BALCONY_DOOR[0]), fx(BALCONY_DOOR[2])
by0, by1 = BALCONY_DOOR[1], BALCONY_DOOR[3]
front.quad("FrontWall__Curtain", b0, by0, b1, by1, 0.12)
bm = (b0 + b1) / 2
t = 0.06
for a, c, b, d in ((b0, by0, b1, by0 + 0.14), (b0, by1 - t, b1, by1), (b0, by0, b0 + t, by1), (b1 - t, by0, b1, by1), (bm - t, by0, bm + t, by1)):
    front.block("FrontWall__Wood", a, c, b, d, 0.12, 0.08)
for u in ((b0 + bm) / 2, (bm + b1) / 2):
    front.block("FrontWall__Wood", u - 0.015, by0 + 0.14, u + 0.015, by1 - t, 0.12, 0.1)
front.block("FrontWall__Wood", b0, 2.36, b1, 2.39, 0.12, 0.1)

# ---------------------------------------------------------------- the porch
PX = (-2.72, -0.45)  # its outer sides
PF = HALF_D + 1.15  # its front face
PT = 1.68  # its walls' top
BRICK = 0.2
EAVE_Z, EAVE_Y, HEAD_Y = PF + 0.27, 1.7, 2.14  # the shed's eave and its top at the wall
ROOF_X = (PX[0] - 0.18, PX[1] + 0.15)


def porch_roof_under(z):
    """The shed's underside over z."""
    return EAVE_Y + (EAVE_Z - z) * (HEAD_Y - EAVE_Y) / (EAVE_Z - HALF_D) - DECK


box("FrontWall__Concrete", (PX[0], 0.0, HALF_D), (PX[1], 0.1, PF))
# the front: a wide window, a brick pier, the arched entry, a broad pier by the garage
porch_front = Frame((PX[0], 0, PF), (1, 0, 0), (0, 0, 1))  # u = x - PX[0]
px = lambda x: x - PX[0]  # noqa: E731
WIDE = (-2.58, -1.66, 0.6, 1.44)
ENTRY = (-1.46, -0.96)
SPRING, RISE = 1.24, 0.25
ENTRY_TOP = SPRING + RISE
slab(
    "FrontWall__Brick",
    "FrontWall__Siding",
    porch_front,
    0,
    px(PX[1]),
    0,
    PT,
    [(px(WIDE[0]), WIDE[2], px(WIDE[1]), WIDE[3]), (px(ENTRY[0]), 0.1, px(ENTRY[1]), ENTRY_TOP)],
    BRICK,
)
# the entry's round head, in brick
e0, e1 = px(ENTRY[0]), px(ENTRY[1])
emid, ehalf = (e0 + e1) / 2, (e1 - e0) / 2
arc = [(emid - ehalf * math.cos(math.pi * k / 6), SPRING + RISE * math.sin(math.pi * k / 6)) for k in range(7)]
for (ua, ya), (ub, yb) in zip(arc, arc[1:]):
    face("FrontWall__Brick", [porch_front.p(ua, ya), porch_front.p(ub, yb), porch_front.p(ub, ENTRY_TOP), porch_front.p(ua, ENTRY_TOP)], porch_front.out)
    face("FrontWall__Brick", [porch_front.p(ua, ya), porch_front.p(ua, ya, BRICK), porch_front.p(ub, yb, BRICK), porch_front.p(ub, yb)], (0, -1, 0))
    face("FrontWall__Siding", [porch_front.p(ua, ya, BRICK), porch_front.p(ua, ENTRY_TOP, BRICK), porch_front.p(ub, ENTRY_TOP, BRICK), porch_front.p(ub, yb, BRICK)], -porch_front.out)
# a soldier course over the window and at the piers' caps
porch_front.block("FrontWall__Concrete", px(WIDE[0]) - 0.05, WIDE[2] - 0.06, px(WIDE[1]) + 0.05, WIDE[2], 0.0, -0.05)
porch_front.block("FrontWall__Brick", px(WIDE[0]) - 0.04, WIDE[3], px(WIDE[1]) + 0.04, WIDE[3] + 0.09, 0.0, -0.025)
# the wide window: a dark stained frame, three lights
w0, w1, wy0, wy1 = px(WIDE[0]), px(WIDE[1]), WIDE[2], WIDE[3]
porch_front.quad("FrontWall__Curtain", w0, wy0, w1, wy1, 0.12)
for a, c, b, d in ((w0, wy0, w1, wy0 + 0.06), (w0, wy1 - 0.06, w1, wy1), (w0, wy0, w0 + 0.06, wy1), (w1 - 0.06, wy0, w1, wy1)):
    porch_front.block("FrontWall__Wood", a, c, b, d, 0.12, 0.06)
for k in (1, 2):
    u = w0 + (w1 - w0) * k / 3
    porch_front.block("FrontWall__Wood", u - 0.025, wy0, u + 0.025, wy1, 0.12, 0.07)
porch_front.block("FrontWall__Wood", w0, wy1 - 0.26, w1, wy1 - 0.235, 0.12, 0.09)

# the sides, brick up to the shed's underside
for x, out in ((PX[0], -1), (PX[1], 1)):
    inner = x - out * BRICK
    outline = [(HALF_D, 0.0), (PF, 0.0), (PF, PT), (HALF_D, porch_roof_under(HALF_D))]
    face("FrontWall__Brick", [(x, y, z) for z, y in outline], (out, 0, 0))
    face("FrontWall__Siding", [(inner, y, z) for z, y in outline], (-out, 0, 0))
# the shed roof
tiled("FrontWall", Slope([(ROOF_X[0], EAVE_Y, EAVE_Z), (ROOF_X[1], EAVE_Y, EAVE_Z), (ROOF_X[1], HEAD_Y, HALF_D), (ROOF_X[0], HEAD_Y, HALF_D)]), flat=True)
# a step before the entry
box("FrontWall__Concrete", (ENTRY[0] - 0.12, 0.0, PF), (ENTRY[1] + 0.12, 0.06, PF + 0.4), skip=("-z", "-y"))

# ---------------------------------------------------------------- the roof, flanks and back
along_hip_roof(flat=True)
for side in ("back", "right", "left"):
    plain_wall(side, PLAN)

export(PLAN, frame="along")
