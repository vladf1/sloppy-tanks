# The Senda small Spanish house, modelled after Rumbo north lot 1, for Sloppy Tanks in Blender
# (headless):
#   blender --background --python assets/blender/small-spanish.py -- <out.blend> <out.glb> [<out.plan.json>]
#   (writes public/models/houses/small-spanish.glb and
#   crates/core/data/houses/small-spanish.plan.json)
#
# Drawn for the 6.2 x 8 m lot with its garage on the right (the game fits it to every lot of
# its plan: mirrored, stretched to the lot's box, wall top and ridge), at the level's scale
# (real sizes x 0.533). A cream two-storey stucco house under a side-gabled clay roof whose
# ridge runs along the street, a vented stucco gable at each end and a small cross gable
# over the back of the garage half; before its left side the tract's office projects under
# a small clay gable with a vent and an arched street window, a triple window over it, the
# entry passage beside it climbing to the front door under a little street gable on stucco
# arches; a stucco chimney stands out of the front between the office and the garage, up
# through the eave; the garage's one-piece roll-up door, strap-hinged, is recessed under
# the plain upper floor, whose wall stands out over it on a scalloped band and dark corbels,
# a single and a double window above; a solar block on the street slope's garage half. The
# paints are the style's (the game's palette).

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from house_kit import *  # noqa: E402,F403

front = front_frame()

# The openings. The front keeps the coded small Spanish plan, which the street's layout
# reads (the path and mailbox find the door): the window (the entry passage's front door
# stands for it), the door behind the office, the garage's two leaves and the windows over
# them, and the triple window over the office (counted over the door, within its column,
# so the game keeps it on every lot). Two windows a floor down each flank, three across
# the back.
PLAN = {
    "w": 2 * HALF_W,
    "d": 2 * HALF_D,
    "walls": {
        "front": {"ground": [[-1.91, "Window"], [-0.66, "Door"], [1.4, "Garage"]], "upper": [-1.0, 1.0, 1.8], "garage_bay": GARAGE_BAY},
        "back": {"ground": [[-1.6, "Window"], [0.4, "Window"], [2.0, "Window"]], "upper": [-1.6, 0.4, 2.0], "garage_bay": 0.0},
        "right": {"ground": [[-2.4, "Window"], [1.5, "Window"]], "upper": [-2.4, 1.5], "garage_bay": 0.0},
        "left": {"ground": [[-1.8, "Window"], [1.6, "Window"]], "upper": [-1.8, 1.6], "garage_bay": 0.0},
    },
}
_front = PLAN["walls"]["front"]
door_u = _front["ground"][1][0]
garage_u = _front["ground"][2][0]

# ---------------------------------------------------------------- the front wall
GARAGE = garage_opening(garage_u)
OVER_GARAGE = [(0.76, 1.24), (1.38, 2.3)]  # the single and the double window over the garage
# the triple window over the office, its centre the plan's upper opening over the door
upper_left = _front["upper"][0]
OVER_OFFICE = (upper_left - 0.48, upper_left + 0.48)
holes = [GARAGE, door_hole(door_u)] + entry_holes()
holes += [(a, UPPER_GLASS[0], b, UPPER_GLASS[1]) for a, b in OVER_GARAGE + [OVER_OFFICE]]
wall("FrontWall", front, 0, 2 * HALF_W, 0, WALL_TOP, [(fx(a), y0, fx(b), y1) for a, y0, b, y1 in holes])
for a, b in OVER_GARAGE:
    window("FrontWall", front, fx(a), UPPER_GLASS[0], fx(b), UPPER_GLASS[1], mullion=b - a > 0.6)
window("FrontWall", front, fx(OVER_OFFICE[0]), UPPER_GLASS[0], fx(OVER_OFFICE[1]), UPPER_GLASS[1], mullion=False)
for k in (1, 2):
    m = fx(OVER_OFFICE[0] + (OVER_OFFICE[1] - OVER_OFFICE[0]) * k / 3)
    front.block("FrontWall__Trim", m - FRAME / 2, UPPER_GLASS[0], m + FRAME / 2, UPPER_GLASS[1], 0.1, 0.075)
# the door behind the office (the passage's front door stands for the coded window)
front_door(front, door_u)

