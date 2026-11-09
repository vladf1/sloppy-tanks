# Shared pieces of Senda's house models, scripted for Blender (headless). A house's
# script imports them, builds its parts into `geometry` in the game's build frame for its
# lot (metres: front +z, the garage side +x, y up) and calls `export`. Objects are named
# <HousePart>__<Surface> after the coded house's pieces (crates/core/src/sim/house_layout.rs
# HousePart, building_kit.rs Surface), so the structural house cuts them as it cuts the
# coded meshes.
import math
import sys
from collections import defaultdict

import bpy
from mathutils import Vector


# The level's scale: game metres per real metre (a Bruiser hull is 1.95 wide). Every
# detail below is its real size times this, not an old size stretched.
SCALE = 0.533

# The lot a model is drawn for (crates/core/examples/home_dump.rs prints a lot's): the
# tract's 11.7 x 15 m two-storey box, two 3 m storeys to its 6 m eaves under a 5:12 clay
# roof whose ridge rises 2.5 m over half the front (house_layout.rs GARAGE_HOUSE_PITCH),
# eaves overhanging 0.55 m.
HALF_W, HALF_D = 3.1, 4.0
WALL_TOP = 3.2
PITCH = 0.42
OVERHANG = 0.3
RIDGE_Y = WALL_TOP + PITCH * HALF_W
WALL = 0.2  # slab depth, siding to lining
EAVE_X, EAVE_Z = HALF_W + OVERHANG, HALF_D + OVERHANG
EAVE_Y = WALL_TOP - PITCH * OVERHANG
RIDGE_HALF = EAVE_Z - EAVE_X  # a front-to-back hip's ridge, either way from the middle
DECK = 0.1  # roof thickness at the edges: the fascia
RIB = (0.3 * SCALE, 0.07 * SCALE)  # barrel tile spacing and height
COURSE = 0.42 * SCALE  # tile course spacing down a slope
LIP = 0.25 * SCALE  # how far down the slope a course's lip runs

geometry = defaultdict(lambda: ([], []))  # key -> (vertices, faces)


def V(x, y, z):
    return Vector((x, y, z))


def face(key, points, want=None):
    """A polygon in the build frame; flipped to face `want` if given."""
    vertices, faces = geometry[key]
    points = [Vector(p) for p in points]
    if want is not None:
        normal = (points[1] - points[0]).cross(points[2] - points[0])
        if normal.length < 1e-12 and len(points) > 3:
            normal = (points[2] - points[1]).cross(points[3] - points[1])
        if normal.dot(Vector(want)) < 0:
            points.reverse()
    start = len(vertices)
    vertices.extend(points)
    faces.append(list(range(start, start + len(points))))


def box(key, lo, hi, skip=()):
    lo, hi = Vector(lo), Vector(hi)
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    sides = {
        "+x": ([(x1, y0, z0), (x1, y1, z0), (x1, y1, z1), (x1, y0, z1)], (1, 0, 0)),
        "-x": ([(x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0)], (-1, 0, 0)),
        "+y": ([(x0, y1, z0), (x0, y1, z1), (x1, y1, z1), (x1, y1, z0)], (0, 1, 0)),
        "-y": ([(x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1)], (0, -1, 0)),
        "+z": ([(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)], (0, 0, 1)),
        "-z": ([(x0, y0, z0), (x0, y1, z0), (x1, y1, z0), (x1, y0, z0)], (0, 0, -1)),
    }
    for name, (points, want) in sides.items():
        if name not in skip:
            face(key, points, want)


class Frame:
    """A wall's frame: `origin` on its outer face at u = 0, y = 0, `along` its length,
    `out` its outward normal; (u, y, depth into the wall) -> build frame."""

    def __init__(self, origin, along, out):
        self.origin, self.along, self.out = Vector(origin), Vector(along), Vector(out)

    def p(self, u, y, depth=0.0):
        return self.origin + self.along * u + Vector((0, y, 0)) - self.out * depth

    def quad(self, key, u0, y0, u1, y1, depth=0.0, facing=1):
        face(
            key,
            [self.p(u0, y0, depth), self.p(u1, y0, depth), self.p(u1, y1, depth), self.p(u0, y1, depth)],
            self.out * facing,
        )

    def block(self, key, u0, y0, u1, y1, d0, d1):
        """A box in the wall's frame from depth d0 to d1 (negative stands proud)."""
        corners = [self.p(u, y, d) for u in (u0, u1) for y in (y0, y1) for d in (d0, d1)]
        lo = Vector([min(c[i] for c in corners) for i in range(3)])
        hi = Vector([max(c[i] for c in corners) for i in range(3)])
        box(key, lo, hi)


def wall(part, frame, u0, u1, y0, y1, openings):
    """A stucco slab with rectangular openings [(u0, y0, u1, y1)], lined inside; each
    opening's reveal is stucco."""
    us = sorted({u0, u1, *[o[0] for o in openings], *[o[2] for o in openings]})
    ys = sorted({y0, y1, *[o[1] for o in openings], *[o[3] for o in openings]})
    for a, b in zip(us, us[1:]):
        for c, d in zip(ys, ys[1:]):
            mid = ((a + b) / 2, (c + d) / 2)
            if any(o[0] < mid[0] < o[2] and o[1] < mid[1] < o[3] for o in openings):
                continue
            frame.quad(f"{part}__Siding", a, c, b, d)
            frame.quad(f"{part}__Lining", a, c, b, d, WALL, facing=-1)
    for o in openings:
        ou0, oy0, ou1, oy1 = o
        # reveals
        face(f"{part}__Siding", [frame.p(ou0, oy0, 0), frame.p(ou0, oy1, 0), frame.p(ou0, oy1, WALL), frame.p(ou0, oy0, WALL)], frame.along)
        face(f"{part}__Siding", [frame.p(ou1, oy0, 0), frame.p(ou1, oy0, WALL), frame.p(ou1, oy1, WALL), frame.p(ou1, oy1, 0)], -frame.along)
        face(f"{part}__Siding", [frame.p(ou0, oy1, 0), frame.p(ou1, oy1, 0), frame.p(ou1, oy1, WALL), frame.p(ou0, oy1, WALL)], (0, -1, 0))
        face(f"{part}__Siding", [frame.p(ou0, oy0, 0), frame.p(ou0, oy0, WALL), frame.p(ou1, oy0, WALL), frame.p(ou1, oy0, 0)], (0, 1, 0))


# A window's vinyl frame (m across), the plantation shutters' louvre spacing behind it,
# and its sill's lip: real 9 cm frames, 12 cm louvres, a 5 cm sill.
FRAME = 0.06 * SCALE * 1.2
LOUVRE = 0.12 * SCALE
SILL_LIP = 0.05


def window(part, frame, u0, y0, u1, y1, mullion=True, sill=True):
    """White vinyl frames over white plantation shutters, set into an opening."""
    frame.quad(f"{part}__Curtain", u0, y0, u1, y1, 0.11)
    # louvres: slat lines, a face each
    slats = max(2, round((y1 - y0) / LOUVRE))
    for k in range(1, slats):
        y = y0 + (y1 - y0) * k / slats
        frame.quad(f"{part}__Glass", u0 + FRAME, y - 0.004, u1 - FRAME, y + 0.004, 0.105)
    t = FRAME
    frame.block(f"{part}__Trim", u0, y0, u1, y0 + t, 0.1, 0.07)
    frame.block(f"{part}__Trim", u0, y1 - t, u1, y1, 0.1, 0.07)
    frame.block(f"{part}__Trim", u0, y0, u0 + t, y1, 0.1, 0.07)
    frame.block(f"{part}__Trim", u1 - t, y0, u1, y1, 0.1, 0.07)
    if mullion:
        m = (u0 + u1) / 2
        frame.block(f"{part}__Trim", m - t / 2, y0, m + t / 2, y1, 0.1, 0.075)
    if sill:
        frame.block(f"{part}__Trim", u0 - 0.03, y0 - 0.035, u1 + 0.03, y0, 0.02, -SILL_LIP)


# A board shutter (real 0.45 m wide, 4 cm proud) and its battens.
SHUTTER_WIDTH = 0.45 * SCALE


def shutter(part, frame, u0, y0, u1, y1):
    frame.block(f"{part}__Accent", u0, y0, u1, y1, 0.0, -0.025)
    for k in range(1, 3):
        y = y0 + (y1 - y0) * k / 3
        frame.block(f"{part}__Accent", u0 + 0.015, y - 0.012, u1 - 0.015, y + 0.012, -0.025, -0.035)


# ---------------------------------------------------------------- roofs


