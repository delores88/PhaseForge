"""Native CAD volume/STEP roundtrip and KiCad DRC/export acceptance tests."""
import importlib.util,json,math,subprocess,sys,tempfile,time,unittest,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('fabrication',ROOT/'tools/fabrication_worker.py')
worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(worker)
SCHEMA=json.loads((ROOT/'docs/fabrication.schema.json').read_text(encoding='utf-8'))
def part(name,shape,size,position=(0,0,0),radius=0,operation='add'):
    return dict(id=name,label=name,operation=operation,shape=shape,position=list(position),rotation=[0,0,0],size=list(size),radius=radius,top_radius=0,points=[])
def plate():
    return dict(kind='cad',units='mm',description='40 x 30 x 3 mm instrument plate with four 3 mm through holes',cad=dict(parts=[part('plate','box',[40,30,3])]+[part(f'hole{i}','cylinder',[0,0,5],(x,y,0),1.5,'subtract') for i,(x,y) in enumerate([(-15,-10),(15,-10),(-15,10),(15,10)])],fillet_radius=0),pcb=None)
def board():
    def connector(ref,x):
        return dict(reference=ref,value='2-pin terminal',position=[x,10],rotation=0,body_size=[4,9,5],pads=[dict(number=str(i+1),net=n,position=[0,y],size=[1.8,1.8],drill=.8,shape='circle',kind='through_hole',layer='F.Cu') for i,(n,y) in enumerate([('VCC',-2.54),('GND',2.54)])])
    return dict(kind='pcb',units='mm',description='Two-net instrument interconnect board',cad=None,pcb=dict(width_mm=30,height_mm=20,thickness_mm=1.6,nets=['VCC','GND'],components=[connector('J1',5),connector('J2',25)],tracks=[dict(net=n,layer='F.Cu',width_mm=.4,points=[[5,y],[25,y]]) for n,y in [('VCC',7.46),('GND',12.54)]],mounting_holes=[],silkscreen=[]))