# The garage's one-piece roll-up door: stained boards, strap hinges.
sectional_garage_door(front, garage_u, decor="straps", boards=True)

# The upper floor stands out over the garage on a scalloped stucco band hung on dark
# corbels, as Rumbo north lot 1's does.
BAND = (0.0, HALF_W)  # x
BAND_Y = (1.4, 1.56)  # its corbels' line and its top
BAND_OUT = 0.09
SCALLOP_RISE = 0.07
CORBELS = [0.06, 1.0, 1.94, 2.88]
band_top, band_low = BAND_Y[1], BAND_Y[0]
for x0, x1 in zip(CORBELS, CORBELS[1:]):
    # a shallow arc between two corbels: the band's face and its soffit
    segments = 5
    arc = [
        (x0 + (x1 - x0) * k / segments, band_low + SCALLOP_RISE * math.sin(math.pi * k / segments))
        for k in range(segments + 1)
    ]
    for (xa, ya), (xb, yb) in zip(arc, arc[1:]):
        face("FrontWall__Siding", [(xa, ya, HALF_D + BAND_OUT), (xb, yb, HALF_D + BAND_OUT), (xb, band_top, HALF_D + BAND_OUT), (xa, band_top, HALF_D + BAND_OUT)], (0, 0, 1))
        face("FrontWall__Siding", [(xa, ya, HALF_D), (xb, yb, HALF_D), (xb, yb, HALF_D + BAND_OUT), (xa, ya, HALF_D + BAND_OUT)], (0, -1, 0))
# its ends beyond the outer corbels, its top and its ends
for xa, xb in ((BAND[0], CORBELS[0]), (CORBELS[-1], BAND[1])):
    face("FrontWall__Siding", [(xa, band_low, HALF_D + BAND_OUT), (xb, band_low, HALF_D + BAND_OUT), (xb, band_top, HALF_D + BAND_OUT), (xa, band_top, HALF_D + BAND_OUT)], (0, 0, 1))
    face("FrontWall__Siding", [(xa, band_low, HALF_D), (xb, band_low, HALF_D), (xb, band_low, HALF_D + BAND_OUT), (xa, band_low, HALF_D + BAND_OUT)], (0, -1, 0))
face("FrontWall__Siding", [(BAND[0], band_top, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[0], band_top, HALF_D + BAND_OUT)], (0, 1, 0))
face("FrontWall__Siding", [(BAND[0], band_low, HALF_D), (BAND[0], band_top, HALF_D), (BAND[0], band_top, HALF_D + BAND_OUT), (BAND[0], band_low, HALF_D + BAND_OUT)], (-1, 0, 0))
face("FrontWall__Siding", [(BAND[1], band_low, HALF_D), (BAND[1], band_top, HALF_D), (BAND[1], band_top, HALF_D + BAND_OUT), (BAND[1], band_low, HALF_D + BAND_OUT)], (1, 0, 0))
for cx in CORBELS[1:-1] + [CORBELS[0] + 0.04, CORBELS[-1] - 0.04]:
    box("FrontWall__Wood", (cx - 0.04, band_low - 0.06, HALF_D), (cx + 0.04, band_low + 0.03, HALF_D + BAND_OUT + 0.05), skip=("-z",))

# A lantern beside the garage.
box("FrontWall__Iron", (2.8, 0.78, HALF_D), (2.88, 0.95, HALF_D + 0.07), skip=("-z",))
box("FrontWall__Iron", (2.82, 0.95, HALF_D + 0.01), (2.86, 0.99, HALF_D + 0.05), skip=("-z", "-y"))
# The downpipe at the house's right corner.
box("FrontWall__Metal", (HALF_W - 0.07, 0.0, HALF_D + 0.01), (HALF_W - 0.03, EAVE_Y - 0.05, HALF_D + 0.05), skip=("-z", "-y", "+y"))