class Slope:
    """A planar roof polygon (convex, build frame), its first edge level (the eave, or the
    ridge of a piece hung from it): u runs along it, v up the slope."""

    def __init__(self, points):
        self.points = [Vector(p) for p in points]
        p0, p1 = self.points[0], self.points[1]
        centroid = sum(self.points, Vector()) / len(self.points)
        self.U = (p1 - p0).normalized()
        n = self.U.cross(centroid - p0).normalized()
        if n.y < 0:
            n = -n
        self.N = n
        v = n.cross(self.U)
        if v.y < 0 or (abs(v.y) < 1e-9 and (centroid - p0).dot(v) < 0):
            v = -v
        self.V = v.normalized()
        self.origin = p0
        self.uv = [((p - p0).dot(self.U), (p - p0).dot(self.V)) for p in self.points]

    def at(self, u, v, h=0.0):
        return self.origin + self.U * u + self.V * v + self.N * h

    def span(self, axis, value):
        """Where the line (u or v) = value crosses the polygon: (lo, hi) along the other."""
        hits = []
        n = len(self.uv)
        for i in range(n):
            a, b = self.uv[i], self.uv[(i + 1) % n]
            if (a[axis] - value) * (b[axis] - value) <= 0 and a[axis] != b[axis]:
                t = (value - a[axis]) / (b[axis] - a[axis])
                hits.append(a[1 - axis] + t * (b[1 - axis] - a[1 - axis]))
        if len(hits) < 2:
            return None
        return min(hits), max(hits)


# A solar panel on the roof (m across and up the slope: a real 1.0 x 1.7 m panel laid
# portrait), the gap between panels and how far the array stands off the tile.
SOLAR_PANEL = (1.0 * SCALE, 1.7 * SCALE)
SOLAR_GAP = 0.02
SOLAR_LIFT = 0.06


def solar_array(part, slope, cols, rows, up=0.3, across=0.5):
    """A block of `cols` by `rows` panels on `slope`, its middle at `across` of the slope's
    width and its foot `up` of the way from the eave to the ridge; panels that would hang
    past the slope's edges are left out."""
    us = [u for u, _ in slope.uv]
    vs = [v for _, v in slope.uv]
    width = cols * SOLAR_PANEL[0] + (cols - 1) * SOLAR_GAP
    u0 = min(us) + (max(us) - min(us)) * across - width / 2
    v0 = min(vs) + (max(vs) - min(vs)) * up

    def inside(u, v):
        span = slope.span(1, v)
        return span is not None and span[0] + 0.05 <= u <= span[1] - 0.05

    for c in range(cols):
        for r in range(rows):
            pu = u0 + c * (SOLAR_PANEL[0] + SOLAR_GAP)
            pv = v0 + r * (SOLAR_PANEL[1] + SOLAR_GAP)
            corners = [(pu, pv), (pu + SOLAR_PANEL[0], pv), (pu + SOLAR_PANEL[0], pv + SOLAR_PANEL[1]), (pu, pv + SOLAR_PANEL[1])]
            if all(inside(u, v) for u, v in corners):
                face(part + "__Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


# Flat concrete tile (the Craftsman and Prairie roofs): low ribs closer together, deeper
# course lips.
FLAT_RIB = (0.3 * SCALE, 0.025 * SCALE)
FLAT_COURSE = 0.34 * SCALE


def tiled(part, slope, eave_edges=(0,), fascia=True, flat=False):
    """A clay barrel-tile slope (or flat concrete tile, `flat`): the deck, its underside,
    ribs down the slope and a lip at each course, clipped to the polygon."""
    roof, under = f"{part}__Roof", f"{part}__Siding"
    face(roof, [slope.at(u, v, 0) for u, v in slope.uv], slope.N)
    face(under, [slope.at(u, v, -DECK) for u, v in slope.uv], -slope.N)
    n = len(slope.uv)
    for i in range(n):
        (ua, va), (ub, vb) = slope.uv[i], slope.uv[(i + 1) % n]
        edge = slope.at(ub, vb) - slope.at(ua, va)
        out = edge.cross(slope.N)
        mid = (slope.at(ua, va) + slope.at(ub, vb)) / 2
        centroid = sum(slope.points, Vector()) / n
        if out.dot(mid - centroid) < 0:
            out = -out
        if fascia or i in eave_edges:
            face(under, [slope.at(ua, va, 0), slope.at(ub, vb, 0), slope.at(ub, vb, -DECK), slope.at(ua, va, -DECK)], out)
    us = [p[0] for p in slope.uv]
    spacing, height = FLAT_RIB if flat else RIB
    course = FLAT_COURSE if flat else COURSE
    u = min(us) + spacing / 2
    while u < max(us) - spacing / 2:
        left, right = slope.span(0, u - spacing / 2), slope.span(0, u + spacing / 2)
        if left and right:
            lo, hi = max(left[0], right[0]), min(left[1], right[1])
            if hi - lo > 0.05:
                a, m, b = u - spacing / 2, u, u + spacing / 2
                face(roof, [slope.at(a, lo), slope.at(m, lo, height), slope.at(m, hi, height), slope.at(a, hi)], slope.N - slope.U)
                face(roof, [slope.at(m, lo, height), slope.at(b, lo), slope.at(b, hi), slope.at(m, hi, height)], slope.N + slope.U)
                # the barrel's open end at the eave
                face(roof, [slope.at(a, lo), slope.at(b, lo), slope.at(m, lo, height)], -slope.V)
        u += spacing
    vs = [p[1] for p in slope.uv]
    v = min(vs) + course
    while v < max(vs) - 0.1:
        span = slope.span(1, v)
        if span and span[1] - span[0] > 0.1:
            lo, hi = span
            lift = 0.03 if flat else height * 1.25
            face(roof, [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, lift), slope.at(lo, v, lift)], -slope.V)
            face(roof, [slope.at(lo, v, lift), slope.at(hi, v, lift), slope.at(hi, v + LIP, height * 0.5), slope.at(lo, v + LIP, height * 0.5)], slope.N)
        v += course


def ridge_cap(part, a, b, half=0.25 * SCALE / 2, lift=0.1 * SCALE):
    a, b = Vector(a), Vector(b)
    along = (b - a).normalized()
    side = along.cross(Vector((0, 1, 0))).normalized() * half
    up = Vector((0, lift, 0))
    key = f"{part}__Roof"
    face(key, [a + side, b + side, b + up, a + up], side + up)
    face(key, [a - side, a + up, b + up, b - side], -side + up)


def hip_slopes(x0, x1, z0, z1, eave_y, pitch, along_x):
    """The four planes of a hip roof over x0..x1, z0..z1, its ridge along x or z:
    (front +z, back -z, right +x, left -x) as polygons with the eave edge first."""
    if along_x:
        half = (z1 - z0) / 2
        top = eave_y + pitch * half
        zm = (z0 + z1) / 2
        rx0, rx1 = x0 + half, x1 - half
        return {
            "front": [(x0, eave_y, z1), (x1, eave_y, z1), (rx1, top, zm), (rx0, top, zm)],
            "back": [(x1, eave_y, z0), (x0, eave_y, z0), (rx0, top, zm), (rx1, top, zm)],
            "right": [(x1, eave_y, z1), (x1, eave_y, z0), (rx1, top, zm)],
            "left": [(x0, eave_y, z0), (x0, eave_y, z1), (rx0, top, zm)],
            "ridge": ((rx0, top, zm), (rx1, top, zm)),
            "hips": [((x0, eave_y, z0), (rx0, top, zm)), ((x0, eave_y, z1), (rx0, top, zm)),
                     ((x1, eave_y, z0), (rx1, top, zm)), ((x1, eave_y, z1), (rx1, top, zm))],
        }
    half = (x1 - x0) / 2
    top = eave_y + pitch * half
    xm = (x0 + x1) / 2
    rz0, rz1 = z0 + half, z1 - half
    return {
        "right": [(x1, eave_y, z1), (x1, eave_y, z0), (xm, top, rz0), (xm, top, rz1)],
        "left": [(x0, eave_y, z0), (x0, eave_y, z1), (xm, top, rz1), (xm, top, rz0)],
        "front": [(x0, eave_y, z1), (x1, eave_y, z1), (xm, top, rz1)],
        "back": [(x1, eave_y, z0), (x0, eave_y, z0), (xm, top, rz0)],
        "ridge": ((xm, top, rz0), (xm, top, rz1)),
        "hips": [((x0, eave_y, z0), (xm, top, rz0)), ((x1, eave_y, z0), (xm, top, rz0)),
                 ((x0, eave_y, z1), (xm, top, rz1)), ((x1, eave_y, z1), (xm, top, rz1))],
    }


def clip(polygon, axis, limit, keep_above):
    """A convex polygon cut to coordinate `axis` >= limit (or <= when not keep_above)."""
    sign = 1 if keep_above else -1
    out = []
    n = len(polygon)
    for i in range(n):
        a, b = Vector(polygon[i]), Vector(polygon[(i + 1) % n])
        if sign * (a[axis] - limit) >= 0:
            out.append(a)
        if (a[axis] - limit) * (b[axis] - limit) < 0:
            t = (limit - a[axis]) / (b[axis] - a[axis])
            out.append(a.lerp(b, t))
    return out


