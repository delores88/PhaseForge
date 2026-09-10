"""Real standard-library ODE verification tests. No model API, mocked solver, or source-text tests."""
from pathlib import Path
import copy,hashlib,importlib.util,json,math,os,shutil,subprocess,sys,tempfile,time,unittest
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('worker',ROOT/'tools/verification_worker.py')
w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)

def manifest(method='rk4',step=.05):
    return {'id':'candidate','project_id':'world','model':{'kind':'state_vector_ode','variables':[{'name':'x','initial':1.}],
        'derivatives':[{'variable':'x','expression':'-rate*x'}]},'constants':[{'name':'rate','value':1.}],
        'integration':{'method':method,'start_time':0.,'end_time':1.,'time_step':step,'max_steps':100001,'output_stride':1},
        'search':{'enabled':False,'variables':[]},'observables':[{'name':'endpoint','expression':'x'},{'name':'sample_average','expression':'mean_x'}],
        'constraints':[{'name':'finite_bound','expression':'abs(x)','tolerance':2.}]}

def protocol():
    return {'metrics':[{'metric':'endpoint','scale':1.,'absolute_tolerance':1e-6,'relative_tolerance':1e-5,'robustness_absolute':.1}],
        'holdout_replicates':4,'perturb_fraction':.01,'holdout_seed':219,'wall_seconds':30,'max_rhs_evaluations':100000,
        'solver_absolute_tolerance':1e-11,'solver_relative_tolerance':1e-9,'duplicate_distance':.01}

def payload(method='rk4'):
    m=manifest(method);r=w.ref.replay(m)
    return {'protocol':protocol(),'manifest':m,'source_run':{'id':'run','manifest_id':'candidate','project_id':'world','status':'completed',
        'result':{'metrics':r['metrics'],'numerical':{'reached_end_time':True}}},'controls':[],
        'parameters':[{'name':'r','target':'constant:rate','minimum':.5,'maximum':1.5}],
        'protocol_hash':'fixture-protocol','worker_source_hash':w.digest_file(ROOT/'tools/verification_worker.py'),
        'reference_source_hash':w.digest_file(ROOT/'tools/reference_ode.py')}

def adaptive(m=None,atol=1e-11,rtol=1e-9):
    return w.adaptive_replay(m or manifest(),w.Budget(100000,30),atol,rtol)

class AdaptiveTests(unittest.TestCase):
    def test_analytic_decay(self):
        r=adaptive();self.assertTrue(r['reached_end_time']);self.assertAlmostEqual(r['metrics']['endpoint'],math.exp(-1),places=9)
    def test_multistate_oscillator(self):
        m=manifest();m['model']={'kind':'state_vector_ode','variables':[{'name':'x','initial':1.},{'name':'v','initial':0.}],
            'derivatives':[{'variable':'x','expression':'v'},{'variable':'v','expression':'-x'}]}
        r=adaptive(m);self.assertAlmostEqual(r['metrics']['final_x'],math.cos(1),places=9);self.assertAlmostEqual(r['metrics']['final_v'],-math.sin(1),places=9)
    def test_observation_grid_not_internal_steps(self):
        m=manifest(step=.2);r=adaptive(m);expected=sum(math.exp(-i*.2) for i in range(6))/6
        self.assertEqual(r['steps'],5);self.assertGreater(r['accepted_internal_steps'],5);self.assertAlmostEqual(r['metrics']['sample_average'],expected,places=9)
    def test_partial_final_step(self):
        r=adaptive(manifest(step=.3));self.assertEqual(r['actual_end_time'],1.);self.assertEqual(r['steps'],4)
    def test_adaptive_rejections_are_reported(self):
        m=manifest(step=.5);m['constants'][0]['value']=20.
        r=adaptive(m);self.assertGreater(r['rejected_internal_steps'],0);self.assertLess(abs(r['metrics']['endpoint']-math.exp(-20)),1e-10)
    def test_tighter_tolerance_improves_answer(self):
        m=manifest(step=.5);a=adaptive(m,1e-3,1e-3);b=adaptive(m,1e-12,1e-12)
        self.assertLess(abs(b['metrics']['endpoint']-math.exp(-1)),abs(a['metrics']['endpoint']-math.exp(-1)))
    def test_step_budget_rejected(self):
        m=manifest();m['integration']['max_steps']=2
        with self.assertRaises(ValueError):adaptive(m)
    def test_unsupported_physics_rejected(self):
        m=manifest();m['model']['kind']='pairwise_particles'
        with self.assertRaises(ValueError):adaptive(m)
    def test_expression_cannot_access_host(self):
        m=manifest();m['model']['derivatives'][0]['expression']='__import__("os").system("echo no")'
        with self.assertRaises(ValueError):adaptive(m)
    def test_nonfinite_rejected(self):
        m=manifest();m['model']['variables'][0]['initial']=float('inf')
        with self.assertRaises(ValueError):adaptive(m)
    def test_duplicate_state_rejected(self):
        m=manifest();m['model']['variables'].append({'name':'x','initial':2})
        with self.assertRaises(ValueError):adaptive(m)
    def test_alias_preserved(self):
        m=manifest();m['search']['variables']=[{'name':'k','target':'constant:rate'}];m['observables'].append({'name':'alias','expression':'k'})
        self.assertEqual(adaptive(m)['metrics']['alias'],1.)
    def test_calibration_against_scipy_when_available(self):
        try:from scipy.integrate import solve_ivp
        except ImportError:self.skipTest('optional independent installed SciPy comparison')
        expected=solve_ivp(lambda t,y:[-y[0]],(0,1),[1.],method='DOP853',rtol=1e-12,atol=1e-12).y[0,-1]
        self.assertAlmostEqual(adaptive()['metrics']['endpoint'],expected,places=9)

