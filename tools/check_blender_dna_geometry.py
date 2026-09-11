"""Inspect a saved actual BLEND independently of the worker's parameter resolver."""
import hashlib
import json
import math
from pathlib import Path
import sys
import bpy
from mathutils import Vector, kdtree

input_path, output_path = map(Path, sys.argv[sys.argv.index('--')+1:])
payload = json.loads(input_path.read_text(encoding='utf-8-sig'))
node = payload['request']['scene']['nodes'][0]
p = node['parameters']
mapping = json.loads(bpy.context.scene['phaseforge_coordinate_mapping'])
center = Vector(mapping['source_center'])
scale = mapping['display_scale']
meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
components = {o.get('phaseforge_component_id'): o for o in meshes}
assert len(meshes) == 2 + 2*p['count']
assert len(components) == len(meshes)
assert all(o.get('phaseforge_node_id') == node['id'] for o in meshes)
expected_colors = [tuple((int(value[i:i+2],16)/255)**2.2 for i in (1,3,5)) for value in p['colors']]
measurements = []
for strand in range(2):
    obj = components[f'{node["id"]}/backbone-{strand+1}']
    actual_color = tuple(obj.active_material.diffuse_color[:3])
    assert max(abs(a-b) for a,b in zip(actual_color, expected_colors[strand])) < 1e-6
    tree = kdtree.KDTree(16001)
    for i in range(16001):
        t=i/16000; angle=2*math.pi*p['turns']*t+strand*math.pi
        tree.insert((p['radius']*math.cos(angle),p['radius']*math.sin(angle),p['length']*(t-.5)),i)
    tree.balance()
    # Reconstruct original source positions from actual saved display geometry.
    distances = [tree.find((obj.matrix_world@vertex.co)/scale+center)[2] for vertex in obj.data.vertices]
    assert max(distances) <= p['thickness']*1.01
    assert max(distances) >= p['thickness']*.99
    measurements.append({'strand':strand+1,'material_linear_rgb':actual_color,'actual_max_distance_from_analytic_centerline':max(distances),'declared_radius':p['thickness'],'vertices':len(obj.data.vertices)})
for pair in range(p['count']):
    for half in range(2):
        obj=components[f'{node["id"]}/pair-{pair+1:03d}/half-{half+1}']
        expected=expected_colors[2+(2*pair+half)%(len(expected_colors)-2)]
        assert max(abs(a-b) for a,b in zip(obj.active_material.diffuse_color[:3],expected)) < 1e-6
assert json.loads(bpy.context.scene['phaseforge_provenance'])['kind'] == 'conceptual'
report={'passed':True,'scope':'Actual saved Blender geometry for the exact previously failed illustration input; no solver and no biological validation','component_count':len(components),'backbones':2,'base_pairs':p['count'],'colored_base_halves':2*p['count'],'all_six_authored_colors_preserved':True,'radius_tolerance_relative':.01,'backbone_geometry':measurements,'source_request_sha256':hashlib.sha256(json.dumps(payload['request'],sort_keys=True,separators=(',',':')).encode()).hexdigest(),'blender_version':bpy.app.version_string}
output_path.write_text(json.dumps(report,indent=2))
print(f'Actual DNA geometry checks passed: {output_path}')