# ---------------------------------------------------------------- the roof
# Rumbo north lot 1 is side-gabled, not hipped: one clay roof whose ridge runs along the
# street over the whole width, a stucco gable at each end (a clay vent in each), and a
# small cross gable over the back of the garage half whose ridge runs back to the north
# wall's gable. The game's structure is still the coded front-to-back hip (its ridge
# along z, hip ends front and back): the drawn faces are cut along the coded hip lines and
# each piece goes to the <Side>Roof part whose coded slope it lies over, so the roof falls
# as the coded parts do. The long slopes keep the coded ridge height and their pitch puts
# the deck at the wall top over the walls' line.
GABLE_PITCH = (RIDGE_Y - WALL_TOP) / HALF_D
GABLE_EAVE_Y = WALL_TOP - GABLE_PITCH * OVERHANG
# The back cross gable: its eaves' x (the east one the main rake's) and where its ridge
# runs into the main back slope.
WING_X = (-0.1, EAVE_X)
WING_MID = (WING_X[0] + WING_X[1]) / 2
WING_HALF = (WING_X[1] - WING_X[0]) / 2
WING_APEX = GABLE_EAVE_Y + GABLE_PITCH * WING_HALF
WING_RIDGE_END = -EAVE_Z + WING_HALF  # where the valleys meet the ridge


def coded_regions():
    """The coded hip's four slopes in plan (x, z), each carried out past the eaves along
    its hip lines so every drawn piece falls in one of them."""
    far = 20.0
    rh = RIDGE_HALF

    def out(cx, cz, rz):
        return (cx * (1 + far), cz + (cz - rz) * far)

    fr, fl = out(EAVE_X, EAVE_Z, rh), out(-EAVE_X, EAVE_Z, rh)
    br, bl = out(EAVE_X, -EAVE_Z, -rh), out(-EAVE_X, -EAVE_Z, -rh)
    return {
        "FrontRoof": [(0.0, rh), fl, fr],
        "BackRoof": [(0.0, -rh), br, bl],
        "RightRoof": [(0.0, rh), fr, br, (0.0, -rh)],
        "LeftRoof": [(0.0, -rh), bl, fl, (0.0, rh)],
    }


def half_planes(region):
    """A convex plan polygon as half-planes (a, b, c): a x + b z + c >= 0 inside."""
    cx = sum(p[0] for p in region) / len(region)
    cz = sum(p[1] for p in region) / len(region)
    planes = []
    for (x0, z0), (x1, z1) in zip(region, region[1:] + region[:1]):
        a, b = -(z1 - z0), x1 - x0
        c = -(a * x0 + b * z0)
        if a * cx + b * cz + c < 0:
            a, b, c = -a, -b, -c
        planes.append((a, b, c))
    return planes


def clip_plane(polygon, plane):
    """A planar polygon (build-frame points) cut to a vertical half-space."""
    a, b, c = plane
    out = []
    for p, q in zip(polygon, polygon[1:] + polygon[:1]):
        dp, dq = a * p.x + b * p.z + c, a * q.x + b * q.z + c
        if dp >= 0:
            out.append(p)
        if dp * dq < 0:
            out.append(p.lerp(q, dp / (dp - dq)))
    return out


def area(polygon):
    total = Vector()
    for p, q in zip(polygon, polygon[1:] + polygon[:1]):
        total += p.cross(q)
    return total.length / 2


def put(key, points):
    """A polygon added as given (its winding kept)."""
    vertices, faces = geometry[key]
    start = len(vertices)
    vertices.extend(points)
    faces.append(list(range(start, start + len(points))))


def distribute(temp, cut_out=None):
    """Move the faces drawn under `temp` (keys `temp__<Surface>`) into the coded roof parts,
    each cut along the coded hip lines; with `cut_out` (a convex plan polygon) the part of
    each face inside it is dropped."""
    regions = {part: half_planes(region) for part, region in coded_regions().items()}
    holes = half_planes(cut_out) if cut_out else None
    for key in [key for key in geometry if key.startswith(temp + "__")]:
        surface = key.split("__")[1]
        vertices, faces = geometry.pop(key)
        for f in faces:
            pieces = [[Vector(vertices[i]) for i in f]]
            if holes:
                # outside a convex hole: outside its first edge, or inside it and outside
                # the second, or inside both and outside the third
                kept = []
                for k in range(len(holes)):
                    piece = pieces[0]
                    for plane in holes[:k]:
                        piece = clip_plane(piece, plane)
                    a, b, c = holes[k]
                    piece = clip_plane(piece, (-a, -b, -c)) if len(piece) >= 3 else []
                    if len(piece) >= 3:
                        kept.append(piece)
                pieces = kept
            for piece in pieces:
                for part, planes in regions.items():
                    cut = piece
                    for plane in planes:
                        cut = clip_plane(cut, plane)
                        if len(cut) < 3:
                            break
                    if len(cut) >= 3 and area(cut) > 1e-6:
                        put(f"{part}__{surface}", cut)


