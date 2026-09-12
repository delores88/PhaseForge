"""Small retained-format fixtures. These never start an NR process."""
import json
import math
from pathlib import Path
import struct
import tempfile
import unittest
import uuid

import numpy as np
import nr_black_hole_result as reader


FINAL_STAGE_LABEL_SEMANTICS = 'pre_increment_mesh_time_for_final_RK_stage_state'


HEADER = '''<job>
basename = fixture
<mesh>
nx1 = 2
nx2 = 2
nx3 = 2
nghost = 2
<meshblock>
nx1 = 2
nx2 = 2
nx3 = 2
<time>
tlim = 1
<z4c>
excise_chi = 0.0625
<problem>
Newton_tol = 1e-10
adm_tol = 1e-10
give_bare_mass = false
par_P_plus1 = -0.025
par_P_minus1 = 0.025
<fastflow>
num_horizons = 1
'''


def write_native(path, fields, time, cycle, header=HEADER):
    params = header.encode()
    prefix = (f'Athena binary output version=1.1\nsize of preheader=5\ntime={time:.17e}\ncycle={cycle}\n'
              f'size of location=8\nsize of variable=4\nnumber of variables={len(fields)}\n'
              f'variables: {" ".join(fields)}\nheader offset={len(params)}\n').encode()
    # One AMR block, same geometry and exact cell coordinates for every channel.
    block = struct.pack('<6i4i6d', 2, 3, 2, 3, 2, 3, 0, 0, 0, 3, 0, 1, 0, 1, 0, 1)
    path.write_bytes(prefix+params+block+np.asarray(list(fields.values()), dtype='<f4').tobytes())


class BlackHoleResultTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
        self.run = self.root/'work'; self.run.mkdir(); (self.run/'bin').mkdir()
        self.job = str(uuid.uuid4()); self.header = HEADER
        self.make_fields()
        (self.root/'input.athinput').write_text(self.header)
        (self.run/'solver.stdout.log').write_text('Found bare masses.\nADM mass error: M_p_err=4e-12, M_m_err=4e-12\nNewton: it=1 |F|=3e-15\nThe two puncture masses are mp=0.48 and mm=0.48\nPuncture 1 ADM mass is 0.5\nPuncture 2 ADM mass is 0.5\nThe total ADM mass is 0.9622\n')
        verbose, summary = [], ['# columns']
        for n in range(35):
            time = n*0.0146484375
            verbose.append(f'time={time:.4f}, cycle=3\nSearching for horizon 0\ncenter = (3.000000, 0.000000, 0.000000)\n'+('Found horizon 0\n hrms = 0.04\n' if n == 0 else 'Failed, reached max iterations 100\n'))
            summary.append(f'3 {time:.6g} 0.5 0 0 0 0 {4*math.pi:.15e} 0.04 0 1 1')
        (self.run/'fixture.horizon_verbose_0.txt').write_text(''.join(verbose))
        (self.run/'fixture.horizon_summary_0.txt').write_text('\n'.join(summary)+'\n')
        (self.run/'fixture.horizon_shape_0.txt').write_text(f'# iter = 3, Time = 0\n{math.sqrt(4*math.pi):.15e}\n')
        (self.run/'fixture.horizon_grid_0.txt').write_text(f'# Theta Phi Weight\n{math.pi/2:.15e} 0 1\n')
        (self.run/'fixture.horizon_ylm_0.txt').write_text(f'# retained first7columns\n{math.pi/2:.15e} 0 0 0 {1/math.sqrt(4*math.pi):.15e} 0 0\n')
        (self.run/'waveforms').mkdir()
        labels = ['time']+[str(l)+str(m) for l in range(2, 9) for m in range(-l, l+1)]
        wh = '# '+' '.join(f'{n+1}:{v}' for n, v in enumerate(labels))+'\n'
        for part in ('real', 'imag'):
            (self.run/f'waveforms/rpsi4_{part}_0008.txt').write_text(wh+'0 '+' '.join(['0.001']*77)+'\n')
        self.build = {'mode': 'CPU serial double', 'gsl': '2.7.1', 'receipt_sha256': '1'*64}
        self.engine_id = 'athenak_two_punctures_serial'
        self.write_receipts()

    def tearDown(self):
        for path in self.root.rglob('*'):
            if path.is_file():
                path.chmod(0o600)
        self.temp.cleanup()

    def make_fields(self):
        shape = (2, 2, 2)
        self.metric = {f'z4c_g{a}': np.full(shape, 1 if a[0] == a[1] else 0) for a in ('xx', 'xy', 'xz', 'yy', 'yz', 'zz')}
        self.metric.update(z4c_chi=np.ones(shape), z4c_alpha=np.ones(shape))
        # One excised cell with deliberately extreme constraints; the other7
        # have volume1/8 and exact H RMS2, M RMS3. No analytic Einstein claim.
        self.metric['z4c_chi'][0, 0, 0] = 0.03125
        con = {'con_H': np.full(shape, 2.), 'con_M': np.full(shape, 9.)}
        con['con_H'][0, 0, 0] = 1000; con['con_M'][0, 0, 0] = 1000000
        for index, (time, cycle) in enumerate(((0., 0), (0.5126953125, 35))):
            write_native(self.run/f'bin/fixture.z4c.{index:05d}.bin', self.metric, time, cycle, self.header)
            write_native(self.run/f'bin/fixture.con.{index:05d}.bin', con, time, cycle, self.header)

    def write_receipts(self, completed=False, **overrides):
        manifest = {'schema': 'phaseforge.nr-engine.v1', 'engine_id': self.engine_id,
                    'executable': {'path': '/trusted/test-engine-never-executed', 'sha256': 'f'*64},
                    'source': {'athenak': reader.decoder.UPSTREAM_COMMIT, 'kokkos': reader.KOKKOS, 'twopunctures': reader.TWOPUNCTURES},
                    'build': self.build}
        (self.root/'engine.json').write_text(json.dumps(manifest)); mpin = reader.sha((self.root/'engine.json').read_bytes())
        inp = (self.root/'input.athinput').read_bytes()
        request = {'schema': 'phaseforge.nr-request.v1', 'job_id': self.job, 'engine_id': self.engine_id,
                   'engine_manifest_sha256': mpin, 'input': {'path': 'input.athinput', 'sha256': reader.sha(inp)},
                   'deadline_at': None, 'cpu_threads': 1, 'memory_limit_bytes': 2**30, 'output_limit_bytes': 2**30}
        (self.root/'request.json').write_text(json.dumps(request))
        receipt = {'schema': 'phaseforge.nr-process-receipt.v1', 'job_id': self.job,
                   'engine_id': self.engine_id, 'source': manifest['source'], 'build': manifest['build'],
                   'engine_manifest_sha256': mpin, 'request_sha256': reader.sha((self.root/'request.json').read_bytes()),
                   'executable': manifest['executable'] | {'bytes': 123}, 'input': request['input'] | {'bytes': len(inp)},
                   'deadline_at': None, 'cpu_threads': 1, 'memory_limit_bytes': 2**30, 'output_limit_bytes': 2**30,
                   'termination_reason': 'completed' if completed else 'deadline', 'exit_code': 0 if completed else -15,
                   'process_group_drained': True, 'files': [reader.pin(p, self.run) for p in sorted(self.run.rglob('*')) if p.is_file()]}
        receipt.update(overrides)
        self.receipt = self.root/'receipt.json'; self.receipt.write_text(json.dumps(receipt)); self.rpin = reader.sha(self.receipt.read_bytes())

    def produce(self, **kwargs):
        return reader.produce(self.run, self.receipt, self.rpin, self.root/'result', **kwargs)

    def test_partial_35_step_two_time_receipt_and_current_stale_surface_semantics(self):
        result = self.produce()
        self.assertEqual(result['paired_times'], [0., .5126953125]); self.assertFalse(result['execution']['complete'])
        self.assertEqual(result['execution']['termination_reason'], 'deadline'); self.assertFalse(result['execution']['retained_time_target_met'])
        measurements = reader.parse((self.root/'result/measurements.json').read_bytes())
        horizon = measurements['horizons'][0]
        self.assertEqual(horizon['found_flag_count'], 1); self.assertEqual(horizon['failure_count'], 34)
        self.assertEqual(horizon['stale_summary_count'], 34); self.assertEqual(len(horizon['attempts']), 35)
        last = horizon['attempts'][-1]
        self.assertIsNone(last['current_properties']); self.assertIsNone(last['surface']); self.assertFalse(last['horizon_validated'])
        self.assertEqual(last['reported_summary']['mass'], .5); self.assertEqual(last['reported_summary']['expansion_rms'], .2)
        fresh = horizon['attempts'][0]; self.assertIsNotNone(fresh['surface']); self.assertFalse(fresh['horizon_validated'])
        surface = reader.parse((self.root/'result'/fresh['surface']['path']).read_bytes())
        self.assertAlmostEqual(surface['points_xyz'][0][0], 4)
        self.assertEqual(measurements['initial_data']['status'], 'reported_solver_convergence')
        self.assertIsNone(measurements['initial_data']['horizon_rest_masses'])
        self.assertEqual(measurements['waveforms'][0]['quantity'], 'rPsi4')
        self.assertFalse(result['fulfillment']['requested_0_999c_collision'])
        frames = reader.parse((self.root/'result/frames/index.json').read_bytes())['frames']
        self.assertEqual([r['cycle'] for r in frames], [0, 0, 35, 35])
        meta = reader.parse((self.root/'result'/frames[0]['metadata']['path']).read_bytes())
        self.assertEqual(meta['blocks'][0]['native_logical_level'], 3)
        self.assertEqual(meta['blocks'][0]['coordinates'], [[.25, .75]]*3)
        # The native t=0 frame is separate from an AHF/wave row labelled zero.
        # Preserve labels and pins without inventing a timestep correspondence.
        self.assertEqual([frame['time'] for frame in frames if frame['cycle'] == 0], [0., 0.])
        for attempt in horizon['attempts']:
            self.assertEqual(attempt['native_label_time'], attempt['time'])
            self.assertEqual(attempt['label_semantics'], FINAL_STAGE_LABEL_SEMANTICS)
            self.assertIsNone(attempt['physical_frame_time'])
            self.assertIsNone(attempt['time_mapping'])
            self.assertIsInstance(attempt['time_meaning'], str)
            self.assertTrue(attempt['time_meaning'].strip())
        self.assertEqual(surface['time'], 0.)
        self.assertEqual(surface['native_label_time'], 0.)
        self.assertEqual(surface['label_semantics'], FINAL_STAGE_LABEL_SEMANTICS)
        self.assertIsNone(surface['physical_frame_time'])
        self.assertIsNone(surface['time_mapping'])
        self.assertTrue(surface['time_meaning'].strip())
        for wave in measurements['waveforms']:
            self.assertEqual(wave['samples'][0][0], 0.)
            self.assertEqual(wave['label_semantics'], FINAL_STAGE_LABEL_SEMANTICS)
            self.assertIsNone(wave['time_mapping'])
            self.assertEqual(wave['sample_time_metadata'], [{'native_label_time': 0., 'physical_frame_time': None}])
            self.assertTrue(wave['time_meaning'].strip())
        self.assertIn(fresh['surface'], result['artifacts'])
        self.assertEqual(reader.pin(self.root/'result'/fresh['surface']['path'], self.root/'result'), fresh['surface'])
        self.assertEqual(reader.pin(self.root/'result/measurements.json', self.root/'result'), result['measurements'])
        self.assertEqual(reader.parse(reader.read(self.root/'result/measurements.json', result['measurements']['sha256'])), measurements)

    def test_masked_constraint_rms_uses_squared_con_M_volume_and_explicit_units(self):
        self.produce(); m = reader.parse((self.root/'result/measurements.json').read_bytes())
        for row in m['constraints']:
            self.assertEqual(row['included_cells'], 7); self.assertEqual(row['proper_volume'], .875)
            self.assertEqual(row['H_rms'], 2); self.assertEqual(row['M_rms'], 3)
            self.assertEqual(row['units']['H_rms'], '1/L^2'); self.assertEqual(row['units']['M_rms'], '1/L^2')
            self.assertIsNone(row['accuracy_accepted'])

    def test_diagnostic_slice_pins_actual_channels_and_partial_horizon_scope(self):
        result = self.produce(); out = self.root/'result'
        index = reader.parse(reader.read(out/result['diagnostic_slices']['path'], result['diagnostic_slices']['sha256']))
        self.assertEqual([f['time'] for f in index['frames']], [0., .5126953125])
        self.assertEqual(index['time_unit'], 'L/c'); self.assertIsNone(index['units']['si_mapping'])
        self.assertFalse(index['execution']['complete']); self.assertEqual(index['horizons'][0]['latest_status'], 'failed')
        self.assertEqual(index['physical_boost']['status'], 'unknown')
        frame = index['frames'][0]; data = reader.parse(reader.read(out/frame['data']['path'], frame['data']['sha256']))
        self.assertEqual(data['blocks'][0]['native_logical_level'], 3)
        self.assertEqual(data['blocks'][0]['x'], [.25, .75]); self.assertEqual(data['blocks'][0]['x_edges'], [0., .5, 1.])
        self.assertEqual(data['blocks'][0]['z'], .25)
        self.assertEqual(data['blocks'][0]['channels']['chi'][0][0], .03125)
        self.assertEqual(data['blocks'][0]['channels']['hamiltonian'][0][0], 1000.)
        self.assertEqual(data['reduction_mask']['value'], .0625)  # Display retains even masked-out cells.
        self.assertEqual(data['source_metric']['sha256'], frame['source_metric']['sha256'])
        reader.read_result(out, reader.decoder.sha256(out/'result.json'))
        with (out/frame['data']['path']).open('ab') as handle:
            handle.write(b' ')
        with self.assertRaisesRegex(ValueError, 'pin|mismatch'):
            reader.read_result(out, reader.decoder.sha256(out/'result.json'))

    def test_slice_keeps_amr_blocks_native_centres_and_half_open_face_ownership(self):
        # Adjacent coarse/fine leaf regions plus a negative-z neighbour. Different
        # native z centres must remain different; no common-plane interpolation.
        def block(logical, box):
            axes = [np.linspace(box[2*a]+(box[2*a+1]-box[2*a])/4,
                                box[2*a+1]-(box[2*a+1]-box[2*a])/4, 2) for a in range(3)]
            return {'logical': logical, 'index': (2,3,2,3,2,3), 'geometry': box, 'coordinates': axes,
                    'fields': {'z4c_chi': np.ones((2,2,2)), 'z4c_alpha': np.full((2,2,2), 0.75), 'con_H': np.full((2,2,2), 2.)}}
        blocks = [block((0,0,0,3), (-1,0,-1,1,0,1)), block((2,0,0,4), (0,.5,-1,1,0,.5)),
                  block((0,0,-1,3), (-1,0,-1,1,-1,0))]
        frame = {'time': .25, 'cycle': 4, 'blocks': blocks}
        view = reader.diagnostic_slice(frame, frame, {'sha256':'a'*64}, {'sha256':'b'*64}, .0625)
        self.assertEqual(len(view['blocks']), 2); self.assertEqual(view['cell_count'], 8)
        self.assertEqual([b['native_logical_level'] for b in view['blocks']], [3,4])
        self.assertEqual([b['z'] for b in view['blocks']], [.25,.125])
        self.assertTrue(view['plane']['actual_z_varies_by_block']); self.assertEqual(view['interpolation'], 'none')
        self.assertEqual(view['blocks'][1]['x'], [.125,.375])
        blocks[1]['fields']['z4c_alpha'][0,0,0] = np.nan
        with self.assertRaisesRegex(ValueError, 'Invalid retained slice channel'):
            reader.diagnostic_slice(frame, frame, {}, {}, .0625)

    def test_completed_process_without_target_snapshot_is_still_partial(self):
        self.write_receipts(completed=True); result = self.produce()
        self.assertTrue(result['execution']['process_completed']); self.assertFalse(result['execution']['complete'])
        self.assertEqual(result['execution']['last_paired_native_time'], .5126953125)

    def test_complete_retained_target_does_not_certify_speed_or_merger(self):
        self.header = self.header.replace('tlim = 1', 'tlim = 0.5126953125')
        (self.root/'input.athinput').write_text(self.header); self.make_fields(); self.write_receipts(completed=True)
        result = self.produce(); self.assertTrue(result['execution']['complete'])
        self.assertEqual(result['physical_boost']['status'], 'unknown'); self.assertTrue(all(v is False for v in result['fulfillment'].values()))

    def test_model_calibration_claims_cannot_elevate_unknown_speed(self):
        with self.assertRaisesRegex(ValueError, 'authority'):
            self.produce(calibration={'verified': True, 'gamma': 22.366272, 'speed_over_c': .999})
        self.write_receipts(physical_boost={'verified': True, 'gamma': 22.366272})
        result = self.produce(); self.assertIsNone(result['physical_boost']['gamma']); self.assertEqual(result['physical_boost']['status'], 'unknown')

    def test_exact_source_receipt_input_and_executable_bindings_refuse_corruption(self):
        for name in ('engine.json', 'request.json', 'input.athinput'):
            path = self.root/name; raw = path.read_bytes(); path.write_bytes(raw+b' ')
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'SHA256'):
                self.produce()
            path.write_bytes(raw)
        self.assertFalse((self.root/'result').exists())
        r = reader.parse(self.receipt.read_bytes()); r['source']['athenak'] = '0'*40
        self.receipt.write_text(json.dumps(r)); self.rpin = reader.sha(self.receipt.read_bytes())
        with self.assertRaisesRegex(ValueError, 'identity mismatch'):
            self.produce()

    def test_corrupt_native_pin_and_unregistered_files_fail_without_result(self):
        p = self.run/'bin/fixture.z4c.00000.bin'; p.write_bytes(p.read_bytes()[:-1]+b'x')
        with self.assertRaisesRegex(ValueError, 'SHA256'):
            self.produce()
        self.assertFalse((self.root/'result/result.json').exists())

    def test_interrupted_native_write_stays_retained_but_cannot_invent_last_pair(self):
        p = self.run/'bin/fixture.con.00001.bin'; p.write_bytes(p.read_bytes()[:-7])
        self.write_receipts(); result = self.produce()
        self.assertEqual(result['paired_times'], [0.]); self.assertFalse(result['execution']['complete'])
        self.assertTrue(any('truncated' in row.get('error', '') for row in result['diagnostic_errors']))
        self.assertTrue((self.root/'result/native/bin/fixture.con.00001.bin').exists())

    def test_unregistered_and_escaping_artifacts_refused_before_publication(self):
        extra = self.run/'extra.txt'; extra.write_text('not registered')
        with self.assertRaisesRegex(ValueError, 'Unregistered'):
            self.produce()
        extra.unlink()
        row = reader.parse(self.receipt.read_bytes()); row['files'][0]['path'] = '../escape.bin'
        self.receipt.write_text(json.dumps(row)); self.rpin = reader.sha(self.receipt.read_bytes())
        with self.assertRaisesRegex(ValueError, 'relative path'):
            self.produce()
        self.assertFalse((self.root/'result').exists())

    def test_cuda_v1_is_rejected_even_with_plausible_build_metadata(self):
        self.engine_id = 'athenak_two_punctures_cuda'
        self.build = {'backend': 'CUDA', 'precision': 'double', 'problem': 'z4c/two_punctures/z4c_two_puncture', 'cuda_arch': 'ADA89', 'cuda_version': '12.9.86'}
        self.write_receipts()
        with self.assertRaisesRegex(ValueError, 'v1 receipts cannot admit CUDA'):
            self.produce()

    def v2(self, cuda=False):
        """Synthetic protocol fixture only; no CUDA kernel or cgroup is started."""
        self.engine_id = 'athenak_two_punctures_cuda' if cuda else 'athenak_two_punctures_serial'
        self.build = {'backend': 'CUDA' if cuda else 'Serial', 'precision': 'double', 'problem': 'z4c/two_punctures/z4c_two_puncture'}
        if cuda:
            self.build.update(cuda_arch='ADA89', cuda_version='12.9.86', cuda_runtime={
                'schema':'phaseforge.nr-cuda-runtime.v1', 'library_directory':'/trusted/toolkit/lib', 'driver_directory':'/usr/lib/wsl/lib',
                'guard_source_sha256':'a'*64,'build_receipt':{'path':'/trusted/build.json','sha256':'b'*64},
                'libraries':[{'name':'libcudart.so.12.9.79','bytes':123,'sha256':'c'*64,
                              'aliases':{'libcudart.so.12':'libcudart.so.12.9.79','libcudart.so':'libcudart.so.12'}}]})
        self.write_receipts()
        q=reader.parse((self.root/'request.json').read_bytes()); r=reader.parse(self.receipt.read_bytes())
        policy={'backend':'cuda' if cuda else 'cpu','memory_boundary':'systemd_cgroup_v2','address_space_limit_bytes':None,'tasks_max':128,
                'gpu_vram_policy':{'mode':'not_admitted'}}
        if cuda:
            policy['gpu_vram_policy']={'mode':'device_wide_soft_guard','device_uuid':'GPU-12345678-1234-1234-1234-123456789abc',
                                      'maximum_device_used_growth_bytes':1024**3,'minimum_free_bytes':2*1024**3,'poll_interval_seconds':1}
        q.update(schema='phaseforge.nr-request.v2',execution_policy=policy,output_dir=f'/trusted/jobs/{self.job}/work')
        unit=f'phaseforge-nr-{self.job}.scope'; group='/system.slice/'+unit
        r.update(schema='phaseforge.nr-process-receipt.v2',execution_policy=policy,
                 memory_boundary={'kind':'systemd_cgroup_v2','unit':unit,'path':group,'inode':123,'child_cgroup_before_exec':group,
                                  'supervisor_cgroup':'/init.scope','guardian_cgroup':'/init.scope','cgroup_drained':True,'address_space_limit_bytes':None,
                                  'effective_kernel_limits':{'memory.max':q['memory_limit_bytes'],'memory.swap.max':0,'pids.max':128,'memory.oom.group':1}},
                 gpu_vram={'execution_admitted':cuda,'hard_limit_enforced':False,'attributable_process_usage':None,'kernel_execution_observed':None,'policy':policy['gpu_vram_policy']})
        if cuda:
            runtime=self.build['cuda_runtime']; folder=f'/trusted/jobs/{self.job}/cuda-runtime'
            r['cuda_runtime_snapshot']={'directory':folder,'source_directory':runtime['library_directory'],'driver_directory':runtime['driver_directory'],
                                       'library':runtime['libraries'][0],'build_receipt':runtime['build_receipt'],'ld_library_path':folder+':/usr/lib/wsl/lib'}
            sample={'schema':'phaseforge.nr-gpu-sample.v1','status':'known','device_uuid':policy['gpu_vram_policy']['device_uuid'],
                    'scope':'device_wide_soft_guard','returncode':0,'total_bytes':8*1024**3,'free_bytes':6*1024**3,'used_bytes':2*1024**3,
                    'reserved_bytes':0,'query_duration_seconds':.1,'observed_monotonic':100.}
            guard={'allowed':True,'reason':None,'scope':'device_wide_soft_guard','device_used_growth_bytes':0,'hard_vram_quota':False,'process_attribution':False}
            r['gpu_vram'].update(guard_source_sha256=runtime['guard_source_sha256'],sample_count=2,baseline=sample,before_launch=dict(sample),admission_guard=guard,before_launch_guard=dict(guard))
        return q,r

    def save_v2(self, q, r):
        (self.root/'request.json').write_text(json.dumps(q));r['request_sha256']=reader.sha((self.root/'request.json').read_bytes())
        self.receipt.write_text(json.dumps(r));self.rpin=reader.sha(self.receipt.read_bytes())

    def test_cpu_v2_cgroup_policy_and_owner_binding_then_native_diagnostics(self):
        q,r=self.v2();self.save_v2(q,r);result=self.produce()
        self.assertEqual(result['execution']['memory_boundary'],r['memory_boundary']);self.assertEqual(result['paired_state_count'],2)
        manifest=reader.parse((self.root/'engine.json').read_bytes());inp=(self.root/'input.athinput').read_bytes()
        for key,value in [('unit','init.scope'),('path','/init.scope'),('guardian_cgroup',r['memory_boundary']['path']+'/child'),
                          ('address_space_limit_bytes',1024**3),('cgroup_drained',False),('effective_kernel_limits',{'memory.max':'max'})]:
            bad=json.loads(json.dumps(r));bad['memory_boundary'][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):reader.identity(bad,manifest,q,inp)

    def test_synthetic_cuda_v2_binding_preserves_unknown_kernel_and_boost(self):
        q,r=self.v2(cuda=True);self.save_v2(q,r);result=self.produce()
        self.assertFalse(result['sources']['cpu_cuda_equivalence_tested']);self.assertIsNone(result['execution']['gpu_vram']['kernel_execution_observed'])
        self.assertEqual(result['physical_boost']['status'],'unknown')
        manifest=reader.parse((self.root/'engine.json').read_bytes());inp=(self.root/'input.athinput').read_bytes()
        for section,key,value in [('gpu_vram','execution_admitted',False),('gpu_vram','guard_source_sha256','0'*64),
                                   ('gpu_vram','hard_limit_enforced',True),('gpu_vram','sample_count',1),
                                   ('cuda_runtime_snapshot','directory','/another/job/cuda-runtime'),('cuda_runtime_snapshot','ld_library_path','/usr/local/cuda/lib')]:
            bad=json.loads(json.dumps(r));bad[section][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):reader.identity(bad,manifest,q,inp)
        bad=json.loads(json.dumps(r));bad['gpu_vram']['before_launch']['free_bytes']=1024
        with self.assertRaisesRegex(ValueError,'admission'):reader.identity(bad,manifest,q,inp)

    def test_gpu_pre_gate_failure_can_retain_no_execution_but_cannot_claim_states(self):
        q,r=self.v2(cuda=True);r['gpu_vram']['execution_admitted']=False;r.pop('memory_boundary');r.pop('cuda_runtime_snapshot');r['files']=[]
        manifest=reader.parse((self.root/'engine.json').read_bytes());inp=(self.root/'input.athinput').read_bytes()
        reader.identity(r,manifest,q,inp)
        r['files']=[{'path':'bin/invented.bin'}]
        with self.assertRaisesRegex(ValueError,'unadmitted'):reader.identity(r,manifest,q,inp)

    def test_immutable_result_readback_checks_every_artifact(self):
        result = self.produce(); path = self.root/'result/result.json'; digest = reader.sha(path.read_bytes())
        self.assertEqual(reader.read_result(path.parent, digest), result)
        with self.assertRaisesRegex(ValueError, 'immutable'):
            self.produce()
        saved = self.root/'result/measurements.json'; saved.write_bytes(saved.read_bytes()+b' ')
        with self.assertRaisesRegex(ValueError, 'artifact mismatch'):
            reader.read_result(path.parent, digest)


