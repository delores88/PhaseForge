"""Schema regression suite; fixtures are tests only, never runtime experiments.
Requires Python 3 + jsonschema for this optional developer suite.
The Windows application/installer itself does not require Python.
"""
from pathlib import Path
import copy
import json
import unittest
from jsonschema import Draft202012Validator
ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT/'backend/src/agent/proposal.schema.json').read_text(encoding='utf-8'))
# Rust composes the provider schema with the canonical scene schema at runtime.
# Test the same complete contract, while keeping the generated baseline separate.
SCHEMA['$defs']['scene'] = json.loads((ROOT/'docs/scene.schema.json').read_text(encoding='utf-8'))
VISUALIZATION = SCHEMA['$defs']['manifest']['properties']['visualization']
VISUALIZATION['properties']['scene'] = {'anyOf':[{'$ref':'#/$defs/scene'},{'type':'null'}]}
VISUALIZATION['required'].append('scene')
VALIDATOR = Draft202012Validator(SCHEMA)

def draft():
    return dict(title='Schema regression fixture', question='Does this test contract validate?',
        scientific_boundary='A representation test, not evidence about nature.', hypothesis='No scientific claim.',
        model=dict(kind='state_vector_ode', variables=[dict(name='x',unit='',initial=0)],
                   derivatives=[dict(variable='x',expression='0')]), constants=[],trajectory=[],
        integration=dict(method='rk4',start_time=0,end_time=1,time_step=.01,output_stride=1,max_steps=100),
        search=dict(enabled=False,algorithm='none',variables=[],objectives=[],population=2,generations=1,elite_fraction=.2,mutation_scale=.1,seed=1),
        observables=[dict(name='state',expression='x',unit='')], constraints=[],
        visualization=dict(entities=[],kind='trajectory',x='t',y='x',z='0',point_size=1,max_frames=100,trails=True,scene=None),
        falsification=[], compute=dict(preference='cpu',policy='interactive',candidate_count=1,batch_size=0,max_wall_seconds=10,max_memory_mb=256),
        limitations=['Test fixture only.'])
def proposal():
    return dict(assistant_message='Structured proposal.',action='create_manifest',manifest=draft(),
        research_plan=None,requested_capability='',capability_gap_reason='',suggested_extension='',should_run=False)