class NativeFabrication(unittest.TestCase):
    def test_native_cad_worker_exports_and_exits_cleanly(self):
        with tempfile.TemporaryDirectory(prefix='phaseforge-cad-process-') as folder:
            out=Path(folder);request=out/'request.json';request.write_text(json.dumps(plate()),encoding='utf-8')
            process=subprocess.run([sys.executable,str(ROOT/'tools/fabrication_worker.py'),'--input',str(request),'--schema',str(ROOT/'docs/fabrication.schema.json'),'--output',str(out),'--max-seconds','60'],capture_output=True,text=True,timeout=70)
            self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
            result=json.loads((out/'result.json').read_text(encoding='utf-8'))
            self.assertTrue(result['valid_solid']);self.assertGreater((out/'model.step').stat().st_size,1000);self.assertGreater((out/'model.stl').stat().st_size,1000)
    def test_cad_holes_preserve_volume_and_step_roundtrips(self):
        import cadquery as cq
        recipe=plate();worker.validate(recipe,SCHEMA)
        with tempfile.TemporaryDirectory(prefix='phaseforge-cad-') as folder:
            out=Path(folder);report=worker.build_cad(recipe,out);expected=40*30*3-4*math.pi*1.5**2*3
            self.assertTrue(report['valid_solid']);self.assertEqual(report['solid_count'],1);self.assertAlmostEqual(report['volume_mm3'],expected,places=6)
            restored=cq.importers.importStep(str(out/'model.step')).val()
            self.assertTrue(restored.isValid());self.assertAlmostEqual(restored.Volume(),expected,places=5);self.assertGreater((out/'model.stl').stat().st_size,1000)
    def test_invalid_geometry_and_net_references_are_rejected(self):
        bad=plate();bad['cad']['parts'][0]['size'][0]=-1
        with self.assertRaises(ValueError):worker.validate(bad,SCHEMA)
        bad=board();bad['pcb']['tracks'][0]['net']='TYPO'
        with self.assertRaises(ValueError):worker.validate(bad,SCHEMA)
        bad=plate();bad['cad']['parts'][0]['script']='import os'
        with self.assertRaises(Exception):worker.validate(bad,SCHEMA)
    def test_native_board_parses_checks_and_exports(self):
        self.assertIsNotNone(worker.kicad_path(),'Install KiCad for native acceptance');recipe=board();worker.validate(recipe,SCHEMA)
        with tempfile.TemporaryDirectory(prefix='phaseforge-pcb-') as folder:
            out=Path(folder);report=worker.build_pcb(recipe,out,time.monotonic()+180)
            self.assertTrue((out/'board.glb').is_file());self.assertTrue((out/'drc.json').is_file());self.assertEqual(report['drc_status'],'passed',json.loads((out/'drc.json').read_text(encoding='utf-8')))
            self.assertTrue(report['manufacturing_files']);self.assertTrue((out/'manufacturing.zip').is_file())
            with zipfile.ZipFile(out/'manufacturing.zip') as archive:
                names=archive.namelist();self.assertTrue(any(n.lower().endswith('.gbr') for n in names));self.assertTrue(any(n.lower().endswith('.drl') for n in names))
            with zipfile.ZipFile(out/'editable-project.zip') as archive:
                self.assertIn('PhaseForge.pretty/J1.kicad_mod',archive.namelist());self.assertIn('fp-lib-table',archive.namelist());self.assertIn('board.kicad_pro',archive.namelist())
    def test_rotated_footprint_and_mounting_hole_match_native_library(self):
        recipe=board();recipe['pcb']['components']=recipe['pcb']['components'][:1];recipe['pcb']['components'][0]['rotation']=37
        recipe['pcb']['tracks']=[];recipe['pcb']['mounting_holes']=[dict(position=[22,10],diameter_mm=2)]
        worker.validate(recipe,SCHEMA)
        with tempfile.TemporaryDirectory(prefix='phaseforge-pcb-rotated-') as folder:
            out=Path(folder);report=worker.build_pcb(recipe,out,time.monotonic()+180)
            self.assertEqual(report['drc_status'],'passed',json.loads((out/'drc.json').read_text(encoding='utf-8')))
            self.assertTrue((out/'PhaseForge.pretty/PhaseForgeHole1.kicad_mod').is_file())
            self.assertTrue(report['manufacturing_files'])
    def test_generated_footprint_names_cannot_escape_job_library(self):
        with tempfile.TemporaryDirectory(prefix='phaseforge-pcb-paths-') as folder:
            library=Path(folder)/'PhaseForge.pretty';library.mkdir()
            for reference in ['../outside','..\\outside','C:/outside','J1/../../outside','J1\n']:
                recipe=board();recipe['pcb']['components'][0]['reference']=reference
                with self.assertRaises(ValueError):worker.validate(recipe)
                with self.assertRaises(ValueError):worker.board_text(recipe['pcb'],library)
            self.assertEqual(list(library.iterdir()),[])
    def test_unconnected_board_does_not_export_manufacturing_files(self):
        self.assertIsNotNone(worker.kicad_path(),'Install KiCad for native acceptance');recipe=board();recipe['pcb']['tracks']=[]
        with tempfile.TemporaryDirectory(prefix='phaseforge-pcb-invalid-') as folder:
            out=Path(folder);report=worker.build_pcb(recipe,out,time.monotonic()+180)
            self.assertEqual(report['drc_status'],'issues_found');self.assertGreater(report['drc_issue_count'],0);self.assertFalse(report['manufacturing_files']);self.assertFalse((out/'manufacturing.zip').exists())
            drc=json.loads((out/'drc.json').read_text(encoding='utf-8'));self.assertTrue(drc['unconnected_items'])
            self.assertFalse(any(v['type']=='lib_footprint_mismatch' for v in drc['violations']))
if __name__=='__main__':unittest.main()
