"""Independent arithmetic and synthetic retained-format comparison tests.

No CPU/CUDA process, kernel, cgroup or live scientific job is started here.
"""
import copy
import json
import math
import unittest

import numpy as np
import compare_nr_backends as compare
import test_nr_black_hole_result as fixtures


def simple_frame(values):
    values=np.asarray(values,dtype='<f4').reshape(1,1,-1)
    return {'time':.5,'cycle':2,'variables':['field'],'root_shape':(values.size,1,1),'block_shape':(values.size,1,1),
            'nghost':0,'location_bytes':8,'variable_bytes':4,'blocks':[{'logical':(0,0,0,0),'index':(0,values.size-1,0,0,0,0),
            'geometry':(0.,1.,0.,1.,0.,1.),'coordinates':[np.arange(values.size,dtype=float),np.array([.5]),np.array([.5])],
            'fields':{'field':values}}]}


class ArithmeticTests(unittest.TestCase):
    def test_exact_ulp_nextafter_signed_zero_and_sign_reversal(self):
        a=np.array([1.,-1.,0.,-0.],dtype='<f4')
        b=np.array([np.nextafter(np.float32(1),np.float32(2)),-1.,-0.,0.],dtype='<f4')
        result=compare.ulp_statistics(a,b)
        self.assertEqual(result['maximum'],1);self.assertEqual(result['different_values'],1)
        self.assertEqual(result['signed_zero_changes'],2);self.assertEqual(result['nonzero_sign_changes'],0)
        flip=compare.ulp_statistics(np.array([1.],dtype='<f4'),np.array([-1.],dtype='<f4'))
        self.assertEqual(flip['nonzero_sign_changes'],1);self.assertEqual(flip['maximum'],2*0x3f800000)

    def test_independent_norms_and_frozen_combined_gate(self):
        a=simple_frame([1.,2.]);b=simple_frame([1.,-2.]);result=compare.compare_fields(a,b)['field']
        self.assertEqual(result['max_absolute_difference'],4.)
        self.assertAlmostEqual(result['difference_rms'],math.sqrt(8.));self.assertAlmostEqual(result['relative_l2_to_cpu'],math.sqrt(16/5))
        self.assertEqual(result['absolute_tolerance'],64*2**-23*2)
        self.assertAlmostEqual(result['rms_tolerance'],2e-4*math.sqrt(2.5));self.assertFalse(result['passed'])
        tiny=compare.compare_fields(simple_frame([0.]),simple_frame([1e-8]))['field']
        self.assertLess(tiny['max_absolute_difference'],tiny['absolute_tolerance'])
        self.assertFalse(tiny['passed']);self.assertIsNone(tiny['relative_l2_to_cpu'])
        self.assertTrue(tiny['relative_l2_denominator_zero'])

    def test_nonfinite_geometry_precision_and_variable_mismatch_are_rejected(self):
        for alter in [lambda f:f.update(variable_bytes=8),lambda f:f.update(variables=['another']),lambda f:f.update(cycle=99),
                      lambda f:f['blocks'][0].update(geometry=(0.,2.,0.,1.,0.,1.)),
                      lambda f:f['blocks'][0]['fields']['field'].__setitem__((0,0,0),np.nan)]:
            a=simple_frame([1.,2.]);b=copy.deepcopy(a);alter(b)
            with self.assertRaises(ValueError):compare.compare_fields(a,b)


