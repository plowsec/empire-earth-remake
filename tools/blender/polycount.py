import bpy, glob, os, sys
root = os.path.join(os.path.dirname(__file__), "..", "..", "game", "assets", "models")
for f in sorted(glob.glob(os.path.join(root, "*.glb"))):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=f)
    tris = 0
    for o in bpy.context.scene.objects:
        if o.type == "MESH":
            o.data.calc_loop_triangles()
            tris += len(o.data.loop_triangles)
    print(f"POLY {os.path.basename(f)[:-4]} {tris}")
