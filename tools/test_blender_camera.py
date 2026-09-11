"""Run in Blender to verify the actual illustration camera basis, including roll."""
import ast
import json
import math
from pathlib import Path
import sys
import bpy
from mathutils import Vector,Matrix

source=Path(__file__).with_name('blender_render.py')
tree=ast.parse(source.read_text(encoding='utf-8'))
namespace={'bpy':bpy,'Vector':Vector,'Matrix':Matrix,'math':math,'STYLE':'studio'}
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name in {'number','vector','frame_scene'}],type_ignores=[]),str(source),'exec'),namespace)
results=[]
for up in [[0,0,1],[1,0,0],[1,0,1]]:
    bpy.ops.wm.read_factory_settings(use_empty=True);bpy.ops.mesh.primitive_cube_add()
    mapping=namespace['frame_scene']({'camera':{'position':[0,-10,0],'target':[0,0,0],'up':up,'focal_length_mm':50}})
    camera=bpy.context.scene.camera
    actual_up=camera.rotation_quaternion@Vector((0,1,0));actual_forward=camera.rotation_quaternion@Vector((0,0,-1))
    expected_up=Vector(up).normalized()
    assert (actual_up-expected_up).length<2e-6,(actual_up,expected_up)
    assert (actual_forward-Vector((0,1,0))).length<2e-6,actual_forward
    assert abs(camera.data.lens-50)<1e-6
    results.append({'source_up':up,'actual_camera_up':list(actual_up),'actual_forward':list(actual_forward),'passed':True,'mapping':mapping})
for invalid in [[0,0,0],[0,1,0]]:
    bpy.ops.wm.read_factory_settings(use_empty=True);bpy.ops.mesh.primitive_cube_add()
    try:namespace['frame_scene']({'camera':{'position':[0,-10,0],'target':[0,0,0],'up':invalid}})
    except ValueError:results.append({'invalid_up':invalid,'rejected':True})
    else:raise AssertionError('Invalid up vector accepted')
output=Path(sys.argv[sys.argv.index('--')+1]);output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps({'passed':True,'blender_version':bpy.app.version_string,'checks':results},indent=2),encoding='utf-8')
print(f'Actual Blender camera checks passed: {output}')
