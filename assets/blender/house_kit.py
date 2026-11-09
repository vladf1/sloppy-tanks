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


# Lot dimensions (crates/core/examples/home_dump.rs prints them).
HALF_W, HALF_D = 2.5, 3.5
WALL_TOP = 3.15
WALL = 0.2  # slab depth, siding to lining
EAVE_Y, RIDGE_Y, EAVE_X, EAVE_Z, RIDGE_HALF = 3.02, 4.0, 2.88, 3.88, 1.0
DECK = 0.1  # roof thickness at the edges
RIB = (0.22, 0.045)  # barrel tile spacing and height
COURSE = 0.42  # tile course spacing down a slope

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


def window(part, frame, u0, y0, u1, y1, mullion=True, sill=True):
    """White vinyl frames over white plantation shutters, set into an opening."""
    frame.quad(f"{part}__Curtain", u0, y0, u1, y1, 0.11)
    # louvres: a few slat lines
    slats = max(2, int((y1 - y0) / 0.09))
    for k in range(1, slats):
        y = y0 + (y1 - y0) * k / slats
        frame.block(f"{part}__Glass", u0 + 0.03, y - 0.006, u1 - 0.03, y + 0.006, 0.105, 0.1)
    t = 0.045
    frame.block(f"{part}__Trim", u0, y0, u1, y0 + t, 0.1, 0.06)
    frame.block(f"{part}__Trim", u0, y1 - t, u1, y1, 0.1, 0.06)
    frame.block(f"{part}__Trim", u0, y0, u0 + t, y1, 0.1, 0.06)
    frame.block(f"{part}__Trim", u1 - t, y0, u1, y1, 0.1, 0.06)
    if mullion:
        m = (u0 + u1) / 2
        frame.block(f"{part}__Trim", m - t / 2, y0, m + t / 2, y1, 0.1, 0.065)
    if sill:
        frame.block(f"{part}__Trim", u0 - 0.05, y0 - 0.05, u1 + 0.05, y0, 0.02, -0.05)


def shutter(part, frame, u0, y0, u1, y1):
    frame.block(f"{part}__Accent", u0, y0, u1, y1, 0.0, -0.035)
    for k in range(1, 4):
        y = y0 + (y1 - y0) * k / 4
        frame.block(f"{part}__Accent", u0 + 0.02, y - 0.015, u1 - 0.02, y + 0.015, -0.035, -0.05)


# ---------------------------------------------------------------- roofs


class Slope:
    """A planar roof polygon (convex, build frame), its first edge the eave."""

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
        if (centroid - p0).dot(v) < 0:
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


# Flat concrete tile (the Craftsman and Prairie roofs): low ribs closer together, deeper
# course lips.
FLAT_RIB = (0.18, 0.016)
FLAT_COURSE = 0.32


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
            lift = 0.05 if flat else height * 1.25
            face(roof, [slope.at(lo, v), slope.at(hi, v), slope.at(hi, v, lift), slope.at(lo, v, lift)], -slope.V)
            face(roof, [slope.at(lo, v, lift), slope.at(hi, v, lift), slope.at(hi, v + 0.14, height * 0.5), slope.at(lo, v + 0.14, height * 0.5)], slope.N)
        v += course


def ridge_cap(part, a, b, half=0.09, lift=0.07):
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

# The structure's measures (house_layout.rs): an opening's column half-widths, and the
# low storeys' window bands (Roof::ground_window, Roof::upper_window).
WINDOW_BAY, DOOR_BAY = 0.57, 0.62
GROUND_WINDOW = (0.67, 1.97)
UPPER_WINDOW = (2.23, 2.9)
# What the model draws inside a window column: the frame's half width and its height.
WINDOW_HALF = 0.4
GROUND_GLASS = (0.72, 1.62)
UPPER_GLASS = (2.26, 2.86)


def plain_wall(side, plan):
    """A flank or the back: the stucco slab with each planned window cut and dressed."""
    frame, part, half = WALL_FRAMES[side], WALL_PARTS[side], WALL_HALF[side]
    plan = plan["walls"][side]
    holes = []
    for u, kind in plan["ground"]:
        assert kind == "Window", f"{side}: a {kind} is drawn only on the front"
        holes.append((u - WINDOW_HALF, GROUND_GLASS[0], u + WINDOW_HALF, GROUND_GLASS[1]))
    for u in plan["upper"]:
        assert any(abs(u - g) < WINDOW_BAY for g, _ in plan["ground"]), f"{side}: upper window at {u} over no opening"
        holes.append((u - WINDOW_HALF, UPPER_GLASS[0], u + WINDOW_HALF, UPPER_GLASS[1]))
    wall(part, frame, -half, half, 0, WALL_TOP, holes)
    for hole in holes:
        window(part, frame, *hole)



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
# drawn for (5 m front, 7 m deep, ridge 4 m) and exported `frame="along"`.

