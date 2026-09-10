"""Executed reference numerics and strict provider-schema tests; not Rust build tests."""
import math,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tools'))
import reference_ode as ref
import verification_worker as worker
from test_proposal_schema import draft,proposal,VALIDATOR

def measure(kind='maximum',name='peak',expression='x',threshold=0.,hysteresis=0.):
    return dict(name=name,expression=expression,reducer=kind,unit='model units',threshold=threshold,hysteresis=hysteresis)
def track(kind,samples,threshold=0.,band=0.):
    t=ref.TrajectoryTracker([measure(kind,threshold=threshold,hysteresis=band)],{'x':samples[0][1]},samples[0][0])
    for time,x in samples[1:]:t.observe({'x':x},time)
    return t.values()
def plan():
    return dict(title='Investigate a complex goal',goal='Discriminate competing explanations',tractable_question='Which measurable prediction distinguishes the hypotheses?',rationale='Evidence before a large simulation',knowns=[],unknowns=['Calibration data'],tasks=[dict(id='sources',title='Compare prior observations',kind='literature',question='What has actually been measured?',method='Compare retrieved abstracts and identify full-text needs',inputs=['Retrieved sources'],depends_on=[],success_criterion='Measurements and assumptions separated',deliverable='Evidence table',next_prompt='Review supplied source IDs and propose next study',search_query='mechanism measurements uncertainty')],source_ids=[],limitations=['Proposed work, not an observed discovery'])
class FrozenSourceCompatibility(unittest.TestCase):
    def test_original_worker_pair_retained_verbatim(self):
        import hashlib
        root=Path(__file__).resolve().parents[1]
        # Frozen old sources remain bundled, so old protocols need not be reinterpreted.
        original=root/'tools/legacy/0.5.0'
        expected={
            'reference_ode.py':'18b1c90a5bf3f469730744f0785574a5f1987acc095acb51f5600c6322620a4f',
            'verification_worker.py':'e906a32c310471759cc763d8795a771db202e4ecb9e7fee04e50809330173642'
        }
        for name,sha in expected.items():self.assertEqual(hashlib.sha256((original/name).read_bytes()).hexdigest(),sha)
    def test_new_worker_can_replay_new_measures_without_pip(self):
        m=draft();m['trajectory']=[measure('maximum')];m['model']['derivatives'][0]['expression']='1'
        self.assertAlmostEqual(ref.replay(m)['metrics']['peak'],1.0,places=9)