def clip_z(polygon, z_min):
    """The wing's roof cut where it meets the house."""
    return clip(polygon, 2, z_min, True)


# The game's wall frames (house_layout.rs wall_frames): front +z along +x, back -z along
# -x, right +x along -z, left -x along +z, each u measured from the wall's middle.
WALL_FRAMES = {
    "front": Frame((0, 0, HALF_D), (1, 0, 0), (0, 0, 1)),
    "back": Frame((0, 0, -HALF_D), (-1, 0, 0), (0, 0, -1)),
    "right": Frame((HALF_W, 0, 0), (0, 0, -1), (1, 0, 0)),
    "left": Frame((-HALF_W, 0, 0), (0, 0, 1), (-1, 0, 0)),
}
WALL_PARTS = {"front": "FrontWall", "back": "BackWall", "right": "RightWall", "left": "LeftWall"}
WALL_HALF = {"front": HALF_W, "back": HALF_W, "right": HALF_D, "left": HALF_D}

# The structure's measures (house_layout.rs): an opening's column half-widths (a garage
# house's LOW_WINDOW_BAY, LOW_DOOR_BAY, GARAGE_BAY), the slab, and the low storeys'
# bands (Roof::ground_window, Roof::upper_window, Roof::belt_line): sills 0.5 and 2.1
# over the ground, the door's and the ground windows' heads level, the upper floor at 1.62.
WINDOW_BAY, DOOR_BAY, GARAGE_BAY = 0.5, 0.36, 0.7
SLAB = 0.12
DOOR_TOP = SLAB + 1.1
GARAGE_TOP = SLAB + 1.15
GARAGE_WIDTH = 4 * GARAGE_BAY - 0.2
FLOOR = 1.62
# What the model draws inside a window column: its frame's half width (a 0.7 m window,
# a real 1.3 m one) and its heights.
WINDOW_HALF = 0.35
GROUND_GLASS = (0.5, DOOR_TOP)
UPPER_GLASS = (2.1, 2.82)
# A front door's leaf (a real 0.9 x 2.05 m door) and the hole its casing leaves in the
# stucco, wide enough that a narrower lot's door column still finds it open.
DOOR_HALF = 0.25
DOOR_HOLE = 0.33


def plain_wall(side, plan, half_width=WINDOW_HALF, upper_half=None, shutters=(), span=None, extra_holes=()):
    """A flank or the back: the stucco slab with each planned window cut and dressed;
    `half_width` (a number, or a dict by centre) sizes the windows, `shutters` lists the
    centres hung with board shutters, `span` (u0, u1) shortens the slab and `extra_holes`
    are cut for what the caller dresses (a side door)."""
    frame, part, half = WALL_FRAMES[side], WALL_PARTS[side], WALL_HALF[side]
    plan = plan["walls"][side]

    def size(u, default):
        return default.get(u, WINDOW_HALF) if isinstance(default, dict) else default

    holes = []
    for u, kind in plan["ground"]:
        assert kind == "Window", f"{side}: a {kind} is drawn only on the front"
        w = size(u, half_width)
        holes.append((u - w, GROUND_GLASS[0], u + w, GROUND_GLASS[1]))
    for u in plan["upper"]:
        assert any(abs(u - g) < WINDOW_BAY for g, _ in plan["ground"]), f"{side}: upper window at {u} over no opening"
        w = size(u, upper_half if upper_half is not None else half_width)
        holes.append((u - w, UPPER_GLASS[0], u + w, UPPER_GLASS[1]))
    u0, u1 = span if span else (-half, half)
    wall(part, frame, u0, u1, 0, WALL_TOP, holes + list(extra_holes))
    for hole in holes:
        window(part, frame, *hole, mullion=hole[2] - hole[0] > 0.6)
        if (hole[0] + hole[2]) / 2 in shutters:
            for a in (hole[0] - SHUTTER_WIDTH - 0.02, hole[2] + 0.02):
                shutter(part, frame, a, hole[1] - 0.01, a + SHUTTER_WIDTH, hole[3] + 0.01)



# ---------------------------------------------------------------- export

# A house drawn in the street frame (front +z) whose game house stands with its door in
# its eave wall (a hip whose ridge runs along the street, `DoorWall::Eave`) is built a
# quarter turn round: the street's front is the build frame's +x wall, its right -z. Each
# part, and each wall's plan, is the turned one's; a wall's along axis turns with it, so
# its openings keep their centres.
ALONG_PARTS = {
    "FrontWall": "RightWall",
    "BackWall": "LeftWall",
    "RightWall": "BackWall",
    "LeftWall": "FrontWall",
    "FrontRoof": "RightRoof",
    "BackRoof": "LeftRoof",
    "RightRoof": "BackRoof",
    "LeftRoof": "FrontRoof",
}


def turned_along(geometry_by_key, plan):
    """The geometry and plan of a street-frame house turned into its build frame."""
    turned = defaultdict(lambda: ([], []))
    for key, (vertices, faces) in geometry_by_key.items():
        part, surface = key.split("__")
        target = turned[f"{ALONG_PARTS.get(part, part)}__{surface}"]
        base = len(target[0])
        target[0].extend(Vector((v[2], v[1], -v[0])) for v in vertices)
        target[1].extend([index + base for index in f] for f in faces)
    walls = plan["walls"]
    turned_plan = {
        **plan,
        "door_wall": 2,
        "w": plan["d"],
        "d": plan["w"],
        # the build frame's front, back, right and left walls: the street's left, right,
        # front and back
        "walls": {"front": walls["left"], "back": walls["right"], "right": walls["front"], "left": walls["back"]},
    }
    return turned, turned_plan


def export(plan, frame=None):
    """Write the model (.blend and .glb) and its plan, by the paths on the command line:
    -- <out.blend> <out.glb> [<out.plan.json>]. `frame` "along" turns a street-frame house
    into its eave-door build frame ([`turned_along`])."""
    argv = sys.argv[sys.argv.index("--") + 1 :]
    blend_path, glb_path = argv[0], argv[1]
    plan_path = argv[2] if len(argv) > 2 else None
    shapes = geometry
    if frame == "along":
        shapes, plan = turned_along(geometry, plan)


    def build_frame_to_blender(p):
        return (p[0], -p[2], p[1])


    bpy.ops.wm.read_factory_settings(use_empty=True)
    for key, (vertices, faces) in sorted(shapes.items()):
        mesh = bpy.data.meshes.new(key)
        mesh.from_pydata([build_frame_to_blender(v) for v in vertices], [], faces)
        mesh.validate()
        # metre UVs, projected on each face's dominant axis
        uv_layer = mesh.uv_layers.new(name="UVMap")
        for polygon in mesh.polygons:
            n = polygon.normal
            axis = max(range(3), key=lambda i: abs(n[i]))
            for loop_index in polygon.loop_indices:
                co = mesh.vertices[mesh.loops[loop_index].vertex_index].co
                a, b = [(1, 2), (0, 2), (0, 1)][axis]
                uv_layer.data[loop_index].uv = (co[a], co[b])
        obj = bpy.data.objects.new(key, mesh)
        bpy.context.scene.collection.objects.link(obj)

    triangles = sum(sum(len(f) - 2 for f in faces) for _, faces in shapes.values())
    print("objects", len(shapes), "triangles", triangles)
    by_part = defaultdict(int)
    for key, (_, faces) in shapes.items():
        by_part[key.split("__")[0]] += sum(len(f) - 2 for f in faces)
    print("by part", dict(by_part))

    bpy.ops.wm.save_as_mainfile(filepath=blend_path)
    bpy.ops.export_scene.gltf(
        filepath=glb_path,
        export_format="GLB",
        export_yup=True,
        export_apply=True,
        export_materials="NONE",
        export_normals=True,
        export_texcoords=True,
        export_vertex_color="NONE",
    )
    print("exported", glb_path)
    if plan_path:
        import json

        walls = [plan["walls"][side] for side in ("front", "back", "right", "left")]
        with open(plan_path, "w") as out:
            measures = {"w": plan["w"], "d": plan["d"], "wall": WALL_TOP, "ridge": RIDGE_Y, "garage": plan.get("garage", "right"), "door_wall": plan.get("door_wall", 0)}
            json.dump({**measures, "walls": walls}, out, indent=1)
            out.write("\n")
        print("plan", plan_path)