# The main hip whose ridge runs along the street, as the game builds it on that lot: its
# eaves either way from the middle, eave height and the ridge's half length.
ALONG_EAVE = (2.88, 3.88)
ALONG_EAVE_Y = 3.0577
ALONG_RIDGE_HALF = 1.43


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


def sectional_garage_door(front, garage_u, garage_bay, top=1.5, panels=4):
    """A one-piece sectional garage door set back in its opening: stained boards and the
    panels' joints across it, a trim head. Returns the opening (x0, y0, x1, y1)."""
    x0, x1 = garage_u - 2 * garage_bay + 0.14, garage_u + 2 * garage_bay - 0.12
    g0, g1 = fx(x0), fx(x1)
    front.quad("FrontWall__GarageDoor", g0, 0.12, g1, top, 0.12)
    u = g0 + 0.1
    while u < g1 - 0.05:
        front.block("FrontWall__GarageDoor", u - 0.01, 0.16, u + 0.01, top - 0.04, 0.12, 0.108)
        u += 0.12
    for k in range(1, panels):
        y = 0.12 + (top - 0.12) * k / panels
        front.block("FrontWall__Dark", g0, y - 0.008, g1, y + 0.008, 0.12, 0.105)
    front.block("FrontWall__Trim", g0 - 0.05, top, g1 + 0.05, top + 0.05, 0.0, -0.03)
    return (x0, 0.12, x1, top)


def box_bay(front, x0, x1, flat=True, out=0.3, lo=1.72, hi=2.86):
    """A dark stained box bay standing out of the upper floor over x0..x1: board and batten
    under two windows, brackets, a tile hood."""
    b0, b1 = fx(x0), fx(x1)
    for a, c, b, d in ((b0, lo, b1, lo + 0.08), (b0, hi - 0.05, b1, hi), (b0, lo, b0 + 0.06, hi), (b1 - 0.06, lo, b1, hi)):
        front.block("FrontWall__Wood", a, c, b, d, 0.0, -out)
    front.block("FrontWall__Wood", b0, lo, b1, 2.12, -out + 0.03, -out)
    u = b0 + 0.12
    while u < b1 - 0.05:
        front.block("FrontWall__Wood", u - 0.012, lo + 0.02, u + 0.012, 2.12, -out, -out - 0.02)
        u += 0.14
    mid = (b0 + b1) / 2
    for wu0, wu1 in ((b0 + 0.08, mid - 0.03), (mid + 0.03, b1 - 0.08)):
        front.quad("FrontWall__Curtain", wu0, 2.14, wu1, hi - 0.06, -out + 0.03)
        for a, c, b, d in ((wu0, 2.14, wu1, 2.18), (wu0, hi - 0.1, wu1, hi - 0.06), (wu0, 2.14, wu0 + 0.04, hi - 0.06), (wu1 - 0.04, 2.14, wu1, hi - 0.06)):
            front.block("FrontWall__Trim", a, c, b, d, -out + 0.03, -out)
    front.block("FrontWall__Wood", mid - 0.03, 2.12, mid + 0.03, hi, -out + 0.02, -out - 0.01)
    for su, sign in ((b0, -1), (b1, 1)):
        face("FrontWall__Wood", [front.p(su, lo, 0), front.p(su, hi, 0), front.p(su, hi, -out), front.p(su, lo, -out)], front.along * sign)
    for bu in (b0 + 0.12, b1 - 0.12):
        front.block("FrontWall__Wood", bu - 0.04, lo - 0.2, bu + 0.04, lo, 0.0, -out + 0.04)
    hood = HALF_D + out + 0.12
    tiled("FrontWall", Slope([(x0 - 0.1, hi + 0.02, hood), (x1 + 0.1, hi + 0.02, hood), (x1 + 0.1, hi + 0.22, HALF_D), (x0 - 0.1, hi + 0.22, HALF_D)]), flat=flat)


def board_shutters(front, u0, u1, y0, y1, width=0.26):
    """A board shutter each side of a window from u0 to u1."""
    for a, b in ((u0 - width - 0.03, u0 - 0.03), (u1 + 0.03, u1 + width + 0.03)):
        front.block("FrontWall__Accent", a, y0, b, y1, 0.0, -0.035)
        for k in range(1, 3):
            y = y0 + (y1 - y0) * k / 3
            front.block("FrontWall__Accent", a + 0.02, y - 0.02, b - 0.02, y + 0.02, -0.035, -0.05)


def porch_post(x, z, top, pier=0.6, stone=True):
    """A chunky dark timber porch post on a stacked-stone (or brick) pier."""
    surface = "Stone" if stone else "Brick"
    box(f"FrontWall__{surface}", (x - 0.18, 0.08, z - 0.18), (x + 0.18, pier, z + 0.18))
    box("FrontWall__Concrete", (x - 0.21, pier, z - 0.21), (x + 0.21, pier + 0.05, z + 0.21))
    box("FrontWall__Wood", (x - 0.09, pier + 0.05, z - 0.09), (x + 0.09, top, z + 0.09))