def landscape_array(part, slope, cols, rows, up, across):
    """The kit's solar block ([`solar_array`]) with its panels laid landscape, as the
    real array's rows run along the slope."""
    panel = (SOLAR_PANEL[1], SOLAR_PANEL[0])
    us = [u for u, _ in slope.uv]
    vs = [v for _, v in slope.uv]
    width = cols * panel[0] + (cols - 1) * SOLAR_GAP
    u0 = min(us) + (max(us) - min(us)) * across - width / 2
    v0 = min(vs) + (max(vs) - min(vs)) * up
    for c in range(cols):
        for r in range(rows):
            pu = u0 + c * (panel[0] + SOLAR_GAP)
            pv = v0 + r * (panel[1] + SOLAR_GAP)
            corners = [(pu, pv), (pu + panel[0], pv), (pu + panel[0], pv + panel[1]), (pu, pv + panel[1])]
            face(part + "__Solar", [slope.at(u, v, SOLAR_LIFT) for u, v in corners], slope.N)


# the long slopes and the ridge; the array on the street slope's garage half, near the
# ridge, four landscape rows of two
street_slope = Slope([(-EAVE_X, GABLE_EAVE_Y, EAVE_Z), (EAVE_X, GABLE_EAVE_Y, EAVE_Z), (EAVE_X, RIDGE_Y, 0.0), (-EAVE_X, RIDGE_Y, 0.0)])
tiled("Main", street_slope, eave_edges=(0, 1, 3), fascia=False)  # the ridge's edge is under its cap
landscape_array("Main", street_slope, 2, 4, up=0.5, across=0.64)
ridge_cap("Main", (-EAVE_X, RIDGE_Y, 0.0), (EAVE_X, RIDGE_Y, 0.0))
distribute("Main")
# the back slope, open where the cross gable stands on it
tiled("MainBack", Slope([(EAVE_X, GABLE_EAVE_Y, -EAVE_Z), (-EAVE_X, GABLE_EAVE_Y, -EAVE_Z), (-EAVE_X, RIDGE_Y, 0.0), (EAVE_X, RIDGE_Y, 0.0)]), eave_edges=(0, 1, 3), fascia=False)
WING_PLAN = [(WING_X[0] - 0.05, -EAVE_Z - 0.05), (WING_X[1] + 0.05, -EAVE_Z - 0.05), (WING_MID, WING_RIDGE_END)]
distribute("MainBack", cut_out=WING_PLAN)
# the cross gable: two triangles from its ridge, the valleys where they meet the back slope
# (only their rakes show a fascia)
tiled("Wing", Slope([(WING_MID, WING_APEX, WING_RIDGE_END), (WING_MID, WING_APEX, -EAVE_Z), (WING_X[0], GABLE_EAVE_Y, -EAVE_Z)]), eave_edges=(1,), fascia=False)
tiled("Wing", Slope([(WING_MID, WING_APEX, -EAVE_Z), (WING_MID, WING_APEX, WING_RIDGE_END), (WING_X[1], GABLE_EAVE_Y, -EAVE_Z)]), eave_edges=(2,), fascia=False)
ridge_cap("Wing", (WING_MID, WING_APEX, -EAVE_Z), (WING_MID, WING_APEX, WING_RIDGE_END))
for foot in (WING_X[0], WING_X[1]):
    ridge_cap("Wing", (foot, GABLE_EAVE_Y, -EAVE_Z), (WING_MID, WING_APEX, WING_RIDGE_END), half=0.04, lift=0.01)
distribute("Wing")

# The stucco gables over the wall tops, lined inside, each with a clay vent near its peak.
UNDER_DECK = DECK * 1.1