def arch_head(part, frame, u0, u1, spring, rise, depth=0.0, segments=6):
    """Stucco filling a rectangular hole's head down to a round arch: the hole runs up to
    `spring + rise`, the arch springs at `spring` from its sides and rises `rise` at its
    middle. Drawn on the wall's face (and its lining when `depth` is the slab's)."""
    mid, half = (u0 + u1) / 2, (u1 - u0) / 2
    top = spring + rise
    arc = [
        (mid - half * math.cos(math.pi * k / segments), spring + rise * math.sin(math.pi * k / segments))
        for k in range(segments + 1)
    ]
    for (ua, ya), (ub, yb) in zip(arc, arc[1:]):
        face(f"{part}__Siding", [frame.p(ua, ya), frame.p(ub, yb), frame.p(ub, top), frame.p(ua, top)], frame.out)
        # the arch's soffit, back to the reveal's depth
        face(f"{part}__Siding", [frame.p(ua, ya), frame.p(ua, ya, WALL), frame.p(ub, yb, WALL), frame.p(ub, yb)], Vector((0, -1, 0)))
        if depth:
            face(f"{part}__Lining", [frame.p(ua, ya, depth), frame.p(ua, top, depth), frame.p(ub, top, depth), frame.p(ub, yb, depth)], -frame.out)


def gable_roof(part, x0, x1, z_back, z_front, eave_y, pitch):
    """A front gable's two clay slopes over x0..x1 from z_back to z_front, its ridge along z;
    returns the apex height."""
    xc = (x0 + x1) / 2
    apex = eave_y + pitch * (x1 - x0) / 2
    tiled(part, Slope([(x0, eave_y, z_back), (x0, eave_y, z_front), (xc, apex, z_front), (xc, apex, z_back)]))
    tiled(part, Slope([(x1, eave_y, z_front), (x1, eave_y, z_back), (xc, apex, z_back), (xc, apex, z_front)]))
    ridge_cap(part, (xc, apex, z_front), (xc, apex, z_back))
    return apex


def gable_face(part, z, x0, x1, base, roof_x0, roof_x1, roof_eave, pitch, under=0.12):
    """A gable's stucco triangle over a wall top `base` at plane z, from x0 to x1, under a
    roof whose eaves run x0..x1 of `roof_x0..roof_x1` at `roof_eave`, lined inside."""
    xc = (roof_x0 + roof_x1) / 2
    half = (roof_x1 - roof_x0) / 2

    def y_at(x):
        return roof_eave + pitch * (half - abs(x - xc)) - under

    outline = [(x0, base, z), (x1, base, z), (x1, max(base, y_at(x1)), z), (xc, y_at(xc), z), (x0, max(base, y_at(x0)), z)]
    face(f"{part}__Siding", outline, (0, 0, 1))
    face(f"{part}__Lining", [(x, y, z - WALL) for x, y, _ in outline], (0, 0, -1))
    return y_at(xc)


def clay_vent(part, x, y, z, spacing=0.07):
    """Three clay pipe ends in a gable, in a triangle."""
    for dx, dy in ((-spacing / 2, 0), (spacing / 2, 0), (0, spacing * 0.85)):
        box(f"{part}__Terracotta", (x + dx - 0.022, y + dy - 0.022, z - 0.02), (x + dx + 0.022, y + dy + 0.022, z + 0.03))
        box(f"{part}__Dark", (x + dx - 0.012, y + dy - 0.012, z + 0.03), (x + dx + 0.012, y + dy + 0.012, z + 0.032))


# ---------------------------------------------------------------- street-frame pieces
# For houses drawn in the street frame (front +z, garage side +x) on the lot a model is
# drawn for (6.2 m front, 8 m deep, ridge 4.5 m).

# The main hip whose ridge runs along the street (exported `frame="along"`), as the game
# builds it on that lot: its eaves either way from the middle, its long slopes' pitch
# (the rise over half the depth), eave height and the ridge's half length (its hip ends
# steeper, house_layout.rs ALONG_STREET_HIP_END).
ALONG_EAVE = (EAVE_X, EAVE_Z)
ALONG_PITCH = (RIDGE_Y - WALL_TOP) / HALF_D
ALONG_EAVE_Y = WALL_TOP - ALONG_PITCH * OVERHANG
ALONG_RIDGE_HALF = EAVE_X - 1.8


def along_hip():
    """The main hip's four planes, eave edge first: the street slope, the back slope and the
    right and left hip ends."""
    ex, ez, ey, rh = ALONG_EAVE[0], ALONG_EAVE[1], ALONG_EAVE_Y, ALONG_RIDGE_HALF
    return {
        "front": [(-ex, ey, ez), (ex, ey, ez), (rh, RIDGE_Y, 0.0), (-rh, RIDGE_Y, 0.0)],
        "back": [(ex, ey, -ez), (-ex, ey, -ez), (-rh, RIDGE_Y, 0.0), (rh, RIDGE_Y, 0.0)],
        "right": [(ex, ey, ez), (ex, ey, -ez), (rh, RIDGE_Y, 0.0)],
        "left": [(-ex, ey, -ez), (-ex, ey, ez), (-rh, RIDGE_Y, 0.0)],
    }


def along_hip_roof(flat=False):
    """The main hip whole, with its ridge and hip caps."""
    hip = along_hip()
    tiled("FrontRoof", Slope(hip["front"]), flat=flat)
    tiled("BackRoof", Slope(hip["back"]), flat=flat)
    tiled("RightRoof", Slope(hip["right"]), flat=flat)
    tiled("LeftRoof", Slope(hip["left"]), flat=flat)
    ridge_cap("FrontRoof", (-ALONG_RIDGE_HALF, RIDGE_Y, 0.0), (ALONG_RIDGE_HALF, RIDGE_Y, 0.0))
    for cx, cz in ((1, 1), (1, -1), (-1, 1), (-1, -1)):
        corner = (cx * ALONG_EAVE[0], ALONG_EAVE_Y, cz * ALONG_EAVE[1])
        ridge_cap("RightRoof" if cx > 0 else "LeftRoof", corner, (cx * ALONG_RIDGE_HALF, RIDGE_Y, 0.0))
    return hip


def front_frame():
    """The front wall's frame, u from the left corner (x + HALF_W)."""
    return Frame((-HALF_W, 0, HALF_D), (1, 0, 0), (0, 0, 1))


def fx(x):
    """The front wall's u of x."""
    return x + HALF_W


def garage_opening(garage_u):
    """The garage door's opening (x0, y0, x1, y1): its 2.6 m door on the slab."""
    return (garage_u - GARAGE_WIDTH / 2, SLAB, garage_u + GARAGE_WIDTH / 2, GARAGE_TOP)


def door_hole(door_u):
    """The front door's hole in the stucco (x0, y0, x1, y1), its casing inside it."""
    return (door_u - DOOR_HOLE, SLAB, door_u + DOOR_HOLE, DOOR_TOP + 0.05)


def front_door(front, door_u, part="FrontWall", glass=False):
    """The front door in its hole: a painted leaf in two raised panels (or a lite over
    the lower one), its casing filling the hole round it, a knob."""
    a, b = fx(door_u - DOOR_HALF), fx(door_u + DOOR_HALF)
    h0, h1 = fx(door_u - DOOR_HOLE), fx(door_u + DOOR_HOLE)
    front.quad(f"{part}__Accent", a, SLAB, b, DOOR_TOP, 0.12)
    for y0, y1 in ((SLAB + 0.08, SLAB + 0.48), (SLAB + 0.56, DOOR_TOP - 0.08)):
        if glass and y0 > SLAB + 0.3:
            front.quad(f"{part}__Glass", a + 0.07, y0, b - 0.07, y1, 0.115)
        else:
            front.block(f"{part}__Accent", a + 0.06, y0, b - 0.06, y1, 0.12, 0.105)
    front.block(f"{part}__Trim", h0, SLAB, a, DOOR_TOP + 0.05, 0.12, 0.08)
    front.block(f"{part}__Trim", b, SLAB, h1, DOOR_TOP + 0.05, 0.12, 0.08)
    front.block(f"{part}__Trim", a, DOOR_TOP, b, DOOR_TOP + 0.05, 0.12, 0.08)
    front.block(f"{part}__Iron", b - 0.07, SLAB + 0.5, b - 0.045, SLAB + 0.56, 0.105, 0.08)


