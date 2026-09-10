"""Execute the JS manual-editor contract then integrate its actual emitted model
with the independent Python solver. This is NOT a native Rust acceptance test."""
import json,math,subprocess,sys,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
import reference_ode as ref
from test_proposal_schema import proposal,VALIDATOR
class EditorToNumerics(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  with tempfile.TemporaryDirectory() as td:
   path=Path(td)/'editor.json'
   process=subprocess.run(['node',str(ROOT/'tests/experiment-model.mjs'),str(path)],cwd=ROOT,capture_output=True,text=True,timeout=25)
   if process.returncode:raise AssertionError(process.stdout+'\n'+process.stderr)
   cls.manifest=json.loads(path.read_text())
 def test_native_editor_output_matches_provider_contract_after_optional_scene_default(self):
  # Native imports keep scene optional; provider generation requires explicit null.
  p=proposal();p['manifest']=json.loads(json.dumps(self.manifest));p['manifest']['visualization'].setdefault('scene',None);VALIDATOR.validate(p)
 def test_actual_js_model_integrates_analytic_decay(self):
  out=ref.replay(self.manifest);self.assertTrue(out['reached_end_time']);self.assertAlmostEqual(out['metrics']['final_x'],math.exp(-10),places=10)
 def test_finer_copy_is_distinct_and_numerical(self):
  base=ref.replay(self.manifest);finer=ref.replay(self.manifest,refinement=2)
  self.assertEqual(finer['steps'],2*base['steps']);self.assertLessEqual(abs(finer['metrics']['final_x']-math.exp(-10)),abs(base['metrics']['final_x']-math.exp(-10)))
 def test_no_plan_or_ai_execution_appears_in_math_input(self):
  self.assertNotIn('research_plan',self.manifest);self.assertFalse(self.manifest['search']['enabled']);self.assertEqual(self.manifest['compute']['preference'],'cpu')
 def test_manual_control_and_limits_are_preserved(self):
  self.assertEqual(self.manifest['model']['variables'][0]['initial'],1);self.assertEqual(self.manifest['model']['derivatives'][0]['expression'],'-rate*x');self.assertEqual(self.manifest['constants'][0]['value'],1)
  self.assertLessEqual(self.manifest['compute']['max_wall_seconds'],180);self.assertLessEqual(self.manifest['compute']['max_memory_mb'],1024)
if __name__=='__main__':unittest.main(verbosity=2)