class SchemaContractTests(unittest.TestCase):
    def test_schema_is_valid(self): Draft202012Validator.check_schema(SCHEMA)
    def test_complete_ode(self): VALIDATOR.validate(proposal())
    def test_complete_procedural_scene(self):
        p=proposal();p['manifest']['visualization']['scene']=self.scene();VALIDATOR.validate(p)
    @staticmethod
    def scene():
        return dict(schema_version='1.0',title='Geometry contract fixture',units='model units',
            provenance=dict(kind='conceptual',description='Authored geometry, not empirical measurements'),
            nodes=[dict(id='node',type='sphere',label='Representation',description=None,entity_id=None,
                position=[0,0,0],rotation=None,scale=None,color=None,parameters=None)],bonds=None,camera=None)
    def test_scene_cannot_introduce_executable_code_or_unbounded_nodes(self):
        for field in ['executable_code','node_type','node_limit']:
            with self.subTest(field=field):
                p=proposal();scene=self.scene();p['manifest']['visualization']['scene']=scene
                if field=='executable_code':scene['python_code']='forbidden execution field'
                elif field=='node_type':scene['nodes'][0]['type']='unrestricted_python'
                else:scene['nodes']=[dict(scene['nodes'][0],id=f'node-{i}') for i in range(65)]
                self.assertFalse(VALIDATOR.is_valid(p))
    def test_provider_requires_explicit_null_for_unused_scene(self):
        p=proposal();del p['manifest']['visualization']['scene'];self.assertFalse(VALIDATOR.is_valid(p))
    def test_every_integration_field_required(self):
        for field in SCHEMA['$defs']['integration']['required']:
            with self.subTest(field=field):
                p=proposal();del p['manifest']['integration'][field]
                self.assertFalse(VALIDATOR.is_valid(p))
    def test_exact_missing_start_time_rejected(self):
        p=proposal();del p['manifest']['integration']['start_time']
        errors=list(VALIDATOR.iter_errors(p))
        self.assertTrue(errors)
        self.assertTrue(any('start_time' in str(e.context) for e in errors))
    def test_manifest_string_rejected(self):
        p=proposal();p['manifest']=json.dumps(p['manifest']);self.assertFalse(VALIDATOR.is_valid(p))
    def test_old_manifest_json_rejected(self):
        p=proposal();p['manifest_json']=json.dumps(p.pop('manifest'));self.assertFalse(VALIDATOR.is_valid(p))
    def test_unexpected_properties_rejected(self):
        for target in ['root','integration','model','compute']:
            with self.subTest(target=target):
                p=proposal();node=p if target=='root' else p['manifest'][target];node['unexpected']=True
                self.assertFalse(VALIDATOR.is_valid(p))
    def test_invalid_method_rejected(self):
        p=proposal();p['manifest']['integration']['method']='made_up';self.assertFalse(VALIDATOR.is_valid(p))
    def test_numbers_must_not_be_strings(self):
        p=proposal();p['manifest']['integration']['start_time']='0';self.assertFalse(VALIDATOR.is_valid(p))
    def test_explanation_null_manifest(self):
        p=proposal();p['action']='explain';p['manifest']=None;VALIDATOR.validate(p)
    def test_all_distribution_variants(self):
        values=[dict(kind='explicit',values=[[0,0,0]]),dict(kind='uniform',min=[0,0,0],max=[1,1,1]),
                dict(kind='normal',mean=[0,0,0],std_dev=[1,1,1]),dict(kind='grid',extent=[1,1,1],spacing=1,jitter=0),
                dict(kind='sphere',radius=1,thickness=0)]
        for v in values:
            with self.subTest(kind=v['kind']):
                p=proposal();p['manifest']['integration']['method']='euler'
                p['manifest']['model']=dict(kind='pairwise_particles',dimensions=3,
                    population=dict(count=1,mass=1,position=v,velocity=copy.deepcopy(v)),
                    interaction=dict(radial_force='0',cutoff=None,softening=.01,linear_damping=0,external_acceleration=['0','0','0']),boundary=dict(kind='open'))
                VALIDATOR.validate(p)
    def test_every_object_closed(self):
        def walk(n):
            if isinstance(n,dict):
                if n.get('type')=='object':
                    self.assertIs(n.get('additionalProperties'),False)
                    self.assertEqual(set(n['properties']),set(n['required']))
                for c in n.values():walk(c)
            elif isinstance(n,list):
                for c in n:walk(c)
        walk(SCHEMA)
    def test_explicit_entity_mapping(self):
        p=proposal();p['manifest']['visualization']['entities']=[dict(id='trace',x='t',y='x',z='',vx='',vy='',vz='',radius='')]
        VALIDATOR.validate(p)
    def test_entity_mapping_missing_axis_rejected(self):
        p=proposal();p['manifest']['visualization']['entities']=[dict(id='trace',x='x',z='',vx='',vy='',vz='',radius='')]
        self.assertFalse(VALIDATOR.is_valid(p))
    def test_metric_comparison_contract(self):
        for expectation in ['stable','change','decrease','increase']:
            p=proposal();p['manifest']['falsification']=[dict(name='check',kind='step_halving',target=None,magnitude=0,repetitions=1,
                checks=[dict(metric='state',expectation=expectation,absolute_tolerance=1e-8,relative_tolerance=.01)])]
            VALIDATOR.validate(p)
    def test_missing_challenge_checks_rejected_for_new_provider_proposal(self):
        p=proposal();p['manifest']['falsification']=[dict(name='check',kind='step_halving',target=None,magnitude=0,repetitions=1)]
        self.assertFalse(VALIDATOR.is_valid(p))
    def test_empty_checks_explicitly_allowed(self):
        p=proposal();p['manifest']['falsification']=[dict(name='check',kind='step_halving',target=None,magnitude=0,repetitions=1,checks=[])]
        VALIDATOR.validate(p)
    def test_unrecognized_comparison_rule_rejected(self):
        p=proposal();p['manifest']['falsification']=[dict(name='check',kind='step_halving',target=None,magnitude=0,repetitions=1,
            checks=[dict(metric='state',expectation='always_pass',absolute_tolerance=0,relative_tolerance=0)])]
        self.assertFalse(VALIDATOR.is_valid(p))
    def test_schema_generation_is_reproducible(self):
        import subprocess,sys
        path=ROOT/'backend/src/agent/proposal.schema.json';old=path.read_bytes()
        self.assertNotIn(b'\r',old,'Generated schema must use LF bytes on every platform')
        subprocess.run([sys.executable,str(ROOT/'scripts/generate-proposal-schema.py')],check=True,capture_output=True)
        self.assertEqual(old,path.read_bytes())

if __name__=='__main__': unittest.main(verbosity=2)