class RetainedComparisonTests(unittest.TestCase):
    def setUp(self):
        self.cpu=fixtures.BlackHoleResultTests();self.cpu.setUp()
        self.cuda=fixtures.BlackHoleResultTests();self.cuda.setUp()

    def tearDown(self):
        self.cpu.tearDown();self.cuda.tearDown()

    def produce_pair(self):
        cpu_result=self.cpu.produce()
        q,r=self.cuda.v2(cuda=True)
        # Distinct synthetic executable identity; this is not a CUDA execution.
        manifest=compare.retained.parse((self.cuda.root/'engine.json').read_bytes())
        manifest['executable']={'path':'/trusted/synthetic-cuda-never-executed','sha256':'e'*64}
        (self.cuda.root/'engine.json').write_text(json.dumps(manifest))
        digest=compare.retained.sha((self.cuda.root/'engine.json').read_bytes())
        q['engine_manifest_sha256']=r['engine_manifest_sha256']=digest
        r['executable']=manifest['executable']|{'bytes':123}
        self.cuda.save_v2(q,r);gpu_result=self.cuda.produce()
        return cpu_result,gpu_result

    def run_comparison(self):
        return compare.compare(self.cpu.root/'result',compare.decoder.sha256(self.cpu.root/'result/result.json'),
                               self.cuda.root/'result',compare.decoder.sha256(self.cuda.root/'result/result.json'))

    def test_identical_retained_arrays_have_two_state_agreement_without_physics_claims(self):
        self.produce_pair();result=self.run_comparison()
        self.assertTrue(result['common_state_agreement']);self.assertTrue(result['full_recorded_trajectory_agreement'])
        self.assertEqual(result['errors'],[]);self.assertEqual(len(result['fields']),4);self.assertEqual(len(result['derived_constraints']),2)
        self.assertEqual(result['common_states'],[{'time':0.,'cycle':0},{'time':.5126953125,'cycle':35}])
        self.assertTrue(all(s['max_absolute_difference']==0 and s['ulp']['maximum']==0 for row in result['fields'] for s in row['fields'].values()))
        for row in result['derived_constraints']:
            self.assertEqual(row['cpu']['H_rms'],2.);self.assertEqual(row['cuda']['M_rms'],3.)
            self.assertTrue(row['positive_physical_metrics'])
        self.assertFalse(result['physical_accuracy_validated']);self.assertFalse(result['physical_boost_validated']);self.assertIsNone(result['kernel_execution_observed'])
        self.assertEqual(result['criteria_sha256'],compare.criteria_sha256())

    def test_sign_reversal_fails_fields_even_when_constraint_rms_is_unchanged(self):
        for n,(time,cycle) in enumerate(((0.,0),(.5126953125,35))):
            con={'con_H':np.full((2,2,2),-2.),'con_M':np.full((2,2,2),9.)}
            con['con_H'][0,0,0]=-1000;con['con_M'][0,0,0]=1000000
            fixtures.write_native(self.cuda.run/f'bin/fixture.con.{n:05d}.bin',con,time,cycle,self.cuda.header)
        self.produce_pair();result=self.run_comparison()
        self.assertFalse(result['common_state_agreement']);self.assertEqual(result['errors'],[]);self.assertEqual(len(result['derived_constraints']),2)
        self.assertTrue(all(row['passed'] for row in result['derived_constraints']))
        h=[row['fields']['con_H'] for row in result['fields'] if row['kind']=='constraints']
        self.assertEqual(len(h),2)
        self.assertTrue(all(row['ulp']['nonzero_sign_changes']==8 for row in h));self.assertTrue(all(not row['passed'] for row in h))

    def test_extra_gpu_coverage_is_not_a_numerical_disagreement(self):
        frame=compare.decoder.read_binary(self.cuda.run/'bin/fixture.con.00000.bin')
        fixtures.write_native(self.cuda.run/'bin/fixture.z4c.00002.bin',self.cuda.metric,1.,68,self.cuda.header)
        fixtures.write_native(self.cuda.run/'bin/fixture.con.00002.bin',frame['blocks'][0]['fields'],1.,68,self.cuda.header)
        self.produce_pair();result=self.run_comparison()
        self.assertTrue(result['common_state_agreement']);self.assertFalse(result['full_recorded_trajectory_agreement'])
        self.assertEqual(len(result['unmatched_cuda']),2);self.assertEqual(result['unmatched_cpu'],[])

    def test_initial_only_is_insufficient_even_with_zero_differences(self):
        (self.cuda.run/'bin/fixture.z4c.00001.bin').unlink();(self.cuda.run/'bin/fixture.con.00001.bin').unlink()
        self.produce_pair();result=self.run_comparison()
        self.assertFalse(result['common_state_agreement']);self.assertFalse(result['sufficient_nonzero_common_evolution'])
        self.assertEqual(result['errors'],[]);self.assertEqual(len(result['fields']),2)
        self.assertTrue(all(row['passed'] for row in result['fields']))

    def test_exact_input_bytes_required_even_for_semantically_equal_comments(self):
        self.cuda.header+='\n# additional comment with no physics change\n'
        (self.cuda.root/'input.athinput').write_text(self.cuda.header);self.cuda.make_fields()
        self.produce_pair()
        with self.assertRaisesRegex(ValueError,'byte-identical'):self.run_comparison()

    def test_native_headers_may_append_runtime_defaults_but_not_change_declared_input(self):
        header=self.cuda.header+'\n<solver_runtime>\noutput_count = 35\n'
        con=compare.decoder.read_binary(self.cuda.run/'bin/fixture.con.00000.bin')['blocks'][0]['fields']
        for n,(time,cycle) in enumerate(((0.,0),(.5126953125,35))):
            fixtures.write_native(self.cuda.run/f'bin/fixture.z4c.{n:05d}.bin',self.cuda.metric,time,cycle,header)
            fixtures.write_native(self.cuda.run/f'bin/fixture.con.{n:05d}.bin',con,time,cycle,header)
        self.produce_pair();self.assertTrue(self.run_comparison()['common_state_agreement'])

    def test_changed_declared_native_setting_is_not_treated_as_an_added_default(self):
        header=self.cuda.header.replace('par_P_plus1 = -0.025','par_P_plus1 = -0.03')
        con=compare.decoder.read_binary(self.cuda.run/'bin/fixture.con.00000.bin')['blocks'][0]['fields']
        for n,(time,cycle) in enumerate(((0.,0),(.5126953125,35))):
            fixtures.write_native(self.cuda.run/f'bin/fixture.z4c.{n:05d}.bin',self.cuda.metric,time,cycle,header)
            fixtures.write_native(self.cuda.run/f'bin/fixture.con.{n:05d}.bin',con,time,cycle,header)
        self.produce_pair();result=self.run_comparison()
        self.assertFalse(result['common_state_agreement']);self.assertGreaterEqual(len(result['errors']),4)
        self.assertTrue(all('declared input' in row['error'] for row in result['errors']))

    def test_corruption_of_retained_native_pin_is_rejected(self):
        self.produce_pair();path=self.cuda.root/'result/native/bin/fixture.z4c.00001.bin'
        raw=path.read_bytes();path.chmod(0o600);path.write_bytes(raw[:-1]+bytes([raw[-1]^1]))
        with self.assertRaisesRegex(ValueError,'artifact mismatch'):self.run_comparison()


if __name__=='__main__':unittest.main()