def sectional_garage_door(front, garage_u, garage_bay=GARAGE_BAY, top=GARAGE_TOP, panels=4, boards=True, decor=None, header=True):
    """A one-piece sectional roll-up garage door set back in its opening: one door of
    `panels` horizontal sections (thin joints across it), its face in vertical boards when
    `boards`, and its `decor`: "handles" (a pair of small fake pulls at the middle),
    "straps" (strap hinges at both ends of each section), "lites" (a row of small windows
    in the top section) or None; a dark header board over it (`header`), else a trim
    head. Returns the opening (x0, y0, x1, y1)."""
    x0, _, x1, _ = garage_opening(garage_u)
    g0, g1 = fx(x0), fx(x1)
    front.quad("FrontWall__GarageDoor", g0, SLAB, g1, top, 0.12)
    if boards:
        u = g0 + 0.11
        while u < g1 - 0.05:
            front.block("FrontWall__GarageDoor", u - 0.007, SLAB + 0.02, u + 0.007, top - 0.02, 0.12, 0.112)
            u += 0.11
    for k in range(1, panels):
        y = SLAB + (top - SLAB) * k / panels
        front.block("FrontWall__Dark", g0, y - 0.005, g1, y + 0.005, 0.12, 0.109)
    mid = (g0 + g1) / 2
    if decor == "handles":
        for handle in (mid - 0.09, mid + 0.09):
            front.block("FrontWall__Iron", handle - 0.008, SLAB + 0.42, handle + 0.008, SLAB + 0.58, 0.105, 0.085)
    elif decor == "straps":
        for k in range(panels):
            y = SLAB + (top - SLAB) * (k + 0.5) / panels
            for a, b in ((g0 + 0.06, g0 + 0.36), (g1 - 0.36, g1 - 0.06)):
                front.block("FrontWall__Iron", a, y - 0.018, b, y + 0.018, 0.108, 0.098)
    elif decor == "lites":
        y0 = SLAB + (top - SLAB) * (panels - 1) / panels + 0.05
        y1 = top - 0.05
        count = 8
        width = (g1 - g0) / count
        for k in range(count):
            a = g0 + k * width + 0.04
            front.quad("FrontWall__Glass", a, y0, a + width - 0.08, y1, 0.105)
    if header:
        front.block("FrontWall__Wood", g0 - 0.06, top + 0.03, g1 + 0.06, top + 0.09, 0.0, -0.025)
    else:
        front.block("FrontWall__Trim", g0 - 0.04, top, g1 + 0.04, top + 0.04, 0.0, -0.02)
    return (x0, SLAB, x1, top)


def lantern(front, x, y=1.05):
    """A wall lantern beside the garage door (a small dark box and its glass)."""
    front.block("FrontWall__Iron", fx(x) - 0.045, y, fx(x) + 0.045, y + 0.16, 0.0, -0.08)
    front.block("FrontWall__Lamp", fx(x) - 0.035, y + 0.02, fx(x) + 0.035, y + 0.13, -0.005, -0.075)


def lattice_vent(part, frame, u, y, size=0.16):
    """A small square lattice vent in a gable: a dark square crossed by a trim X."""
    frame.quad(f"{part}__Dark", u - size / 2, y - size / 2, u + size / 2, y + size / 2, -0.003)
    frame.block(f"{part}__Trim", u - size / 2 - 0.02, y - size / 2 - 0.02, u + size / 2 + 0.02, y - size / 2, 0.0, -0.015)
    frame.block(f"{part}__Trim", u - size / 2 - 0.02, y + size / 2, u + size / 2 + 0.02, y + size / 2 + 0.02, 0.0, -0.015)
    frame.block(f"{part}__Trim", u - 0.008, y - size / 2, u + 0.008, y + size / 2, 0.0, -0.012)
    frame.block(f"{part}__Trim", u - size / 2, y - 0.008, u + size / 2, y + 0.008, 0.0, -0.012)


# A box bay's depth out of the upper floor and its foot and head (m up the wall): it
# stands over the garage door's head trim, its windows at the upper glass.
BAY_OUT = 0.25
BAY_LO, BAY_HI = 1.5, 2.95


def box_bay(front, x0, x1, flat=True, out=BAY_OUT, lo=BAY_LO, hi=BAY_HI):
    """A dark stained box bay standing out of the upper floor over x0..x1: board and batten
    under two windows, brackets, a tile hood. The caller cuts its opening in the wall."""
    b0, b1 = fx(x0), fx(x1)
    g0, g1 = UPPER_GLASS[0], hi - 0.06
    for a, c, b, d in ((b0, lo, b1, lo + 0.06), (b0, hi - 0.04, b1, hi), (b0, lo, b0 + 0.05, hi), (b1 - 0.05, lo, b1, hi)):
        front.block("FrontWall__Wood", a, c, b, d, 0.0, -out)
    front.block("FrontWall__Wood", b0, lo, b1, g0 - 0.02, -out + 0.03, -out)
    u = b0 + 0.1
    while u < b1 - 0.05:
        front.block("FrontWall__Wood", u - 0.01, lo + 0.02, u + 0.01, g0 - 0.02, -out, -out - 0.015)
        u += 0.11
    mid = (b0 + b1) / 2
    for wu0, wu1 in ((b0 + 0.06, mid - 0.025), (mid + 0.025, b1 - 0.06)):
        front.quad("FrontWall__Curtain", wu0, g0, wu1, g1, -out + 0.03)
        slats = max(2, round((g1 - g0) / LOUVRE))
        for k in range(1, slats):
            y = g0 + (g1 - g0) * k / slats
            front.quad("FrontWall__Glass", wu0 + FRAME, y - 0.004, wu1 - FRAME, y + 0.004, -out + 0.025)
        for a, c, b, d in ((wu0, g0, wu1, g0 + FRAME), (wu0, g1 - FRAME, wu1, g1), (wu0, g0, wu0 + FRAME, g1), (wu1 - FRAME, g0, wu1, g1)):
            front.block("FrontWall__Trim", a, c, b, d, -out + 0.03, -out + 0.005)
    front.block("FrontWall__Wood", mid - 0.025, g0 - 0.02, mid + 0.025, hi, -out + 0.02, -out - 0.01)
    for su, sign in ((b0, -1), (b1, 1)):
        face("FrontWall__Wood", [front.p(su, lo, 0), front.p(su, hi, 0), front.p(su, hi, -out), front.p(su, lo, -out)], front.along * sign)
    for bu in (b0 + 0.1, mid, b1 - 0.1):
        front.block("FrontWall__Wood", bu - 0.03, lo - 0.16, bu + 0.03, lo, 0.0, -out + 0.03)
    hood = HALF_D + out + 0.1
    rise = PITCH * (out + 0.1)
    tiled("FrontWall", Slope([(x0 - 0.08, hi + 0.02, hood), (x1 + 0.08, hi + 0.02, hood), (x1 + 0.08, hi + 0.02 + rise, HALF_D), (x0 - 0.08, hi + 0.02 + rise, HALF_D)]), flat=flat)


def board_shutters(front, u0, u1, y0, y1, width=None):
    """A board shutter each side of a window from u0 to u1."""
    width = SHUTTER_WIDTH if width is None else width
    for a, b in ((u0 - width - 0.02, u0 - 0.02), (u1 + 0.02, u1 + width + 0.02)):
        front.block("FrontWall__Accent", a, y0, b, y1, 0.0, -0.025)
        for k in range(1, 3):
            y = y0 + (y1 - y0) * k / 3
            front.block("FrontWall__Accent", a + 0.015, y - 0.012, b - 0.015, y + 0.012, -0.025, -0.035)


def porch_post(x, z, top, pier=0.48, stone=True):
    """A chunky dark timber porch post (real 0.3 m) on a stacked-stone (or brick) pier
    (real 0.7 m square, 0.9 m high)."""
    surface = "Stone" if stone else "Brick"
    box(f"FrontWall__{surface}", (x - 0.19, 0.08, z - 0.19), (x + 0.19, pier, z + 0.19))
    box("FrontWall__Concrete", (x - 0.21, pier, z - 0.21), (x + 0.21, pier + 0.04, z + 0.21))
    box("FrontWall__Wood", (x - 0.08, pier + 0.04, z - 0.08), (x + 0.08, top, z + 0.08))


# ---------------------------------------------------------------- a front-to-back hip
# The main hip whose ridge runs front to back (the door in a hip end, the game's
# `DoorWall::Gable` hips), optionally with a two-storey gable over one side of the front
# (and of the back) whose right slope runs on in the main right slope's plane and whose
# left slope meets the hip end in a valley: the tract's garage gable.


def main_hip():
    """The front-to-back hip's planes over the lot, eave edge first."""
    return hip_slopes(-EAVE_X, EAVE_X, -EAVE_Z, EAVE_Z, EAVE_Y, PITCH, along_x=False)


