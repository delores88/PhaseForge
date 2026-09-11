"""Read-only renderer contracts, independent of Blender and of the scientific solver."""
import ast
from collections import OrderedDict
import bisect
import hashlib
import io
import json
import math
from pathlib import Path
import tempfile
import sys
import unittest
from types import SimpleNamespace
import numpy as np

namespace={'OrderedDict':OrderedDict,'bisect':bisect,'hashlib':hashlib,'io':io,'json':json,'math':math,'Path':Path,'np':np,'sys':sys}
for name,helpers in [('trajectory_render.py',{'sha256','committed_digest','pinned_source_index','verify_source_bytes','finite','integer','rgb','recorded_index'}),('field_render.py',{'Fields','srgb_bytes','field_colors','set_hud_visibility'})]:
    tree=ast.parse(Path(__file__).with_name(name).read_text(encoding='utf-8'))
    exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node,(ast.FunctionDef,ast.ClassDef)) and node.name in helpers],type_ignores=[]),name,'exec'),namespace)


def fixture(root):
    (root/'fields').mkdir()
    records=[]
    for step in range(2):
        values=np.array([[0.,.25],[.75,1.]])+step
        numeric=root/f'fields/field-{step}.npy';np.save(numeric,values)
        view=root/f'fields/view-{step}.json'
        view.write_text(json.dumps({'values':values.tolist(),'shape':[2,2],'step':step,'time':step/10}),encoding='utf-8')
        records.append({'path':numeric.relative_to(root).as_posix(),'view_path':view.relative_to(root).as_posix(),
                        'sha256':namespace['sha256'](numeric),'view_sha256':namespace['sha256'](view),
                        'step':step,'time':step/10})
    index={'representation':'scalar_field','shape':[2,2],'lengths_um':[4,8],
           'axis_order':['y','x'],'grid_location':'cell_center','frames':records,
           'start_time':0.,'end_time':.1,'frame_count':2}
    (root/'fields/index.json').write_text(json.dumps(index,indent=2),encoding='utf-8')
    snapshot=root/'snapshot.json';snapshot.write_bytes((root/'fields/index.json').read_bytes())
    first=records[0]
    pin={'index_snapshot_path':str(snapshot),'index_sha256':namespace['sha256'](snapshot),
         'entry_number':0,'files':{first['path']:first['sha256'],first['view_path']:first['view_sha256']}}
    return index,pin


class FieldRenderContract(unittest.TestCase):
    def test_exact_state_digest_and_view_must_agree_with_numerical_array(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'fields').mkdir();records=[]
            for step,time in enumerate([0.,.1,.2]):
                values=np.array([[0.,.25],[.75,1.]])+step
                numeric=root/f'fields/field-{step}.npy';np.save(numeric,values)
                view=root/f'fields/view-{step}.json';view.write_text(json.dumps({'values':values.tolist(),'shape':[2,2],'step':step,'time':time}),encoding='utf-8')
                records.append({'path':numeric.relative_to(root).as_posix(),'view_path':view.relative_to(root).as_posix(),'sha256':namespace['sha256'](numeric),'view_sha256':namespace['sha256'](view),'step':step,'time':time})
            index={'representation':'scalar_field','shape':[2,2],'lengths_um':[4,8],'axis_order':['y','x'],'grid_location':'cell_center','frames':records}
            (root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
            source=namespace['Fields'](root);values,receipt=source.sample(.199999);self.assertEqual(receipt['step'],1);self.assertEqual(values[0,0],1);self.assertEqual(source.sample(.2)[1]['step'],2)
            self.assertEqual(source.sample(math.nextafter(.2, 0))[1]['step'], 2)
            # A rewritten display view with its own newly valid digest must still
            # be rejected if it differs from the authoritative .npy values.
            view=root/records[0]['view_path'];bad=json.loads(view.read_text());bad['values'][1][0]=999;view.write_text(json.dumps(bad),encoding='utf-8');index['frames'][0]['view_sha256']=namespace['sha256'](view);(root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'authoritative numerical field'):namespace['Fields'](root).sample(0)
            index['frames'][0]['view_sha256']='0'*64;(root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'digest'):namespace['Fields'](root).sample(0)

    def test_quantitative_palette_matches_explicit_srgb_values(self):
        colors=namespace['field_colors'](np.array([[0.,.5,1.]]),{'min':0,'max':1},{'colorLow':'#000000','colorHigh':'#ffffff'})
        expected=np.array([0.,((128/255+.055)/1.055)**2.4,1.])
        np.testing.assert_allclose(colors[0,:,0],expected,atol=1e-15)
        np.testing.assert_array_equal(colors[:,:,0],colors[:,:,1])

    def test_pin_allows_appended_frames_and_keeps_frozen_selection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);index,pin=fixture(root)
            index['frames'].append(dict(index['frames'][1],step=2,time=.2))
            index.update(end_time=.2,frame_count=3)
            (root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
            source=namespace['Fields'](root,pin)
            self.assertEqual(len(source.records),2)
            self.assertEqual(source.index_sha256,pin['index_sha256'])
            self.assertEqual(source.sample(.05)[1]['display_time'],0.)
            with self.assertRaisesRegex(ValueError,'different pinned'):
                source.sample(.1)

    def test_pin_rejects_metadata_entry_snapshot_and_loaded_byte_mutations(self):
        for mutation in ('metadata','entry','snapshot','numeric_bytes','view_bytes','pin_file_hash','missing_file_pin'):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);index,pin=fixture(root)
                record=index['frames'][0]
                if mutation=='metadata':index['lengths_um']=[40,8]
                elif mutation=='entry':record['step']=99
                elif mutation=='snapshot':pin['index_sha256']='0'*64
                elif mutation=='pin_file_hash':pin['files'][record['path']]='0'*64
                elif mutation=='missing_file_pin':del pin['files'][record['view_path']]
                (root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
                if mutation in ('metadata','entry','snapshot'):
                    with self.assertRaises(ValueError):namespace['Fields'](root,pin)
                    continue
                source=namespace['Fields'](root,pin)
                if mutation=='numeric_bytes':(root/record['path']).write_bytes(b'changed numeric state')
                elif mutation=='view_bytes':(root/record['view_path']).write_bytes(b'{}')
                with self.assertRaisesRegex(ValueError,'digest'):source.sample(0.)

    def test_all_field_records_require_committed_digests(self):
        for key in ('sha256','view_sha256'):
            for bad in (None,'','f'*63,'z'*64):
                with self.subTest(key=key,digest=bad),tempfile.TemporaryDirectory() as temporary:
                    root=Path(temporary);index,_=fixture(root)
                    index['frames'][0][key]=bad
                    (root/'fields/index.json').write_text(json.dumps(index),encoding='utf-8')
                    with self.assertRaisesRegex(ValueError,'digest'):namespace['Fields'](root)

    def test_labels_toggle_hides_every_hud_object(self):
        objects=[SimpleNamespace(name=name,hide_render=False,hide_viewport=False) for name in
                 ('Title backing','Legend backing','Saved numerical field label',
                  'Fixed numerical color scale','Quantitative legend')]
        namespace['set_hud_visibility'](objects,False)
        self.assertTrue(all(obj.hide_render and obj.hide_viewport for obj in objects))
        namespace['set_hud_visibility'](objects,True)
        self.assertTrue(all(not obj.hide_render and not obj.hide_viewport for obj in objects))
        with self.assertRaises(ValueError):namespace['set_hud_visibility'](objects,'false')


if __name__=='__main__':unittest.main()