class ProcessTests(unittest.TestCase):
    def test_nominal_independent_agreement(self):
        r=w.evaluate(payload());self.assertEqual(r['state'],'completed');self.assertTrue(r['independent_agreement']);self.assertEqual(r['robustness'],'within_declared_limits')
    def test_distinct_methods_can_disagree(self):
        r=w.evaluate(payload('euler'));self.assertEqual(r['state'],'completed');self.assertEqual(r['same_method']['status'],'agreement');self.assertFalse(r['independent_agreement'])
    def test_missing_measurement_is_not_zero(self):
        p=payload();p['source_run']['result']['metrics'].pop('endpoint');r=w.evaluate(p)
        self.assertEqual(r['same_method']['comparisons'][0]['status'],'inconclusive');self.assertFalse(r['independent_agreement'])
    def test_holdouts_reproducible_distinct_bounded(self):
        p=payload();a=w.evaluate(p);b=w.evaluate(p);av=[h['values'] for h in a['holdouts']];bv=[h['values'] for h in b['holdouts']]
        self.assertEqual(av,bv);self.assertEqual(len({tuple(x) for x in av}),4)
        self.assertTrue(all(.99<=x[0]<=1.01 for x in av));self.assertTrue(all(h['changed'] for h in a['holdouts']))
    def test_outside_parameter_bounds_rejected(self):
        p=payload();p['parameters'][0]['minimum']=2.;p['parameters'][0]['maximum']=3.
        with self.assertRaises(ValueError):w.evaluate(p)
    def test_new_seed_changes_holdouts(self):
        p=payload();a=w.evaluate(p);p['protocol']['holdout_seed']+=1;b=w.evaluate(p)
        self.assertNotEqual(a['holdouts'][0]['values'],b['holdouts'][0]['values'])
    def test_tight_robustness_rejects_change(self):
        p=payload();p['protocol']['metrics'][0]['robustness_absolute']=0.;r=w.evaluate(p)
        self.assertEqual(r['robustness'],'sensitive_or_invalid')
    def test_controls_absent_not_pass(self):self.assertFalse(w.evaluate(payload())['controls_passed'])
    def test_declared_control_intervals_pass_and_fail(self):
        for passing in (True,False):
            p=payload();m=manifest();m['id']='control';cr={'id':'controlrun','manifest_id':'control','status':'completed','project_id':'world'}
            p['controls']=[{'spec':{'run_id':'controlrun','metric':'endpoint','minimum':.3678 if passing else 10,'maximum':.368 if passing else 11,'rationale':'analytic fixture'},'manifest':m,'run':cr}]
            self.assertEqual(w.evaluate(p)['controls_passed'],passing)
    def test_reference_implementation_charges_actual_calls(self):
        budget=w.Budget(1000,30)
        result=w.ref.replay(manifest(),consume_rhs=budget.charge)
        self.assertEqual(budget.used,4*result['steps'])
        limited=w.Budget(100,30)
        with self.assertRaises(TimeoutError):w.ref.replay(manifest(step=.001),consume_rhs=limited.charge)
        self.assertEqual(limited.used,100)
    def test_rhs_budget_covers_all_phases(self):
        p=payload();p['protocol']['max_rhs_evaluations']=250;r=w.evaluate(p)
        self.assertEqual(r['state'],'budget_exhausted');self.assertIsNotNone(r['same_method']);self.assertLessEqual(r['rhs_evaluations'],250)
    def test_bad_run_provenance_rejected(self):
        p=payload();p['source_run']['manifest_id']='wrong'
        with self.assertRaises(ValueError):w.evaluate(p)
    def test_incomplete_source_rejected(self):
        p=payload();p['source_run']['result']['numerical']['reached_end_time']=False
        with self.assertRaises(ValueError):w.evaluate(p)
    def test_nested_search_not_replayed(self):
        p=payload();p['manifest']['search']['enabled']=True
        with self.assertRaises(ValueError):w.evaluate(p)
    def test_completed_checkpoints_survive_later_budget_exhaustion(self):
        p=payload();p['protocol']['max_rhs_evaluations']=250;kept=[]
        result=w.evaluate(p,checkpoint=lambda r:kept.append(copy.deepcopy(r)))
        self.assertEqual(result['state'],'budget_exhausted');self.assertEqual(len(kept),2);self.assertIsNotNone(kept[0]['same_method']);self.assertIsNotNone(kept[-1]['different_method']);self.assertNotIn('independent_agreement',kept[-1])
    def test_progress_has_real_task_counts(self):
        events=[];r=w.evaluate(payload(),lambda *event:events.append(event));self.assertEqual(events[-1][1:],(6,6));self.assertEqual(r['state'],'completed')
    def test_budget_deadline(self):
        b=w.Budget(100,1);b.deadline=time.monotonic()-1
        with self.assertRaises(TimeoutError):b.charge()
    def test_near_zero_absolute_threshold(self):
        rule={'metric':'x','absolute_tolerance':1e-8,'relative_tolerance':0.,'robustness_absolute':1e-6}
        self.assertEqual(w.checks({'x':0.},{'x':1e-9},[rule])[0]['status'],'agreement')
    def test_nonfinite_does_not_pass(self):
        rules=[{'metric':'x','absolute_tolerance':0,'relative_tolerance':0,'robustness_absolute':0}]
        self.assertEqual(w.checks({'x':1e308},{'x':-1e308},rules)[0]['status'],'inconclusive')
    def test_cli_runs_real_isolated_worker_and_checks_provenance(self):
        with tempfile.TemporaryDirectory() as temp:
            d=Path(temp)
            for name in ['reference_ode.py','verification_worker.py']:shutil.copyfile(ROOT/'tools'/name,d/name)
            raw=json.dumps(payload()).encode();(d/'input.json').write_bytes(raw)
            run=subprocess.run([sys.executable,'-I',str(d/'verification_worker.py'),str(d)],capture_output=True,text=True,timeout=30)
            self.assertEqual(run.returncode,0,run.stderr);result=json.loads((d/'output.json').read_text())
            self.assertEqual(result['input_sha256'],hashlib.sha256(raw).hexdigest());self.assertTrue(result['independent_agreement'])
            self.assertEqual(json.loads((d/'progress.json').read_text())['completed'],6)
            checkpoint=json.loads((d/'checkpoint.json').read_text());self.assertTrue(checkpoint['partial']);self.assertEqual(len(checkpoint['holdouts']),4)
    def test_cli_rejects_tampered_worker(self):
        with tempfile.TemporaryDirectory() as temp:
            d=Path(temp)
            for name in ['reference_ode.py','verification_worker.py']:shutil.copyfile(ROOT/'tools'/name,d/name)
            p=payload();p['worker_source_hash']='wrong';(d/'input.json').write_text(json.dumps(p))
            run=subprocess.run([sys.executable,'-I',str(d/'verification_worker.py'),str(d)],capture_output=True,text=True,timeout=30)
            self.assertNotEqual(run.returncode,0);self.assertFalse((d/'output.json').exists())

if __name__=='__main__':unittest.main(verbosity=2)