def hip_with_gables(gable_x, front=True, back=True, flat=False):
    """The main hip with a gable end at the front and/or the back over x from
    `2 * gable_x - EAVE_X` to the right eave, its ridge at x = gable_x; returns each
    gable's apex height."""
    hip = main_hip()
    apex = EAVE_Y + PITCH * (EAVE_X - gable_x)
    # where the gable's ridge crosses the hip line from the ridge's end to the right
    # eave corner
    hip_z = RIDGE_HALF + gable_x / EAVE_X * (EAVE_Z - RIDGE_HALF)
    left_eave = 2 * gable_x - EAVE_X
    tiled("RightRoof", Slope(hip["right"]), flat=flat)
    tiled("LeftRoof", Slope(hip["left"]), flat=flat)
    ridge_cap("LeftRoof", (0.0, RIDGE_Y, -RIDGE_HALF), (0.0, RIDGE_Y, RIDGE_HALF))
    for sign, wanted in ((1, front), (-1, back)):
        part = "FrontRoof" if sign > 0 else "BackRoof"
        corner_l = (-EAVE_X, EAVE_Y, sign * EAVE_Z)
        corner_r = (EAVE_X, EAVE_Y, sign * EAVE_Z)
        ridge_end = (0.0, RIDGE_Y, sign * RIDGE_HALF)
        tiled_end = hip["front"] if sign > 0 else hip["back"]
        if not wanted:
            tiled(part, Slope(tiled_end), flat=flat)
            ridge_cap(part, corner_l, ridge_end)
            ridge_cap(part, corner_r, ridge_end)
            continue
        meet = (gable_x, apex, sign * hip_z)
        rake = (gable_x, apex, sign * EAVE_Z)
        valley_foot = (left_eave, EAVE_Y, sign * EAVE_Z)
        # the right slope run on to the rake (in its plane, where the game's hip end
        # stands, so in the end's part), the gable's left slope and the hip end left of
        # the valley
        tiled(part, Slope([rake, meet, corner_r]), flat=flat)
        tiled(part, Slope([rake, meet, valley_foot]), flat=flat)
        tiled(part, Slope([corner_l, valley_foot, meet, ridge_end] if sign > 0 else [valley_foot, corner_l, ridge_end, meet]), flat=flat)
        ridge_cap(part, rake, meet)
        ridge_cap(part, corner_l, ridge_end)
        ridge_cap(part, valley_foot, meet, half=0.04, lift=0.01)
        ridge_cap(part, meet, ridge_end)
    return apex


def gable_end_wall(part, z, x0, x1, gable_x, outward=1):
    """The stucco triangle (and lining) of a gable end over the wall top at plane z, from
    x0 to x1, under a gable roof centred on gable_x whose right eave is the main one;
    returns its peak's height."""

    def y_at(x):
        return EAVE_Y + PITCH * (EAVE_X - gable_x - abs(x - gable_x)) - DECK * 1.1

    outline = [(x0, WALL_TOP), (x1, WALL_TOP), (x1, max(WALL_TOP, y_at(x1))), (gable_x, y_at(gable_x)), (x0, max(WALL_TOP, y_at(x0)))]
    face(f"{part}__Siding", [(x, y, z) for x, y in outline], (0, 0, outward))
    face(f"{part}__Lining", [(x, y, z - outward * WALL) for x, y in outline], (0, 0, -outward))
    return y_at(gable_x)


# ---------------------------------------------------------------- the entry
# Every home of the tract steps down at the front to one storey on the half away from the
# garage: an office projecting from the front wall beside the garage (one window to the
# street, a lean-to roof leaning on the wall under the upper windows) and, along its
# outer side, the entry: a few steps up and a passage beside the office's side wall (its
# slit windows) to the front door, set back in the main wall facing the street, under a
# small cover of its own, lower than the office's roof, on a post or two at its outer
# corner (real 5.3 x 3 m in all). Drawn in the street frame as the front wall's, so it
# breaks with that wall.
OFFICE_SPAN = (-1.65, -0.3)  # the office's x: its inner side by the garage gable's edge
PASSAGE_X = (-HALF_W, OFFICE_SPAN[0])  # the entry passage's
ENTRY_DOOR = -2.2  # the front door's centre at the passage's end: its hole spans the
# coded ground window's centre (-1.91), which the passage's door stands for
OFFICE_DEPTH = 1.5
OFFICE_TOP = 2.05  # where its roof meets the wall, under the upper sills
OFFICE_EAVE = 1.72
OFFICE_OVER = 0.25
COVER_TOP, COVER_EAVE = 1.86, 1.56  # the entry cover's, under the office's eave line
PASSAGE_FLOOR = 0.24  # two risers up from the walk
BAY_FLOOR = 0.36  # a porch bay's: four risers up from the walk (real 18 cm)
PIER, PIER_HEIGHT = 0.26, 0.48  # the stone piers' square plan and height, under their caps
PLANTER_HEIGHT = 0.28  # the planter's low wall
OFFICE_PITCH = 0.3  # a small gable's or hip's


def _move_faces(source, target, start):
    """The faces of `source` from index `start` on, moved to `target`."""
    vertices, faces = geometry[source]
    moved = faces[start:]
    del faces[start:]
    out = geometry[target]
    for f in moved:
        first = len(out[0])
        out[0].extend(vertices[i] for i in f)
        out[1].append(list(range(first, first + len(f))))


def entry_holes(wall_z=HALF_D):
    """The holes the entry needs in the front wall: the front door at the passage's end
    (x0, y0, x1, y1)."""
    return [(ENTRY_DOOR - DOOR_HOLE, SLAB, ENTRY_DOOR + DOOR_HOLE, PASSAGE_FLOOR + 1.1 + 0.05)]


