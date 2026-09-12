"""Actual private systemd scope fixtures. CPU only; no scientific engine or GPU.

Each synthetic workload expires within 12 seconds, caps output at 1 MiB, and is
staged below the current user's private home. Only freshly named owned scopes
are touched. Set PHASEFORGE_NR_CGROUP_EVIDENCE to retain test receipts.
"""
import datetime as dt
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import unittest
import uuid
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
if sys.platform.startswith("linux"):
    spec = importlib.util.spec_from_file_location("cpu_fixture", HERE / "test_nr_supervisor.py")
    fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
    nr = fixture.nr

CHILD = r'''#!/usr/bin/python3
import json,mmap,os,resource,signal,sys,time
from pathlib import Path
signal.alarm(12)
request=json.loads(Path(sys.argv[2]).read_text())
def group(): return Path('/proc/self/cgroup').read_text().strip()
if request['mode']=='virtual':
    block=mmap.mmap(-1,16*1024**3)
    block[0:4096]=b'x'*4096
    Path('result.json').write_text(json.dumps({'mapped_bytes':len(block),'touched_bytes':4096,
      'as_limit':resource.getrlimit(resource.RLIMIT_AS),'cgroup':group(),
      'affinity':sorted(os.sched_getaffinity(0)),'cuda_visible':os.environ.get('CUDA_VISIBLE_DEVICES')}))
    time.sleep(.3)
else:
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    child=os.fork()
    if child==0:
        os.setsid();signal.alarm(12)
        Path('descendant.json').write_text(json.dumps({'pid':os.getpid(),'cgroup':group(),'session':os.getsid(0)}))
        time.sleep(12);os._exit(0)
    Path('children.json').write_text(json.dumps({'pids':[os.getpid(),child],'cgroup':group()}))
    if request['mode']=='oom':
        time.sleep(.25)
        blocks=[]
        for _ in range(32):
            blocks.append(bytearray(8*1024**2));time.sleep(.015)
    time.sleep(12)
'''


def protected_observation():
    paths = (Path('/sys/fs/cgroup'), Path('/sys/fs/cgroup/init.scope'), Path('/sys/fs/cgroup/system.slice'))
    return {'self': nr.process_cgroup(os.getpid()), 'pid1': nr.process_cgroup(1),
        'limits': {str(path): {name: (path/name).read_text().strip() for name in
            ('memory.max','memory.swap.max','pids.max','memory.oom.group','cgroup.subtree_control') if (path/name).exists()} for path in paths}}