class Measurements(unittest.TestCase):
    def test_temporal_extrema_not_endpoint(self):
        self.assertEqual(track('maximum',[(0,0),(1,9),(2,0)])['peak'],9)
        self.assertEqual(track('minimum',[(0,0),(1,-3),(2,0)])['peak'],-3)
    def test_range(self):self.assertEqual(track('range',[(0,-1),(1,3),(2,0)])['peak'],4)
    def test_weighted_mean(self):self.assertEqual(track('time_mean',[(0,0),(1,2),(4,2)])['peak'],1.75)
    def test_integral(self):self.assertEqual(track('integral',[(0,0),(1,2),(4,2)])['peak'],7)
    def test_occupancy_interpolation(self):
        for kind in ['duration_below','duration_above']:self.assertEqual(track(kind,[(0,-1),(2,1)])['peak'],1)
    def test_initial_inside_not_an_entry(self):self.assertEqual(track('entries_below',[(0,-1),(1,-2)])['peak'],0)
    def test_repeated_encounters(self):self.assertEqual(track('entries_below',[(0,1),(1,-1),(2,1),(3,-1)])['peak'],2)
    def test_above_entries(self):self.assertEqual(track('entries_above',[(0,-1),(1,1),(2,-1),(3,1)])['peak'],2)
    def test_hysteresis(self):self.assertEqual(track('entries_below',[(0,1),(1,-.2),(2,.02),(3,-.02),(4,.2),(5,-.2)],band=.1)['peak'],2)
    def test_first_passage_censored(self):
        x=track('first_below',[(0,2),(2,1)]);self.assertEqual(x['peak'],2);self.assertEqual(x['peak_observed'],0)
    def test_temporary_exit_not_erased(self):
        x=track('first_above',[(0,-1),(1,1),(2,-1)]);self.assertEqual(x['peak'],1);self.assertEqual(x['peak_observed'],1)
    def test_initial_first_passage(self):self.assertEqual(track('first_below',[(4,-1),(6,-2)])['peak'],0)
    def test_threshold_plateau(self):self.assertEqual(track('duration_below',[(0,0),(1,0)])['peak'],0)
    def test_bad_times_and_nonfinite_rejected(self):
        for samples in [[(0,1),(0,2)],[(0,1),(1,float('nan'))]]:
            with self.assertRaises(ValueError):track('maximum',samples)
    def test_bad_spec_rejected(self):
        for kind,band in [('invented',0),('entries_above',-1)]:
            with self.assertRaises(ValueError):track(kind,[(0,1),(1,2)],band=band)
    def test_reducer_in_constraints_and_alias(self):
        m=draft();m['model']['derivatives'][0]['expression']='1';m['trajectory']=[measure()]
        m['observables']=[dict(name='max_state',expression='peak',unit='1')];m['constraints']=[dict(name='bound',expression='peak',tolerance=.4)]
        out=ref.replay(m);self.assertAlmostEqual(out['metrics']['peak'],1);self.assertAlmostEqual(out['metrics']['max_state'],1);self.assertFalse(out['constraints'][0]['passed'])
    def test_display_stride_cannot_change_measurement(self):
        m=draft();m['model']['derivatives'][0]['expression']='cos(t)';m['trajectory']=[measure()];m['integration'].update(end_time=6.28,time_step=.01,max_steps=630,output_stride=500)
        a=ref.replay(m)['metrics']['peak'];m['integration']['output_stride']=1;b=ref.replay(m)['metrics']['peak'];self.assertEqual(a,b);self.assertAlmostEqual(a,1,places=5)
    def test_refinement_grids_distinct(self):
        m=draft();m['model']['variables'][0]['initial']=1;m['model']['derivatives'][0]['expression']='-x';m['trajectory']=[measure('integral','area')]
        out=[ref.replay(m,refinement=f) for f in (1,2,4)];self.assertEqual([r['steps'] for r in out],[100,200,400])
        errors=[abs(r['metrics']['area']-(1-math.exp(-1))) for r in out];self.assertGreater(errors[0],errors[1]);self.assertGreater(errors[1],errors[2])
    def test_adaptive_comparison_same_observation_grid(self):
        m=draft();m['model']['derivatives'][0]['expression']='1';m['trajectory']=[measure('integral','area'),measure()]
        a=ref.replay(m);b=worker.adaptive_replay(m,worker.Budget(100000,10),1e-10,1e-8)
        for name in ['area','peak']:self.assertAlmostEqual(a['metrics'][name],b['metrics'][name],places=9)
    def test_128_state_reference(self):
        m=draft();m['model']['variables']=[dict(name=f'x{i}',initial=1) for i in range(128)];m['model']['derivatives']=[dict(variable=f'x{i}',expression='0') for i in range(128)];m['observables']=[];m['trajectory']=[measure(expression='x127')]
        self.assertEqual(ref.replay(m)['metrics']['peak'],1)
    def test_legacy_missing_field(self):m=draft();m.pop('trajectory');self.assertTrue(ref.replay(m)['reached_end_time'])
class ResearchSchema(unittest.TestCase):
    def test_research_programme_without_simulator(self):p=proposal();p.update(action='research_plan',manifest=None,research_plan=plan());VALIDATOR.validate(p)
    def test_gap_keeps_a_programme(self):p=proposal();p.update(action='capability_gap',manifest=None,research_plan=plan(),requested_capability='domain engine',capability_gap_reason='absent');VALIDATOR.validate(p)
    def test_success_criteria_required(self):
        p=proposal();p.update(action='research_plan',manifest=None,research_plan=plan());del p['research_plan']['tasks'][0]['success_criterion'];self.assertFalse(VALIDATOR.is_valid(p))
    def test_arbitrary_execution_not_added(self):
        p=proposal();p.update(action='research_plan',manifest=None,research_plan=plan());p['research_plan']['tasks'][0]['kind']='unrestricted_python';self.assertFalse(VALIDATOR.is_valid(p))
    def test_all_reducers_in_schema(self):
        for k in ref.TrajectoryTracker.OPERATIONS:p=proposal();p['manifest']['trajectory']=[measure(k)];VALIDATOR.validate(p)
    def test_radius_is_expression(self):
        p=proposal();p['manifest']['visualization']['entities']=[dict(id='body',x='x',y='0',z='0',vx='',vy='',vz='',radius='0.3')];VALIDATOR.validate(p)
        p['manifest']['visualization']['entities'][0]['radius']=.3;self.assertFalse(VALIDATOR.is_valid(p))
    def test_resolution_ladder_supported(self):
        p=proposal();p['manifest']['falsification']=[dict(name='resolution',kind='resolution_ladder',target=None,magnitude=0,repetitions=2,checks=[])];VALIDATOR.validate(p)
if __name__=='__main__':unittest.main(verbosity=2)