def office_entry(wall_z=HALF_D, office_roof="shed", cover="shed", supports="posts", flat=False,
                 finish="Siding", window_style="header", popout=True, slits=2, inner_slit=True,
                 steps_piers=False, planter=False, top=OFFICE_TOP, vent=False, bay=False):
    """The office and the entry beside it, out from the wall at `wall_z`: the office's roof
    a "shed" (one plane leaning on the wall, exposed rafter tails and a dark fascia at its
    eave), a small front "gable" or a "hip"; its street window under a dark "header", with
    board "shutters" or "arched", over a brick rowlock sill on a battered stucco apron
    (`popout`); `slits` narrow windows in its outer side wall over the passage and one in
    its inner side wall (`inner_slit`); the entry's `cover` a "shed" or a small street
    "gable" (a pediment), carried by dark square "posts" on short bases, Craftsman
    "tapered" posts on stone piers or stucco "arches"; the front door in the wall at the
    passage's end, steps up to it, stacked-stone piers by them (`steps_piers`) and a low
    stone planter before the office (`planter`). Its stucco is in `finish`. With `bay`
    (a shed only) the office's roof runs on over the passage as an open porch bay to one
    dark post on a short tapered base at its outer front corner, straight steps rise to it
    and the way in runs on past the front corner to a door in the side wall
    ([`side_entrance`]): no cover and no front door. Returns its measures."""
    ox0, ox1 = OFFICE_SPAN
    px0, px1 = PASSAGE_X
    zf = wall_z + OFFICE_DEPTH
    skin = f"FrontWall__{finish}"

    def slab(frame, length, height, holes):
        """A slab of the office's, stucco or its finish."""
        start = len(geometry["FrontWall__Siding"][1])
        wall("FrontWall", frame, 0, length, 0, height, holes)
        if finish != "Siding":
            _move_faces("FrontWall__Siding", skin, start)

    # the office's roof: its height over a point, for its walls' tops
    xa, xb, ze = ox0 - OFFICE_OVER, ox1 + 0.02, zf + OFFICE_OVER
    if bay:
        xa = px0 - 0.12
    floor = BAY_FLOOR if bay else PASSAGE_FLOOR
    if office_roof == "shed":
        def roof_y(x, z):
            return OFFICE_EAVE + (top - OFFICE_EAVE) * (ze - z) / (ze - wall_z) - DECK
    elif office_roof == "gable":
        xc = (ox0 + ox1) / 2

        def roof_y(x, z):
            return OFFICE_EAVE + OFFICE_PITCH * ((xb - xa) / 2 - abs(x - xc)) - DECK
    else:
        def roof_y(x, z):
            return OFFICE_EAVE - DECK
    wall_top = OFFICE_EAVE - 0.08

    # its street face and window
    width = ox1 - ox0
    front = Frame((ox0, 0, zf), (1, 0, 0), (0, 0, 1))
    w0, w1 = width / 2 - 0.31, width / 2 + 0.31
    head = DOOR_TOP + 0.05
    slab(front, width, wall_top, [(w0, GROUND_GLASS[0], w1, head)])
    window("FrontWall", front, w0, GROUND_GLASS[0], w1, head, mullion=False)
    # double-hung: the sashes' meeting rail
    front.block("FrontWall__Trim", w0, (GROUND_GLASS[0] + head) / 2 - 0.015, w1, (GROUND_GLASS[0] + head) / 2 + 0.015, 0.1, 0.07)
    if window_style == "header":
        front.block("FrontWall__Wood", w0 - 0.12, head + 0.06, w1 + 0.12, head + 0.13, 0.0, -0.025)
    elif window_style == "shutters":
        for a in (w0 - SHUTTER_WIDTH - 0.02, w1 + 0.02):
            shutter("FrontWall", front, a, GROUND_GLASS[0] - 0.01, a + SHUTTER_WIDTH, head + 0.01)
    elif window_style == "arched":
        arch_head("FrontWall", front, w0, w1, head - 0.18, 0.18, depth=WALL)
    if popout:
        # a brick rowlock sill over a battered stucco apron flaring to the ground
        front.block("FrontWall__Brick", w0 - 0.14, GROUND_GLASS[0] - 0.07, w1 + 0.14, GROUND_GLASS[0], 0.0, -0.13)
        a, b = w0 - 0.12, w1 + 0.12
        top_y, out_top, out_foot = GROUND_GLASS[0] - 0.07, 0.11, 0.2
        face(skin, [front.p(a, 0, -out_foot), front.p(b, 0, -out_foot), front.p(b, top_y, -out_top), front.p(a, top_y, -out_top)], (0, 0.3, 1))
        for u, sign in ((a, -1), (b, 1)):
            face(skin, [front.p(u, 0, 0), front.p(u, 0, -out_foot), front.p(u, top_y, -out_top), front.p(u, top_y, 0)], front.along * sign)
    # its side walls, closed up to its roof
    for x, outward, count in ((ox0, -1, slits), (ox1, 1, 1 if inner_slit else 0)):
        frame = Frame((x, 0, wall_z), (0, 0, 1), (-1, 0, 0)) if outward < 0 else Frame((x, 0, zf), (0, 0, -1), (1, 0, 0))
        holes = []
        for k in range(count):
            mid = OFFICE_DEPTH * (k + 1) / (count + 1)
            holes.append((mid - 0.08, GROUND_GLASS[0], mid + 0.08, head))
        slab(frame, OFFICE_DEPTH, wall_top, holes)
        for hole in holes:
            window("FrontWall", frame, *hole, mullion=False)
        gable = [(x, wall_top, wall_z), (x, wall_top, zf), (x, roof_y(x, zf), zf), (x, roof_y(x, wall_z), wall_z)]
        if roof_y(x, wall_z) > wall_top + 0.01:
            face(skin, gable, (outward, 0, 0))
    if office_roof == "shed":
        tiled("FrontWall", Slope([(xa, OFFICE_EAVE, ze), (xb, OFFICE_EAVE, ze), (xb, top, wall_z), (xa, top, wall_z)]), flat=flat)
        # the dark fascia and the exposed rafter tails under the eave
        fall = (top - OFFICE_EAVE) / (ze - wall_z)
        box("FrontWall__Wood", (xa, OFFICE_EAVE - DECK - 0.05, ze - 0.035), (xb, OFFICE_EAVE - DECK + 0.01, ze + 0.005))
        x = xa + 0.12
        while x < xb - 0.05:
            y0 = OFFICE_EAVE - DECK - 0.05
            box("FrontWall__Wood", (x - 0.025, y0, zf - 0.02), (x + 0.025, y0 + 0.05 + fall * (ze - zf), ze - 0.04))
            x += 0.4
    elif office_roof == "gable":
        apex = OFFICE_EAVE + OFFICE_PITCH * (xb - xa) / 2
        tiled("FrontWall", Slope([(xa, OFFICE_EAVE, wall_z), (xa, OFFICE_EAVE, ze), (xc, apex, ze), (xc, apex, wall_z)]), flat=flat)
        tiled("FrontWall", Slope([(xb, OFFICE_EAVE, ze), (xb, OFFICE_EAVE, wall_z), (xc, apex, wall_z), (xc, apex, ze)]), flat=flat)
        ridge_cap("FrontWall", (xc, apex, ze), (xc, apex, wall_z))
        outline = [(ox0, wall_top), (ox1, wall_top), (ox1, roof_y(ox1, zf)), (xc, roof_y(xc, zf)), (ox0, roof_y(ox0, zf))]
        face(skin, [(x, y, zf) for x, y in outline], (0, 0, 1))
        face("FrontWall__Lining", [(x, y, zf - WALL) for x, y in outline], (0, 0, -1))
        if vent:
            clay_vent("FrontWall", xc, roof_y(xc, zf) - 0.14, zf)
    else:
        rise = OFFICE_PITCH * (xb - xa) / 2
        end = (xb - xa) / 2
        ridge_z = max(wall_z, ze - end)
        hip_top = OFFICE_EAVE + rise
        tiled("FrontWall", Slope([(xa, OFFICE_EAVE, ze), (xb, OFFICE_EAVE, ze), ((xa + xb) / 2, hip_top, ridge_z)]), flat=flat)
        tiled("FrontWall", Slope([(xa, OFFICE_EAVE, wall_z), (xa, OFFICE_EAVE, ze), ((xa + xb) / 2, hip_top, ridge_z), ((xa + xb) / 2, hip_top, wall_z)]), flat=flat)
        tiled("FrontWall", Slope([(xb, OFFICE_EAVE, ze), (xb, OFFICE_EAVE, wall_z), ((xa + xb) / 2, hip_top, wall_z), ((xa + xb) / 2, hip_top, ridge_z)]), flat=flat)
        ridge_cap("FrontWall", (xa, OFFICE_EAVE, ze), ((xa + xb) / 2, hip_top, ridge_z))
        ridge_cap("FrontWall", (xb, OFFICE_EAVE, ze), ((xa + xb) / 2, hip_top, ridge_z))
        ridge_cap("FrontWall", ((xa + xb) / 2, hip_top, ridge_z), ((xa + xb) / 2, hip_top, wall_z))

    # the entry: its floor, the steps up to it, the front door at its end
    box("FrontWall__Concrete", (px0, 0.0, wall_z), (px1, floor, zf))
    risers = round(floor / 0.1)
    for k in range(risers):
        box("FrontWall__Concrete", (px0 + PIER + 0.02, 0.0, zf), (px1 - PIER / 2 - 0.04, floor - floor / risers * k, zf + 0.2 * (k + 1)))
    door = Frame((-HALF_W, 0, wall_z), (1, 0, 0), (0, 0, 1))
    a, b = fx(ENTRY_DOOR - DOOR_HALF), fx(ENTRY_DOOR + DOOR_HALF)
    door_drawn = not bay
    h0, h1 = fx(ENTRY_DOOR - DOOR_HOLE), fx(ENTRY_DOOR + DOOR_HOLE)
    leaf_top = PASSAGE_FLOOR + 1.1
    if door_drawn:
        door.quad("FrontWall__Accent", a, SLAB, b, leaf_top, 0.12)
    if door_drawn:
        door.block("FrontWall__Accent", a + 0.06, PASSAGE_FLOOR + 0.56, b - 0.06, leaf_top - 0.08, 0.12, 0.105)
    if door_drawn:
        door.block("FrontWall__Accent", a + 0.06, PASSAGE_FLOOR + 0.08, b - 0.06, PASSAGE_FLOOR + 0.48, 0.12, 0.105)
    if door_drawn:
        door.block("FrontWall__Trim", h0, SLAB, a, leaf_top + 0.05, 0.12, 0.08)
    if door_drawn:
        door.block("FrontWall__Trim", b, SLAB, h1, leaf_top + 0.05, 0.12, 0.08)
    if door_drawn:
        door.block("FrontWall__Trim", a, leaf_top, b, leaf_top + 0.05, 0.12, 0.08)
    if door_drawn:
        door.block("FrontWall__Iron", b - 0.07, PASSAGE_FLOOR + 0.5, b - 0.045, PASSAGE_FLOOR + 0.56, 0.105, 0.08)
    # stacked-stone piers, all one size under pale caps: either side of the steps (`steps_piers`)
    # and, with the planter (`planter`), one at its far end by the driveway, the planter's
    # low wall curving out between the second and the third
    pier_z = zf + 0.62

    def stone_pier(x):
        box("FrontWall__Stone", (x - PIER / 2, 0.0, pier_z - PIER / 2), (x + PIER / 2, PIER_HEIGHT, pier_z + PIER / 2))
        box("FrontWall__Concrete", (x - PIER / 2 - 0.02, PIER_HEIGHT, pier_z - PIER / 2 - 0.02), (x + PIER / 2 + 0.02, PIER_HEIGHT + 0.04, pier_z + PIER / 2 + 0.02))

    by_steps = (px0 + 0.12, px1 - 0.02)
    if steps_piers:
        for x in by_steps:
            stone_pier(x)
    if planter:
        start, end = by_steps[1], ox1 - PIER / 2
        stone_pier(end)
        bow = 0.16
        points = [(start + (end - start) * k / 3, pier_z + bow * math.sin(math.pi * k / 3)) for k in range(4)]
        for (xa, za), (xb, zb) in zip(points, points[1:]):
            a, b = Vector((xa, 0, za)), Vector((xb, 0, zb))
            across = Vector((-(zb - za), 0, xb - xa)).normalized() * 0.05
            for y0, y1, half, surface in ((0.0, PLANTER_HEIGHT, 0.05, "Stone"), (PLANTER_HEIGHT, PLANTER_HEIGHT + 0.03, 0.07, "Concrete")):
                side = across * (half / 0.05)
                ring = [a - side, b - side, b + side, a + side]
                face(f"FrontWall__{surface}", [(p.x, y1, p.z) for p in ring], (0, 1, 0))
                # the segments' long faces (their ends meet their neighbours')
                for p0, p1 in (ring[0:2], ring[2:4]):
                    out = ((p0 + p1) / 2 - (a + b) / 2)
                    face(f"FrontWall__{surface}", [(p0.x, y0, p0.z), (p1.x, y0, p1.z), (p1.x, y1, p1.z), (p0.x, y1, p0.z)], (out.x, 0, out.z))

    if bay:
        # the office's roof run on over the bay to one dark post at its outer front corner,
        # a beam along its front and back to the wall, heavy brackets at the office
        post_x, post_z = px0 + 0.12, zf - 0.08
        beam_y = OFFICE_EAVE - DECK - 0.05
        box("FrontWall__Concrete", (post_x - 0.13, floor, post_z - 0.13), (post_x + 0.13, floor + 0.05, post_z + 0.13))
        corners = lambda h, y: [(post_x - h, y, post_z - h), (post_x + h, y, post_z - h), (post_x + h, y, post_z + h), (post_x - h, y, post_z + h)]  # noqa: E731
        foot, head_ring = corners(0.12, floor + 0.05), corners(0.08, floor + 0.3)
        for k in range(4):
            mid = (Vector(foot[k]) + Vector(foot[(k + 1) % 4])) / 2
            face("FrontWall__Wood", [foot[k], foot[(k + 1) % 4], head_ring[(k + 1) % 4], head_ring[k]], (mid.x - post_x, 0, mid.z - post_z))
        box("FrontWall__Wood", (post_x - 0.07, floor + 0.3, post_z - 0.07), (post_x + 0.07, beam_y - 0.12, post_z + 0.07))
        box("FrontWall__Wood", (xa + 0.05, beam_y - 0.12, post_z - 0.07), (ox0, beam_y, post_z + 0.07))
        box("FrontWall__Wood", (post_x - 0.07, beam_y - 0.12, wall_z), (post_x + 0.07, beam_y, post_z))
        for x0, x1 in ((post_x + 0.07, post_x + 0.3), (ox0 - 0.23, ox0)):
            box("FrontWall__Wood", (x0, beam_y - 0.32, post_z - 0.05), (x1, beam_y - 0.12, post_z + 0.05))
        return {"office": (ox0, ox1), "passage": (px0, px1), "front": zf}

    # the entry's cover: lower than the office's roof, out to the street end
    cx0, cx1, cze = px0 - 0.12, ox0, zf + 0.15
    post_x, post_z = px0 + 0.14, zf - 0.04
    if cover == "gable":
        cc = (cx0 + cx1) / 2
        apex = COVER_EAVE + 0.42 * (cx1 - cx0) / 2
        tiled("FrontWall", Slope([(cx0, COVER_EAVE, wall_z), (cx0, COVER_EAVE, cze), (cc, apex, cze), (cc, apex, wall_z)]), flat=flat)
        tiled("FrontWall", Slope([(cx1, COVER_EAVE, cze), (cx1, COVER_EAVE, wall_z), (cc, apex, wall_z), (cc, apex, cze)]), flat=flat)
        ridge_cap("FrontWall", (cc, apex, cze), (cc, apex, wall_z))
        # its pediment to the street, over a beam
        pz = cze - 0.12
        face(skin, [(cx0 + 0.12, COVER_EAVE - 0.06, pz), (cx1, COVER_EAVE - 0.06, pz), (cc, apex - 0.12, pz)], (0, 0, 1))
        beam_y = COVER_EAVE - 0.06
    else:
        tiled("FrontWall", Slope([(cx0, COVER_EAVE, cze), (cx1, COVER_EAVE, cze), (cx1, COVER_TOP, wall_z), (cx0, COVER_TOP, wall_z)]), flat=flat)
        beam_y = COVER_EAVE - 0.03
    if supports == "arches":
        # a stucco arch to the street and one in the passage's outer side
        spring = PASSAGE_FLOOR + 1.0
        street = Frame((px0, 0, zf), (1, 0, 0), (0, 0, 1))
        span = px1 - px0
        wall("FrontWall", street, 0, span, 0, beam_y, [(0.22, PASSAGE_FLOOR, span - 0.05, spring + 0.2)])
        arch_head("FrontWall", street, 0.22, span - 0.05, spring, 0.2, depth=WALL)
        side = Frame((px0, 0, wall_z), (0, 0, 1), (-1, 0, 0))
        wall("FrontWall", side, 0, OFFICE_DEPTH, 0, beam_y, [(0.3, PASSAGE_FLOOR, OFFICE_DEPTH - 0.25, spring + 0.2)])
        arch_head("FrontWall", side, 0.3, OFFICE_DEPTH - 0.25, spring, 0.2, depth=WALL)
    else:
        if supports == "tapered":
            box("FrontWall__Stone", (post_x - 0.15, 0.0, post_z - 0.15), (post_x + 0.15, 0.62, post_z + 0.15))
            box("FrontWall__Concrete", (post_x - 0.17, 0.62, post_z - 0.17), (post_x + 0.17, 0.66, post_z + 0.17))
            # a post tapering from 0.22 at its foot to 0.13 under the beam
            bottom, top_y = 0.66, beam_y - 0.12
            lo, hi = 0.11, 0.065
            corners = lambda h, y: [(post_x - h, y, post_z - h), (post_x + h, y, post_z - h), (post_x + h, y, post_z + h), (post_x - h, y, post_z + h)]  # noqa: E731
            foot, head_ring = corners(lo, bottom), corners(hi, top_y)
            for k in range(4):
                mid = (Vector(foot[k]) + Vector(foot[(k + 1) % 4])) / 2
                face("FrontWall__Wood", [foot[k], foot[(k + 1) % 4], head_ring[(k + 1) % 4], head_ring[k]], (mid.x - post_x, 0, mid.z - post_z))
            face("FrontWall__Wood", head_ring, (0, 1, 0))
        else:
            box("FrontWall__Concrete", (post_x - 0.1, PASSAGE_FLOOR, post_z - 0.1), (post_x + 0.1, PASSAGE_FLOOR + 0.16, post_z + 0.1))
            box("FrontWall__Wood", (post_x - 0.07, PASSAGE_FLOOR + 0.16, post_z - 0.07), (post_x + 0.07, beam_y - 0.12, post_z + 0.07))
        # the beams: to the office and back to the wall, heavy brackets at the office
        box("FrontWall__Wood", (cx0 + 0.08, beam_y - 0.12, post_z - 0.07), (cx1, beam_y, post_z + 0.07))
        box("FrontWall__Wood", (post_x - 0.07, beam_y - 0.12, wall_z), (post_x + 0.07, beam_y, post_z))
        for z in (post_z - 0.02, wall_z + 0.45):
            box("FrontWall__Wood", (ox0 - 0.2, beam_y - 0.3, z - 0.05), (ox0, beam_y - 0.12, z + 0.05))
    return {"office": (ox0, ox1), "passage": (px0, px1), "front": zf}