@unittest.skipUnless(sys.platform.startswith('linux'), 'Requires Linux and real systemd cgroups')
class CgroupTests(unittest.TestCase):
    configure = fixture.SupervisorTests.configure if sys.platform.startswith('linux') else None
    start = fixture.SupervisorTests.start if sys.platform.startswith('linux') else None
    await_file = fixture.SupervisorTests.await_file if sys.platform.startswith('linux') else None
    finish = fixture.SupervisorTests.finish if sys.platform.startswith('linux') else None

    def setUp(self):
        fixture.SupervisorTests.setUp(self)
        self.before = protected_observation()
        self.siblings = []
        self.engine.write_text(CHILD); self.engine.chmod(0o700)
        self.manifest['executable']['sha256'] = fixture.digest(self.engine.read_bytes())
        self.manifest['source'] = {'test_only':True,'sha256':fixture.digest(CHILD.encode()),'scope':'synthetic RAM boundary, not NR physics'}
        self.manifest['build']['backend'] = 'Serial'
        self.request.update(schema='phaseforge.nr-request.v2', engine_manifest_sha256=fixture.digest(fixture.write_json(self.root/'engine.json',self.manifest)),
            memory_limit_bytes=128*1024**2, execution_policy={'backend':'cpu','memory_boundary':'systemd_cgroup_v2',
                'address_space_limit_bytes':None,'tasks_max':128,'gpu_vram_policy':{'mode':'not_admitted'}})
        self.configure('virtual',deadline_at=(dt.datetime.now(dt.timezone.utc)+dt.timedelta(seconds=10)).isoformat())

    def tearDown(self):
        try:
            self.assertEqual(protected_observation(), self.before, 'Existing parent/root/init scopes must remain unchanged')
            evidence = os.environ.get('PHASEFORGE_NR_CGROUP_EVIDENCE')
            if evidence:
                dest=Path(evidence)/self.id().rsplit('.',1)[-1]
                dest.mkdir(parents=True,exist_ok=False,mode=0o700)
                for name in ('engine.json',): shutil.copyfile(self.root/name,dest/name)
                for name in ('request.json','input.athinput','launch.json','receipt.json'):
                    if (self.job/name).exists():shutil.copyfile(self.job/name,dest/name)
                if (self.job/'work').exists():shutil.copytree(self.job/'work',dest/'work')
                fixture.write_json(dest/'protected-scopes.json',{'before':self.before,'after':protected_observation()})
        finally:
            for sibling in self.siblings:
                if sibling.poll() is None:sibling.terminate()
                sibling.wait(timeout=3)
            fixture.SupervisorTests.tearDown(self)

    def boundary(self, receipt):
        self.assertEqual(receipt['schema'],'phaseforge.nr-process-receipt.v2')
        boundary=receipt['memory_boundary'];group='/system.slice/phaseforge-nr-'+self.job.name+'.scope'
        self.assertEqual(boundary['path'],group)
        self.assertEqual(boundary['effective_kernel_limits'],{'memory.max':128*1024**2,'memory.swap.max':0,'pids.max':128,'memory.oom.group':1})
        self.assertEqual(boundary['child_cgroup_before_exec'],group)
        self.assertNotEqual(boundary['supervisor_cgroup'],group);self.assertNotEqual(boundary['guardian_cgroup'],group)
        self.assertIsNone(boundary['address_space_limit_bytes'])
        self.assertTrue(boundary['cgroup_drained'])
        self.assertFalse(receipt['gpu_vram']['execution_admitted']);self.assertFalse(receipt['gpu_vram']['hard_limit_enforced'])
        return boundary

    def sibling(self):
        sibling=subprocess.Popen([sys.executable,'-I','-B','-c','import time;time.sleep(12)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        self.siblings.append(sibling)
        self.assertEqual(nr.process_cgroup(sibling.pid),self.before['self'])
        return sibling

    def test_01_large_virtual_mapping_small_ram_and_exact_kernel_limits(self):
        self.start();receipt=self.finish('completed');boundary=self.boundary(receipt)
        result=json.loads((self.job/'work/result.json').read_text())
        self.assertEqual(result['mapped_bytes'],16*1024**3)
        self.assertEqual(result['as_limit'],[-1,-1]);self.assertEqual(result['touched_bytes'],4096)
        self.assertEqual(result['affinity'],receipt['affinity_cpus']);self.assertEqual(result['cuda_visible'],'')
        self.assertEqual(result['cgroup'],'0::'+boundary['path'])
        self.assertLess(boundary['latest_sample']['last_observed_values']['memory.peak'],128*1024**2)

    def test_02_one_128mib_kernel_oom_kills_entire_owned_scope(self):
        self.configure('oom');self.start();children=self.await_file('work/children.json');descendant=self.await_file('work/descendant.json')
        self.assertEqual(descendant['cgroup'],children['cgroup']);self.assertEqual(descendant['session'],descendant['pid'])
        receipt=self.finish('cgroup_oom');boundary=self.boundary(receipt)
        values=boundary['before_cleanup']['last_observed_values']
        self.assertTrue(boundary['systemd_result']=='oom-kill' or values.get('memory.events',{}).get('oom_kill',0)>0,
            'Require the exact owned systemd unit OOM result or an actual kernel OOM counter')
        # Empty scopes can disappear before the final sample. Missing final
        # counters remain unavailable; retained older zeros are not final zeros.
        self.assertTrue(all(not fixture.living(pid) for pid in children['pids']))
        self.assertLess(receipt['elapsed_seconds'],10)

    def test_03_eof_drains_escaped_descendant_and_preserves_sibling(self):
        sibling=self.sibling();self.configure('sleep');self.start()
        children=self.await_file('work/children.json');self.await_file('work/descendant.json')
        self.process.stdin.close();receipt=self.finish('parent_lost');self.boundary(receipt)
        self.assertTrue(all(not fixture.living(pid) for pid in children['pids']))
        self.assertIsNone(sibling.poll())

    def test_04_sigkill_guardian_drains_scope_and_preserves_sibling(self):
        sibling=self.sibling();self.configure('sleep');self.start()
        children=self.await_file('work/children.json');self.await_file('work/descendant.json');launch=self.await_file('launch.json')
        self.process.kill();self.process.wait(timeout=3)
        pids=children['pids']+[launch['guardian_pid']];until=time.monotonic()+4
        while any(fixture.living(pid) for pid in pids) and time.monotonic()<until:time.sleep(.02)
        self.assertTrue(all(not fixture.living(pid) for pid in pids))
        path=Path('/sys/fs/cgroup')/launch['memory_boundary']['path'].lstrip('/')
        self.assertTrue(not path.exists() or 'populated 0' in (path/'cgroup.events').read_text())
        self.assertFalse((self.job/'receipt.json').exists(),'SIGKILL must not fabricate a successful receipt')
        self.assertIsNone(sibling.poll())

    def test_05_policy_and_missing_systemd_fail_closed_before_engine_exec(self):
        for policy in ({**self.request['execution_policy'],'backend':'cuda'},
                       {**self.request['execution_policy'],'gpu_vram_policy':{'mode':'unlimited'}},
                       {**self.request['execution_policy'],'address_space_limit_bytes':128*1024**2}):
            with self.subTest(policy=policy),self.assertRaises(ValueError):
                nr.execution_policy({**self.request,'execution_policy':policy},self.manifest)
        with self.assertRaises(ValueError):nr.CgroupScope.scope_name('not-an-owned-uuid')
        admitted=nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        os.close(admitted['engine_fd']);os.close(admitted['input_fd'])
        with patch.object(nr.CgroupScope,'bus',side_effect=OSError('synthetic bus unavailable')):
            child=subprocess.Popen([sys.executable,'-I','-B','-c','import time;time.sleep(12)'],start_new_session=True)
            try:
                with self.assertRaisesRegex(OSError,'unavailable'):
                    nr.CgroupScope.create(self.job.name,child.pid,128*1024**2,128)
            finally:child.terminate();child.wait(timeout=3)
        self.assertFalse((self.job/'launch.json').exists())

    def test_06_existing_private_unit_is_rejected_without_altering_it(self):
        self.configure('sleep');self.start();self.await_file('work/children.json');launch=self.await_file('launch.json')
        path=Path('/sys/fs/cgroup')/launch['memory_boundary']['path'].lstrip('/')
        before={name:(path/name).read_text() for name in ('memory.max','memory.swap.max','pids.max','cgroup.procs')}
        child=subprocess.Popen([sys.executable,'-I','-B','-c','import time;time.sleep(12)'],start_new_session=True)
        try:
            with self.assertRaisesRegex(ValueError,'must not be reused'):
                nr.CgroupScope.create(self.job.name,child.pid,128*1024**2,128)
            self.assertEqual(before,{name:(path/name).read_text() for name in before})
            self.assertTrue(fixture.living(launch['engine_pid']))
        finally:child.terminate();child.wait(timeout=3)
        self.process.stdin.write('{"action":"cancel"}\n');self.process.stdin.flush();self.finish('cancel')


@unittest.skipUnless(sys.platform.startswith('linux'), 'Requires Linux private runtime fixtures')
class CudaContractTests(unittest.TestCase):
    """Synthetic library data and pre-exec failures; never run a CUDA binary."""
    configure = CgroupTests.configure
    await_file = CgroupTests.await_file
    finish = CgroupTests.finish
    tearDown = CgroupTests.tearDown

    def setUp(self):
        CgroupTests.setUp(self)
        tools=self.root/'tools';tools.mkdir(mode=0o700)
        for name in ('nr_supervisor.py','nr_gpu.py'):
            shutil.copyfile(HERE/name,tools/name);(tools/name).chmod(0o600)
        spec=importlib.util.spec_from_file_location('private_cuda_contract_supervisor',tools/'nr_supervisor.py')
        self.nr=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.nr)
        self.library=self.root/'toolkit/lib';self.library.mkdir(parents=True,mode=0o700)
        raw=b'Synthetic snapshot bytes. Not a CUDA library and never loaded.\n'
        (self.library/'libcudart.so.12.9.79').write_bytes(raw)
        os.symlink('libcudart.so.12.9.79',self.library/'libcudart.so.12')
        os.symlink('libcudart.so.12',self.library/'libcudart.so')
        self.row={'name':'libcudart.so.12.9.79','sha256':fixture.digest(raw),'bytes':len(raw),
            'aliases':{'libcudart.so.12':'libcudart.so.12.9.79','libcudart.so':'libcudart.so.12'}}
        self.manifest['source']['scope']='Synthetic pre-exec admission fixture; no CUDA binary or kernel executed'
        build={'backend':'CUDA','precision':'double','cuda_arch':'ADA89','cuda_version':'12.9.86'}
        self.build_receipt={'schema':'phaseforge.nr-cuda-build.v1','built':True,'engine':self.manifest['executable'],
            'source':self.manifest['source'],'build':build,'runtime_libraries':[{key:self.row[key] for key in ('name','sha256','bytes')}],
            'synthetic_fixture_only':True}
        pin=fixture.digest(fixture.write_json(self.root/'synthetic-build-receipt.json',self.build_receipt))
        self.manifest['build']={**build,'cuda_runtime':{'schema':'phaseforge.nr-cuda-runtime.v1','library_directory':str(self.library),
            'driver_directory':'/usr/lib/wsl/lib','libraries':[self.row],
            'build_receipt':{'path':str(self.root/'synthetic-build-receipt.json'),'sha256':pin},
            'guard_source_sha256':fixture.digest((tools/'nr_gpu.py').read_bytes())}}
        self.policy={'mode':'device_wide_soft_guard','device_uuid':'GPU-00000000-0000-0000-0000-000000000001',
            'maximum_device_used_growth_bytes':1024**3,'minimum_free_bytes':2*1024**3,'poll_interval_seconds':1}
        self.request['execution_policy'].update(backend='cuda',gpu_vram_policy=self.policy)
        self.repin()

    def repin(self):
        self.request['engine_manifest_sha256']=fixture.digest(fixture.write_json(self.root/'engine.json',self.manifest))
        fixture.write_json(self.job/'request.json',self.request)

    def admitted(self):
        return self.nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)

    def close_admitted(self,value):
        for key in ('engine_fd','input_fd'):os.close(value[key])
        if value.get('cuda_runtime'):os.close(value['cuda_runtime']['library_fd'])

    def test_pinned_private_snapshot_survives_source_change_and_uses_exact_uuid(self):
        admitted=self.admitted()
        try:
            before=(self.library/self.row['name']).read_bytes()
            (self.library/self.row['name']).write_bytes(b'changed source after immutable admission')
            snapshot=self.nr.cuda_snapshot(admitted);admitted['cuda_snapshot']=snapshot
            root=Path(snapshot['directory'])
            self.assertEqual((root/self.row['name']).read_bytes(),before)
            self.assertEqual((root/'libcudart.so').read_bytes(),before)
            self.assertEqual(os.readlink(root/'libcudart.so'),'libcudart.so.12')
            with patch.dict(os.environ,{'LD_LIBRARY_PATH':'/untrusted/stubs','LD_PRELOAD':'evil.so','CUDA_VISIBLE_DEVICES':'0'}):
                env=self.nr.engine_environment(admitted)
            self.assertEqual(env['CUDA_VISIBLE_DEVICES'],self.policy['device_uuid'])
            self.assertEqual(env['LD_LIBRARY_PATH'],str(root)+':/usr/lib/wsl/lib')
            self.assertNotIn(str(self.library),env['LD_LIBRARY_PATH']);self.assertNotIn('LD_PRELOAD',env)
            self.assertNotIn('stubs',env['LD_LIBRARY_PATH'])
            self.assertEqual(snapshot['library']['sha256'],fixture.digest(before))
            self.assertFalse((self.job/'launch.json').exists())
        finally:self.close_admitted(admitted)

    def test_helper_pin_is_checked_before_its_python_code_can_run(self):
        marker=self.root/'must-not-exist'
        script=self.root/'tools/nr_gpu.py'
        script.write_text('from pathlib import Path\nPath('+repr(str(marker))+').write_text("bad")\n')
        with self.assertRaisesRegex(ValueError,'SHA-256'):self.admitted()
        self.assertFalse(marker.exists());self.assertFalse((self.job/'launch.json').exists())

    def test_runtime_alias_unknown_library_build_identity_and_cpu_cuda_policy_fail_closed(self):
        with patch.object(self.nr.resource,'getrlimit',return_value=(128*1024**2,128*1024**2)),self.assertRaisesRegex(ValueError,'inherited address space'):
            self.nr.execution_policy(self.request,self.manifest)
        alias=self.library/'libcudart.so.12';alias.unlink();os.symlink('/outside/libcudart.so',alias)
        with self.assertRaisesRegex(ValueError,'same-directory'):self.admitted()
        alias.unlink();os.symlink(self.row['name'],alias)
        extra=self.library/'libcuda.so.1';extra.write_bytes(b'unregistered driver shadow')
        with self.assertRaisesRegex(ValueError,'Unregistered shared'):self.admitted()
        extra.unlink()
        self.build_receipt['engine']={**self.manifest['executable'],'sha256':'0'*64}
        raw=fixture.write_json(self.root/'synthetic-build-receipt.json',self.build_receipt)
        self.manifest['build']['cuda_runtime']['build_receipt']['sha256']=fixture.digest(raw);self.repin()
        with self.assertRaisesRegex(ValueError,'another executable'):self.admitted()
        request={key:value for key,value in self.request.items() if key!='execution_policy'};request['schema']='phaseforge.nr-request.v1'
        with self.assertRaisesRegex(ValueError,'CUDA requires'):self.nr.execution_policy(request,self.manifest)
        self.assertFalse((self.job/'launch.json').exists())

    def test_positive_soft_guard_is_only_a_proposal_and_unknown_stale_or_unfunded_blocks(self):
        module=self.nr.gpu_module(self.manifest['build']['cuda_runtime']['guard_source_sha256'])
        def sample(**changed):
            return {'status':'known','device_uuid':self.policy['device_uuid'],'observed_monotonic':time.monotonic(),
                'query_duration_seconds':0.01,'total_bytes':16*1024**3,'reserved_bytes':0,'used_bytes':2*1024**3,'free_bytes':14*1024**3,**changed}
        for observation,allowed in [(sample(),True),(sample(status='unknown',free_bytes=None),False),
            (sample(observed_monotonic=time.monotonic()-3),False),(sample(free_bytes=2*1024**3+1),False)]:
            with patch.object(module,'query',return_value=observation):
                _snapshot,decision=self.nr.observe_gpu(self.policy)
            self.assertEqual(decision['allowed'],allowed)
            self.assertFalse(decision['hard_vram_quota']);self.assertFalse(decision['process_attribution'])
        proposed={**self.policy,'maximum_device_used_growth_bytes':4*1024**3}
        for free,allowed in [(6*1024**3-1,False),(6*1024**3,True)]:
            with patch.object(module,'query',return_value=sample(free_bytes=free)):
                _snapshot,decision=self.nr.observe_gpu(proposed)
            self.assertEqual(decision['allowed'],allowed,'4 GiB proposed growth requires the full 2 GiB free reserve as well')
        self.assertFalse((self.job/'launch.json').exists())

    def start_query_fixture(self, before_scope_unknown, delayed_fsync=False):
        # This separate test driver replaces telemetry with labelled synthetic
        # observations. Its second (or first) query always rejects before exec.
        driver=self.root/'synthetic-query-driver.py'
        source='''import importlib.util,json,sys,time
from pathlib import Path
p=Path(__file__).parent
spec=importlib.util.spec_from_file_location('fixture_supervisor',p/'tools/nr_supervisor.py')
nr=importlib.util.module_from_spec(spec);spec.loader.exec_module(nr)
manifest=json.loads((p/'engine.json').read_text())
gpu=nr.gpu_module(manifest['build']['cuda_runtime']['guard_source_sha256'])
calls=0
def synthetic_query(uuid):
    global calls
    calls+=1
    known=(calls==1 and not BEFORE_SCOPE_UNKNOWN) or DELAYED_FSYNC
    return {'status':'known' if known else 'unknown','device_uuid':uuid,'observed_monotonic':time.monotonic(),
      'observed_at':'synthetic query fixture','query_duration_seconds':0.01,'total_bytes':16*1024**3,
      'reserved_bytes':0,'used_bytes':2*1024**3,'free_bytes':14*1024**3 if known else None,'synthetic_fixture':True}
gpu.query=synthetic_query
original_atomic=nr.atomic_json
def delayed_atomic(path,value):
    original_atomic(path,value)
    if DELAYED_FSYNC and path.name=='launch.json' and value.get('gpu_vram',{}).get('execution_admitted'):
        time.sleep(2.1)
nr.atomic_json=delayed_atomic
raise SystemExit(nr.main())
'''.replace('BEFORE_SCOPE_UNKNOWN',repr(before_scope_unknown)).replace('DELAYED_FSYNC',repr(delayed_fsync))
        driver.write_text(source);driver.chmod(0o600)
        self.process=subprocess.Popen([sys.executable,'-I','-B',str(driver),'--engine-manifest',str(self.root/'engine.json'),
            '--request',str(self.job/'request.json'),'--job-root',str(self.jobs)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)

    def test_failed_fresh_query_after_ram_assignment_keeps_gate_closed_and_drains_scope(self):
        self.start_query_fixture(False)
        receipt=self.finish('gpu_admission:telemetry_unknown')
        self.assertTrue(receipt['memory_boundary']['cgroup_drained'])
        self.assertFalse(receipt['gpu_vram']['execution_admitted'])
        self.assertIsNone(receipt['gpu_vram']['kernel_execution_observed'])
        self.assertEqual(receipt['gpu_vram']['sample_count'],2)
        self.assertEqual(receipt['gpu_vram']['guard_source_sha256'],self.manifest['build']['cuda_runtime']['guard_source_sha256'])
        self.assertFalse((self.job/'work/result.json').exists(),'The synthetic child must not cross the engine exec gate')
        self.assertEqual((self.job/'work/solver.stdout.log').stat().st_size,0)

    def test_failed_initial_query_never_forks_workload(self):
        self.start_query_fixture(True)
        receipt=self.finish('gpu_admission:telemetry_unknown')
        self.assertNotIn('engine_pid',receipt);self.assertNotIn('memory_boundary',receipt)
        self.assertFalse(receipt['gpu_vram']['execution_admitted'])
        self.assertFalse((self.job/'work/result.json').exists())

    def test_slow_durable_launch_write_cannot_release_stale_gpu_observation(self):
        self.start_query_fixture(False,delayed_fsync=True)
        receipt=self.finish('gpu_admission:telemetry_stale')
        self.assertFalse(receipt['gpu_vram']['execution_admitted'])
        self.assertFalse(receipt['gpu_vram']['before_launch_guard']['allowed'])
        self.assertEqual(receipt['gpu_vram']['sample_count'],2,'No silent new observation or budget was substituted')
        self.assertTrue(receipt['memory_boundary']['cgroup_drained'])
        self.assertFalse(self.await_file('launch.json')['gpu_vram']['execution_admitted'])
        self.assertFalse((self.job/'work/result.json').exists())
        self.assertLess(receipt['elapsed_seconds'],5)


if __name__=='__main__':unittest.main(verbosity=2)