class DiagnosticTimestampProvenanceTests(unittest.TestCase):
    """Direct text parsers; no native evolution and no inferred timestep."""

    def assert_unmapped_label(self, record, raw_time):
        self.assertEqual(record['time'], raw_time)
        self.assertEqual(record['native_label_time'], raw_time)
        self.assertEqual(record['label_semantics'], FINAL_STAGE_LABEL_SEMANTICS)
        self.assertIsNone(record['physical_frame_time'])
        self.assertIsNone(record['time_mapping'])
        self.assertIsInstance(record['time_meaning'], str)
        self.assertTrue(record['time_meaning'].strip())

    @staticmethod
    def horizon_text():
        # The summary carries more digits than verbose, and the last search was
        # interrupted before its summary write. Label intervals are nonuniform.
        labels = ('0', '0.123456', '0.40001', '0.9375')
        outcomes = ('Found horizon 0\n', 'Failed, reached max iterations 100\n',
                    'Found horizon 0\n', '')
        verbose = ''.join(f'time={float(label):.4f}, cycle=3\nSearching for horizon 0\n'
                          'center = (3.000000, 0.000000, 0.000000)\n'+outcome
                          for label, outcome in zip(labels, outcomes))
        summary = '# columns\n' + ''.join(
            f'3 {label} 0.5 0 0 0 0 {4*math.pi:.15e} 0.04 0 1 1\n' for label in labels[:3])
        return labels, verbose, summary

    def test_horizon_labels_keep_summary_precision_and_interrupted_verbose_time(self):
        labels, verbose, summary = self.horizon_text()
        records = reader.horizon_records(verbose, summary, 0)
        attempts = records['attempts']
        self.assertEqual(len(attempts), 4)
        for attempt, label in zip(attempts, labels):
            self.assert_unmapped_label(attempt, float(label))
        self.assertEqual(attempts[1]['time_rounded'], .1235)
        self.assertEqual(attempts[1]['time'], .123456)
        self.assertEqual(attempts[2]['time_rounded'], .4)
        self.assertEqual(attempts[2]['time'], .40001)
        self.assertEqual(attempts[1]['current_search_status'], 'failed')
        self.assertTrue(attempts[1]['stale_summary'])
        self.assertIsNone(attempts[1]['current_properties'])
        self.assertEqual(attempts[3]['current_search_status'], 'interrupted')
        self.assertIsNone(attempts[3]['reported_summary'])
        self.assertIsNone(attempts[3]['current_properties'])
        self.assertFalse(any(attempt['horizon_validated'] for attempt in attempts))

    def test_only_fresh_surfaces_keep_their_original_unmapped_labels(self):
        _, verbose, summary = self.horizon_text()
        records = reader.horizon_records(verbose, summary, 0)
        grid = f'# Theta Phi Weight\n{math.pi/2:.15e} 0 1\n'
        basis = f'# retained first7columns\n{math.pi/2:.15e} 0 0 0 {1/math.sqrt(4*math.pi):.15e} 0 0\n'
        shapes = ''.join(f'# iter = 3, Time = {label}\n{math.sqrt(4*math.pi):.15e}\n'
                         for label in ('0', '0.40001'))
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            reader.surfaces(records, shapes, grid, basis, output, 0)
            for index, time in ((0, 0.), (2, .40001)):
                attempt = records['attempts'][index]
                pin = attempt['surface']
                surface = reader.parse(reader.read(output/pin['path'], pin['sha256']))
                self.assert_unmapped_label(surface, time)
                self.assertEqual(reader.pin(output/pin['path'], output), pin)
                self.assertFalse(surface['horizon_validated'])
            self.assertIsNone(records['attempts'][1]['surface'])
            self.assertIsNone(records['attempts'][3]['surface'])
            self.assertEqual(len(list((output/'horizons').glob('*.json'))), 2)

    def test_waveform_zero_and_nonuniform_labels_preserve_all_samples_and_columns(self):
        columns = ['time']+[str(l)+str(m) for l in range(2, 9) for m in range(-l, l+1)]
        header = '# '+' '.join(f'{index+1}:{label}' for index, label in enumerate(columns))+'\n'
        for times in ((0.,), (0., .03125, .125, .3125)):
            with self.subTest(times=times):
                texts, expected = {}, {}
                for part, sign in (('real', 1.), ('imag', -1.)):
                    name = f'waveforms/rpsi4_{part}_0008.txt'
                    samples = [[time]+[sign*(row*77+column+1)/1024 for column in range(77)]
                               for row, time in enumerate(times)]
                    texts[name] = header+''.join(' '.join(f'{value:.17g}' for value in row)+'\n' for row in samples)
                    expected[part] = samples
                series = reader.waveform_records(set(texts), texts.__getitem__)
                self.assertEqual(len(series), 2)
                for wave in series:
                    self.assertEqual(wave['columns'], columns)
                    self.assertEqual(wave['samples'], expected[wave['part']])
                    self.assertEqual(wave['label_semantics'], FINAL_STAGE_LABEL_SEMANTICS)
                    self.assertIsNone(wave['time_mapping'])
                    self.assertEqual(wave['sample_time_metadata'], [
                        {'native_label_time': time, 'physical_frame_time': None} for time in times])
                    self.assertIsInstance(wave['time_meaning'], str)
                    self.assertTrue(wave['time_meaning'].strip())
                    self.assertEqual(wave['quantity'], 'rPsi4')


if __name__ == '__main__':
    unittest.main()