def side_entrance(z_door, floor=BAY_FLOOR, z_walk=(None, None)):
    """The front door in the left side wall near its front corner (look only: the game's
    door is the front wall's), dark and recessed, under a small, low tile shed out of the
    side wall (0.45, the neighbour's wall 1.7 off), the walk to it at the bay's floor. Its
    hole is the caller's (`side_entrance_hole`); drawn as the side wall's."""
    side = WALL_FRAMES["left"]  # u along +z
    a, b = z_door - DOOR_HALF, z_door + DOOR_HALF
    top = floor + 1.1
    side.quad("LeftWall__Wood", a, floor, b, top, 0.16)
    side.block("LeftWall__Wood", a + 0.06, floor + 0.56, b - 0.06, top - 0.08, 0.16, 0.15)
    for u0, u1 in ((a - 0.06, a), (b, b + 0.06)):
        side.block("LeftWall__Trim", u0, floor, u1, top + 0.06, 0.16, 0.0)
    side.block("LeftWall__Trim", a - 0.06, top, b + 0.06, top + 0.06, 0.16, 0.0)
    side.block("LeftWall__Iron", a + 0.05, floor + 0.5, a + 0.075, floor + 0.56, 0.15, 0.13)
    z0, z1 = z_walk
    box("LeftWall__Concrete", (-HALF_W - 0.5, 0.0, z0), (-HALF_W, floor, z1))
    out = 0.45
    x_out, x_in = -HALF_W - out, -HALF_W
    c0, c1 = a - 0.3, b + 0.3
    tiled("LeftWall", Slope([(x_out, top + 0.22, c1), (x_out, top + 0.22, c0), (x_in, top + 0.22 + 0.42 * out, c0), (x_in, top + 0.22 + 0.42 * out, c1)]))


def side_entrance_hole(z_door, floor=BAY_FLOOR):
    """The side door's hole in the left side wall, in its frame (u along +z)."""
    return (z_door - DOOR_HALF - 0.06, floor, z_door + DOOR_HALF + 0.06, floor + 1.1 + 0.06)