def wall_vent(side, u, y, spacing=0.07):
    """Three clay pipe ends in a gable, as the kit's [`clay_vent`] on any wall's frame but
    drawn as their faces alone (a high gable's vent is a few pixels in the game)."""
    frame, part = WALL_FRAMES[side], WALL_PARTS[side]
    for du, dy in ((-spacing / 2, 0), (spacing / 2, 0), (0, spacing * 0.85)):
        frame.quad(f"{part}__Terracotta", u + du - 0.022, y + dy - 0.022, u + du + 0.022, y + dy + 0.022, -0.004)
        frame.quad(f"{part}__Dark", u + du - 0.012, y + dy - 0.012, u + du + 0.012, y + dy + 0.012, -0.006)


def gable(side, mid, half):
    """A gable's stucco triangle in a wall's frame (u from its middle) under a roof whose
    eaves lie `half` either way of u = `mid`; returns its peak's height."""
    frame, part = WALL_FRAMES[side], WALL_PARTS[side]
    peak = GABLE_EAVE_Y + GABLE_PITCH * half - UNDER_DECK
    foot = half - (WALL_TOP + UNDER_DECK - GABLE_EAVE_Y) / GABLE_PITCH
    outline = [(mid - foot, WALL_TOP), (mid + foot, WALL_TOP), (mid, peak)]
    face(f"{part}__Siding", [frame.p(u, y) for u, y in outline], frame.out)
    face(f"{part}__Lining", [frame.p(u, y, WALL) for u, y in outline], -frame.out)
    wall_vent(side, mid, peak - 0.24)
    return peak


gable("right", 0.0, EAVE_Z)
gable("left", 0.0, EAVE_Z)
gable("back", -WING_MID, WING_HALF)  # the back wall's u runs along -x

# ---------------------------------------------------------------- the entry
# The office beside the garage under a small clay gable with a vent, its arched street
# window, and the entry passage up its outer side to the front door under a small street
# gable on stucco arches.
office_entry(office_roof="gable", vent=True, window_style="arched", cover="gable", supports="arches")

# ---------------------------------------------------------------- the chimney
# Stucco (a real 0.75 m stack), standing out of the front wall between the office and the
# garage from over the garage's head through the eave, its capped top a little over the
# eave, as Rumbo north lot 1's does.
CHIMNEY = (-0.28, 0.12, HALF_D - 0.18, HALF_D + 0.22)
CHIMNEY_FOOT = GARAGE_TOP + 0.1
CHIMNEY_TOP = 3.72
box("FrontWall__Siding", (CHIMNEY[0], CHIMNEY_FOOT, CHIMNEY[2]), (CHIMNEY[1], CHIMNEY_TOP - 0.06, CHIMNEY[3]), skip=("-y",))
box("FrontWall__Siding", (CHIMNEY[0] - 0.03, CHIMNEY_TOP - 0.06, CHIMNEY[2]), (CHIMNEY[1] + 0.03, CHIMNEY_TOP, CHIMNEY[3] + 0.03), skip=("-z",))
face("FrontWall__Dark", [(CHIMNEY[0] + 0.08, CHIMNEY_TOP + 0.002, CHIMNEY[2] + 0.1), (CHIMNEY[1] - 0.08, CHIMNEY_TOP + 0.002, CHIMNEY[2] + 0.1), (CHIMNEY[1] - 0.08, CHIMNEY_TOP + 0.002, CHIMNEY[3] - 0.08), (CHIMNEY[0] + 0.08, CHIMNEY_TOP + 0.002, CHIMNEY[3] - 0.08)], (0, 1, 0))

# ---------------------------------------------------------------- the flanks and back
# the upper floor's back windows as wide as the ground floor's: twin windows over twins
plain_wall("back", PLAN, half_width={-1.6: 0.42, 0.4: 0.35, 2.0: 0.3})
# the upper window under the back gable hung with plain board shutters
_back = WALL_FRAMES["back"]
for a in (-1.6 - 0.3 - SHUTTER_WIDTH - 0.02, -1.6 + 0.3 + 0.02):
    _back.block("BackWall__Accent", a, UPPER_GLASS[0] - 0.01, a + SHUTTER_WIDTH, UPPER_GLASS[1] + 0.01, 0.0, -0.025)
plain_wall("right", PLAN, half_width=0.27)
plain_wall("left", PLAN, half_width=0.3)

export(PLAN)
