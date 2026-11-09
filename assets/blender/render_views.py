# Render a house from the reference angles, headless:
#   blender --background --python assets/blender/render_views.py -- <model.obj|model.blend|model.glb> <out prefix> [along]
# `along` turns a model exported in an eave-door house's build frame (front +x) back to the
# street frame these views expect.
# Views match artifacts/performance/blender-house/refs: straight on from the street, the
# east and west obliques at street height, an Earth-like high oblique and the top.
# Build frame: front +z, right +x, y up; in Blender that is front -Y, up +Z.
import math
import sys

import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1 :]
source, prefix = argv[0], argv[1]
along = len(argv) > 2 and argv[2] == "along"

SURFACE_COLORS = {
    "Siding": (0.62, 0.45, 0.28),
    "Trim": (0.68, 0.52, 0.35),
    "Accent": (0.13, 0.20, 0.25),
    "Glass": (0.10, 0.13, 0.16),
    "Dark": (0.02, 0.02, 0.02),
    "Wood": (0.07, 0.035, 0.02),
    "GarageDoor": (0.19, 0.11, 0.07),
    "Roof": (0.32, 0.12, 0.10),
    "Shingles": (0.32, 0.12, 0.10),
    "Brick": (0.30, 0.15, 0.11),
    "Stone": (0.55, 0.47, 0.37),
    "Concrete": (0.42, 0.40, 0.37),
    "Curtain": (0.75, 0.74, 0.70),
    "Metal": (0.33, 0.35, 0.36),
    "Iron": (0.03, 0.03, 0.03),
    "Solar": (0.012, 0.016, 0.03),
    "Lining": (0.4, 0.35, 0.3),
    "Cladding": (0.25, 0.28, 0.24),
    "Terracotta": (0.35, 0.10, 0.04),
    "Lamp": (0.9, 0.7, 0.4),
}

bpy.ops.wm.read_factory_settings(use_empty=True)
if source.endswith(".obj"):
    bpy.ops.wm.obj_import(filepath=source, forward_axis="NEGATIVE_Z", up_axis="Y")
elif source.endswith(".glb"):
    bpy.ops.import_scene.gltf(filepath=source)
else:
    with bpy.data.libraries.load(source) as (src, dst):
        dst.objects = src.objects
    for obj in dst.objects:
        bpy.context.scene.collection.objects.link(obj)

if along:
    # the build frame's +x front back to the street frame's +z: a quarter turn about up
    for obj in list(bpy.context.scene.objects):
        if obj.parent is None:
            obj.rotation_mode = "XYZ"
            obj.rotation_euler[2] -= math.pi / 2

materials = {}
for obj in bpy.context.scene.objects:
    if obj.type != "MESH":
        continue
    surface = obj.name.split(".")[0].split("__")[-1]
    color = SURFACE_COLORS.get(surface, (0.5, 0.5, 0.5))
    if surface not in materials:
        material = bpy.data.materials.new(surface)
        material.diffuse_color = (*color, 1.0)
        materials[surface] = material
    obj.data.materials.clear()
    obj.data.materials.append(materials[surface])

# Ground plane and the street kerb line for scale.
bpy.ops.mesh.primitive_plane_add(size=60, location=(0, 0, -0.005))
ground = bpy.context.object
ground_material = bpy.data.materials.new("ground")
ground_material.diffuse_color = (0.30, 0.36, 0.22, 1)
ground.data.materials.append(ground_material)

scene = bpy.context.scene
scene.render.engine = "BLENDER_WORKBENCH"
scene.display.shading.light = "STUDIO"
scene.display.shading.color_type = "MATERIAL"
scene.display.shading.show_shadows = True
scene.display.shading.show_cavity = True
scene.display.shading.background_type = "WORLD"
scene.world = bpy.data.worlds.new("sky")
scene.world.color = (0.55, 0.68, 0.85)
scene.render.resolution_x = 1200
scene.render.resolution_y = 800
scene.render.film_transparent = False

camera = bpy.data.cameras.new("camera")
camera_object = bpy.data.objects.new("camera", camera)
scene.collection.objects.link(camera_object)
scene.camera = camera_object

# (name, eye in build frame, target in build frame, lens mm)
VIEWS = [
    ("front", (0.6, 1.4, 17.0), (0.0, 2.0, 0.0), 35),
    ("oblique-e", (9.0, 1.4, 13.5), (0.0, 2.0, 1.0), 32),
    ("oblique-w", (-8.0, 1.4, 14.0), (0.0, 2.0, 1.0), 32),
    ("high", (2.0, 14.0, 17.0), (0.0, 1.5, 0.5), 30),
    ("top", (0.0, 30.0, 0.01), (0.0, 0.0, 0.0), 35),
    ("back", (-4.0, 6.0, -15.0), (0.0, 2.0, 0.0), 32),
]


def blender(point):
    x, y, z = point
    return Vector((x, -z, y))


for name, eye, target, lens in VIEWS:
    camera.lens = lens
    camera_object.location = blender(eye)
    direction = blender(target) - blender(eye)
    camera_object.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = f"{prefix}-{name}.png"
    bpy.ops.render.render(write_still=True)
    print("rendered", scene.render.filepath)
