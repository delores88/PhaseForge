"""Executed offline tests of the actual independent Python verifier and integrity checker."""
from pathlib import Path
import importlib.util,json,math,tempfile,unittest,hashlib,subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);out=importlib.util.module_from_spec(spec);spec.loader.exec_module(out);return out
ref=module('reference_ode',ROOT/'tools/reference_ode.py');integrity=module('verify_bundle',ROOT/'tools/verify_bundle.py')
def draft(method='rk4'):
    return {'id':'control','model':{'kind':'state_vector_ode','variables':[{'name':'x','initial':1.0}],
            'derivatives':[{'variable':'x','expression':'-rate*x'}]},'constants':[{'name':'rate','value':1}],
            'integration':{'method':method,'start_time':0,'end_time':1,'time_step':.01,'max_steps':101},
            'search':{'variables':[]},'observables':[{'name':'endpoint','expression':'x'}],
            'constraints':[{'name':'bound','expression':'abs(x)','tolerance':1}]}
class ExpressionTests(unittest.TestCase):
    def test_precedence(self): self.assertEqual(ref.expression('-2^2')({}),-4);self.assertEqual(ref.expression('2^3^2')({}),512)
    def test_negative_power(self): self.assertEqual(ref.expression('2^-2')({}),.25)
    def test_sign_and_round(self): self.assertEqual(ref.expression('round(-2.5)')({}),-3);self.assertEqual(ref.expression('sign(0)')({}),1)
    def test_functions(self): self.assertAlmostEqual(ref.expression('log(e)+clamp(5,0,2)+sqrt(9)')({}),6)
    def test_forbidden_nodes(self):
        for text in ['__import__("os").system("echo no")','x[0]','[x for x in range(5)]','(lambda:1)()','x if y else z']:
            with self.subTest(text=text),self.assertRaises(ValueError):ref.expression(text)
    def test_arity(self):
        with self.assertRaises(ValueError):ref.expression('sin(1,2)')
    def test_missing_variable(self):
        with self.assertRaises(KeyError):ref.expression('missing+1')({})
    def test_nonfinite_literal(self):
        with self.assertRaises(ValueError):ref.expression('1e999')
class ReplayTests(unittest.TestCase):
    def test_analytic_decay(self):
        r=ref.replay(draft());self.assertTrue(r['reached_end_time']);self.assertAlmostEqual(r['metrics']['final_x'],math.exp(-1),places=9);self.assertTrue(r['constraints'][0]['passed'])
    def test_euler(self):self.assertAlmostEqual(ref.replay(draft('euler'))['metrics']['final_x'],.99**100,places=12)
    def test_refinement_improves_error(self):
        d=draft();d['integration']['time_step']=.1;a=ref.replay(d);b=ref.replay(d,refinement=2);self.assertLess(abs(b['metrics']['endpoint']-math.exp(-1)),abs(a['metrics']['endpoint']-math.exp(-1)))
    def test_winning_target(self):
        d=draft();d['search']['variables']=[{'name':'k','target':'constant:rate'}];r=ref.replay(d,{'result':{'best_candidate':{'k':2}}});self.assertAlmostEqual(r['metrics']['endpoint'],math.exp(-2),places=8)
    def test_incomplete_rejected(self):
        d=draft();d['integration']['max_steps']=2
        with self.assertRaises(ValueError):ref.replay(d)
    def test_partial_final_step(self):
        d=draft();d['integration']['time_step']=.3;self.assertEqual(ref.replay(d)['actual_end_time'],1)
    def test_nonode_is_not_faked(self):
        d=draft();d['model']['kind']='pairwise_particles'
        with self.assertRaises(ValueError):ref.replay(d)
    def test_nonfinite_state_rejected(self):
        d=draft();d['model']['variables'][0]['initial']=float('inf')
        with self.assertRaises(ValueError):ref.replay(d)
    def test_wrong_derivative_mapping(self):
        d=draft();d['model']['derivatives'][0]['variable']='wrong'
        with self.assertRaises(ValueError):ref.replay(d)
    def test_cli_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);(p/'m.json').write_text(json.dumps(draft()));(p/'r.json').write_text(json.dumps({'manifest_id':'wrong'}))
            run=subprocess.run([sys.executable,str(ROOT/'tools/reference_ode.py'),str(p/'m.json'),'--run',str(p/'r.json')],capture_output=True,text=True)
            self.assertNotEqual(run.returncode,0);self.assertIn('provenance mismatch',run.stderr)
class IntegrityTests(unittest.TestCase):
    def fixture(self,folder):
        root=Path(folder);(root/'data.txt').write_text('evidence');(root/'checksums.json').write_text(json.dumps({'data.txt':hashlib.sha256(b'evidence').hexdigest()}));return root
    def test_complete(self):
        with tempfile.TemporaryDirectory() as f:self.assertEqual(integrity.verify(self.fixture(f)),[])
    def test_tamper(self):
        with tempfile.TemporaryDirectory() as f:
            root=self.fixture(f);(root/'data.txt').write_text('changed');self.assertTrue(integrity.verify(root))
    def test_missing(self):
        with tempfile.TemporaryDirectory() as f:
            root=self.fixture(f);(root/'data.txt').unlink();self.assertTrue(integrity.verify(root))
    def test_extra(self):
        with tempfile.TemporaryDirectory() as f:
            root=self.fixture(f);(root/'secret.txt').write_text('not distributed');self.assertTrue(integrity.verify(root))
    def test_unsafe_path(self):
        with tempfile.TemporaryDirectory() as f:
            root=self.fixture(f);(root/'checksums.json').write_text(json.dumps({'../escape':'0'*64}));self.assertTrue(integrity.verify(root))
if __name__=='__main__':unittest.main(verbosity=2)
