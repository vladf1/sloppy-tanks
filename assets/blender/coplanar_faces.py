# List the faces of a house model that z-fight: pairs of triangles facing the same way in
# nearly the same plane (within TOLERANCE) whose outlines overlap. Plain Python, no Blender:
#   python3 assets/blender/coplanar_faces.py public/models/houses/*.glb
# Faces facing opposite ways in one plane are left out: the back one is culled. So are
# pairs of one finish (`--all` keeps them): their UVs and shading are projected from the
# position, so they draw the same colour whichever wins.
import json
import math
import struct
import sys
from collections import defaultdict

TOLERANCE = 0.005  # m between the planes
MIN_AREA = 2e-4  # m² of overlap worth reporting (a 1.4 cm square)


def triangles(path):
    """Each triangle of the binary glTF at `path` as (node name, (a, b, c))."""
    data = open(path, "rb").read()
    json_length = struct.unpack("<I", data[12:16])[0]
    gltf = json.loads(data[20 : 20 + json_length])
    binary = data[20 + json_length + 8 :]

    def read(index, width, kind):
        accessor = gltf["accessors"][index]
        view = gltf["bufferViews"][accessor["bufferView"]]
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        count = accessor["count"]
        size = struct.calcsize(kind)
        values = struct.unpack_from(f"<{count * width}{kind}", binary, start)
        return [values[i : i + width] for i in range(0, len(values), width)] if width > 1 else list(values)

    for node in gltf["nodes"]:
        if "mesh" not in node:
            continue
        at = node.get("translation", [0, 0, 0])
        for primitive in gltf["meshes"][node["mesh"]]["primitives"]:
            positions = [tuple(p[k] + at[k] for k in range(3)) for p in read(primitive["attributes"]["POSITION"], 3, "f")]
            kind = {5123: "H", 5125: "I"}[gltf["accessors"][primitive["indices"]]["componentType"]]
            indices = read(primitive["indices"], 1, kind)
            for i in range(0, len(indices), 3):
                yield node["name"], tuple(positions[j] for j in indices[i : i + 3])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def clip(polygon, a, b):
    """The part of a convex 2D polygon left of the line a -> b."""
    out = []
    side = lambda p: (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])  # noqa: E731
    for i, p in enumerate(polygon):
        q = polygon[(i + 1) % len(polygon)]
        sp, sq = side(p), side(q)
        if sp >= 0:
            out.append(p)
        if sp * sq < 0:
            t = sp / (sp - sq)
            out.append((p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])))
    return out


def area(polygon):
    return abs(sum(p[0] * q[1] - q[0] * p[1] for p, q in zip(polygon, polygon[1:] + polygon[:1]))) / 2


def under_the_ground(tri, n, length):
    """Whether a face lies on the ground facing down, where no one sees it."""
    return n[1] / length < -0.999 and all(abs(p[1]) < 1e-3 for p in tri)


def overlaps(path):
    """The overlapping same-facing coplanar pairs: (area, name, name, centre)."""
    planes = defaultdict(list)
    for index, (name, tri) in enumerate(triangles(path)):
        n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]))
        length = math.sqrt(dot(n, n))
        if length < 1e-9:
            continue
        if under_the_ground(tri, n, length):
            continue
        n = tuple(c / length for c in n)
        d = dot(n, tri[0])
        key = (round(n[0] * 50), round(n[1] * 50), round(n[2] * 50), round(d / 0.01))
        planes[key].append((index, name, tri, n, d))
    found = []
    for (kx, ky, kz, kd), members in planes.items():
        # this offset's faces and the next one's (normals are exact enough to share a key)
        neighbours = planes.get((kx, ky, kz, kd + 1), [])
        for index_a, name_a, a, n, d in members:
            others = [m for m in members if m[0] > index_a] + neighbours
            axis = max(range(3), key=lambda k: abs(n[k]))
            keep = [k for k in range(3) if k != axis]
            flat_a = [(p[keep[0]], p[keep[1]]) for p in a]
            if (flat_a[1][0] - flat_a[0][0]) * (flat_a[2][1] - flat_a[0][1]) - (flat_a[1][1] - flat_a[0][1]) * (flat_a[2][0] - flat_a[0][0]) < 0:
                flat_a.reverse()
            box_a = [min(p[k] for p in a) for k in range(3)], [max(p[k] for p in a) for k in range(3)]
            for _, name_b, b, m, e in others:
                if dot(n, m) < 0.999 or abs(d - e) > TOLERANCE:
                    continue
                if any(min(p[k] for p in b) > box_a[1][k] + 1e-6 or max(p[k] for p in b) < box_a[0][k] - 1e-6 for k in range(3)):
                    continue
                polygon = [(p[keep[0]], p[keep[1]]) for p in b]
                for j in range(3):
                    polygon = clip(polygon, flat_a[j], flat_a[(j + 1) % 3])
                    if len(polygon) < 3:
                        break
                if len(polygon) < 3:
                    continue
                overlap = area(polygon) / max(abs(n[axis]), 1e-9)
                if overlap > MIN_AREA:
                    centre = [sum(p[k] for p in a) / 3 for k in range(3)]
                    found.append((overlap, name_a, name_b, centre))
    return found


if __name__ == "__main__":
    every = "--all" in sys.argv
    for path in [a for a in sys.argv[1:] if a != "--all"]:
        surface = lambda name: name.split(".")[0].split("__")[1]  # noqa: E731
        found = [f for f in overlaps(path) if every or surface(f[1]) != surface(f[2])]
        print(f"{path}: {len(found)} overlapping pairs")
        by_names = defaultdict(lambda: [0, 0.0, None])
        for overlap, a, b, centre in found:
            entry = by_names[tuple(sorted((a.split(".")[0], b.split(".")[0])))]
            entry[0] += 1
            entry[1] += overlap
            entry[2] = entry[2] or centre
        for (a, b), (count, total, centre) in sorted(by_names.items(), key=lambda item: -item[1][1]):
            print(f"  {a} / {b}: {count} pairs, {total:.4f} m², near ({centre[0]:.2f}, {centre[1]:.2f}, {centre[2]:.2f})")
