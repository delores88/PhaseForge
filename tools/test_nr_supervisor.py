"""Linux lifecycle fixtures only. No scientific engine download or new study."""
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

SUPERVISOR = Path(__file__).with_name("nr_supervisor.py").resolve()
if sys.platform.startswith("linux"):
    spec = importlib.util.spec_from_file_location("nr_supervisor", SUPERVISOR)
    nr = importlib.util.module_from_spec(spec); spec.loader.exec_module(nr)

# This explicitly synthetic pinned executable is not an application engine.
CHILD = r'''#!/usr/bin/python3
import hashlib,json,os,resource,signal,sys,time
from pathlib import Path
assert len(sys.argv)==3 and sys.argv[1]=='-i'
raw=Path(sys.argv[2]).read_bytes(); request=json.loads(raw)
Path('parameters-reopened.json').write_text(json.dumps({'sha256':hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest()}))
if request['mode']=='success':
    Path('result.json').write_text(json.dumps({'affinity':sorted(os.sched_getaffinity(0)),'as_limit':resource.getrlimit(resource.RLIMIT_AS),
        'secret_present':'PHASEFORGE_TEST_SECRET' in os.environ,'threads':os.environ['OMP_NUM_THREADS'],'input_sha256':hashlib.sha256(raw).hexdigest()}))
elif request['mode']=='as_limit':
    try: block=bytearray(512*1024*1024); blocked=False
    except MemoryError: blocked=True
    Path('result.json').write_text(json.dumps({'allocation_blocked':blocked}))
elif request['mode']=='output':
    for index in range(4): Path('output-%s.dat'%index).write_bytes(b'x'*40000)
    time.sleep(30)
else:
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    child=os.fork()
    if child==0:
        if request['mode']=='rss': block=bytearray(55*1024*1024)
        while True: time.sleep(1)
    if request['mode']=='rss': block=bytearray(55*1024*1024)
    Path('children.json').write_text(json.dumps({'pids':[os.getpid(),child],'group':os.getpgrp()}))
    while True: time.sleep(1)
'''


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def write_json(path, value):
    raw = json.dumps(value, separators=(",", ":")).encode()
    path.write_bytes(raw); path.chmod(0o600)
    return raw


def living(pid):
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
        return text[text.rfind(")") + 2:].split()[0] not in ("Z", "X")
    except FileNotFoundError:
        return False


@unittest.skipUnless(sys.platform.startswith("linux"), "Requires the actual Linux process boundary")
class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix=".phaseforge-nr-supervisor-test-", dir=Path.home())
        self.root = Path(self.temp.name); self.root.chmod(0o700)
        self.jobs = self.root / "jobs"; self.jobs.mkdir(mode=0o700)
        self.job = self.jobs / str(uuid.uuid4()); self.job.mkdir(mode=0o700)
        self.engine = self.root / "synthetic-engine"; self.engine.write_text(CHILD); self.engine.chmod(0o700)
        self.manifest = {"schema":"phaseforge.nr-engine.v1","engine_id":"synthetic_lifecycle_fixture","executable":{"path":str(self.engine),"sha256":digest(self.engine.read_bytes())},
                         "source":{"test_only":True,"sha256":digest(CHILD.encode())},"build":{"test_only":True,"kind":"Python lifecycle fixture"}}
        manifest_raw=write_json(self.root / "engine.json", self.manifest)
        self.request={"schema":"phaseforge.nr-request.v1","job_id":self.job.name,"engine_id":self.manifest['engine_id'],"engine_manifest_sha256":digest(manifest_raw),
                      "input":{"path":"input.athinput","sha256":""},"output_dir":str(self.job / "work"),"cpu_threads":1,"memory_limit_bytes":96*1024**2,
                      "output_limit_bytes":1024**2,"deadline_at":None}
        self.process = None
        self.configure('success')

    def tearDown(self):
        if self.process is not None:
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait(timeout=5)
            for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
                if stream is not None: stream.close()
        launch=self.job / "launch.json"
        if launch.exists():
            receipt=json.loads(launch.read_text())
            group=receipt.get('process_group_id')
            if group:
                try: os.killpg(group,signal.SIGKILL)
                except ProcessLookupError: pass
                until=time.monotonic()+4
                while nr.group_rows(group) and time.monotonic()<until: time.sleep(.02)
        self.temp.cleanup()

    def configure(self, mode, **limits):
        raw=write_json(self.job / 'input.athinput',{'mode':mode})
        self.request['input']['sha256']=digest(raw);self.request.update(limits)
        write_json(self.job / 'request.json',self.request)

    def start(self):
        env={**os.environ,'PHASEFORGE_TEST_SECRET':'fixture-value-must-not-reach-child'}
        self.process=subprocess.Popen([sys.executable,'-I','-B',str(SUPERVISOR),'--engine-manifest',str(self.root/'engine.json'),
                                       '--request',str(self.job/'request.json'),'--job-root',str(self.jobs)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)

    def await_file(self, name):
        path=self.job/name;until=time.monotonic()+5
        while time.monotonic()<until:
            if path.exists():
                try:return json.loads(path.read_text())
                except json.JSONDecodeError: pass
            if self.process.poll() is not None:break
            time.sleep(.02)
        self.fail(f"Missing retained {name}; exit={self.process.poll()}")

    def finish(self, expected):
        code=self.process.wait(timeout=8)
        stdout=self.process.stdout.read();stderr=self.process.stderr.read()
        receipt=json.loads((self.job/'receipt.json').read_text())
        self.assertEqual(receipt['termination_reason'],expected,(receipt,stdout,stderr))
        self.assertEqual(code,0 if expected=='completed' else 3,stderr)
        self.assertTrue(receipt['process_group_drained'])
        self.assertTrue(all(not living(pid) for pid in receipt['observed_pids']))
        self.assertEqual(receipt['deadline_at'],self.request['deadline_at'])
        self.assertFalse(receipt['checkpoint_resume_supported'])
        for row in receipt['files']:
            raw=(self.job/'work'/row['path']).read_bytes()
            self.assertEqual(len(raw),row['bytes']);self.assertEqual(digest(raw),row['sha256'])
        self.assertIn('terminal',[json.loads(line)['event'] for line in stdout.splitlines()])
        return receipt

    def test_success_pins_reopenable_input_affinity_environment_and_all_artifacts(self):
        self.start();receipt=self.finish('completed')
        result=json.loads((self.job/'work/result.json').read_text())
        self.assertEqual(len(result['affinity']),1);self.assertEqual(result['affinity'],receipt['affinity_cpus'])
        self.assertEqual(result['as_limit'],[self.request['memory_limit_bytes']]*2)
        self.assertFalse(result['secret_present']);self.assertEqual(result['threads'],'1')
        self.assertEqual(result['input_sha256'],self.request['input']['sha256'])
        self.assertEqual(json.loads((self.job/'work/parameters-reopened.json').read_text())['sha256'],result['input_sha256'])
        capacity=receipt['execution_resources']
        self.assertGreater(capacity['mem_total_bytes'],0);self.assertGreater(capacity['job_root_free_bytes'],0)
        self.assertEqual(capacity['memory_admission_ceiling_bytes'],max(0,capacity['mem_available_bytes']-512*1024**2))
        self.assertEqual(capacity['output_admission_ceiling_bytes'],max(0,capacity['job_root_free_bytes']-512*1024**2))

    def test_pause_stops_a_sigterm_ignoring_descendant_without_checkpoint_claim(self):
        self.configure('sleep');self.start();children=self.await_file('work/children.json')
        self.process.stdin.write('{"action":"pause"}\n');self.process.stdin.flush()
        self.finish('pause');self.assertTrue(all(not living(pid) for pid in children['pids']))

    def test_cancel_and_stdin_eof_stop_the_owned_group(self):
        self.configure('sleep');self.start();children=self.await_file('work/children.json')
        self.process.stdin.write('{"action":"cancel"}\n');self.process.stdin.flush()
        self.finish('cancel');self.assertTrue(all(not living(pid) for pid in children['pids']))

    def test_transport_eof_stops_off_job(self):
        self.configure('sleep');self.start();children=self.await_file('work/children.json')
        self.process.stdin.close()
        self.finish('parent_lost');self.assertTrue(all(not living(pid) for pid in children['pids']))

    def test_supervisor_sigkill_guardian_stops_engine_and_descendant(self):
        self.configure('sleep');self.start();children=self.await_file('work/children.json');launch=self.await_file('launch.json')
        self.process.kill();self.process.wait(timeout=5)
        until=time.monotonic()+5
        while any(living(pid) for pid in children['pids']+[launch['guardian_pid']]) and time.monotonic()<until:time.sleep(.02)
        self.assertTrue(all(not living(pid) for pid in children['pids']+[launch['guardian_pid']]))
        self.assertFalse((self.job/'receipt.json').exists(),'SIGKILL must not fabricate a clean terminal receipt')

    def test_original_absolute_deadline_stops_off_to_numeric_attempt_without_extension(self):
        stop=(dt.datetime.now(dt.timezone.utc)+dt.timedelta(seconds=1.5)).isoformat()
        self.configure('sleep',deadline_at=stop);self.start();self.await_file('work/children.json')
        receipt=self.finish('deadline');self.assertLess(receipt['elapsed_seconds'],5)

    def test_per_process_address_space_hard_limit(self):
        self.configure('as_limit');self.start();self.finish('completed')
        self.assertTrue(json.loads((self.job/'work/result.json').read_text())['allocation_blocked'])

    def test_aggregate_rss_soft_limit_counts_child_processes(self):
        self.configure('rss');self.start();receipt=self.finish('aggregate_rss_limit')
        self.assertGreater(receipt['peak_sampled_group_rss_bytes'],self.request['memory_limit_bytes'])
        self.assertGreaterEqual(len(receipt['observed_pids']),2)

    def test_aggregate_output_limit_has_explicit_sampling_overshoot(self):
        self.configure('output',output_limit_bytes=65536);self.start();receipt=self.finish('aggregate_output_limit')
        self.assertGreater(receipt['retained_bytes'],65536)
        self.assertTrue(all(row['bytes']<=65536 for row in receipt['files']))

    def test_engine_and_input_memfds_are_sealed_against_post_admission_mutation(self):
        admitted=nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        try:
            before=os.read(admitted['input_fd'],1024);self.configure('sleep')
            os.lseek(admitted['input_fd'],0,0);self.assertEqual(os.read(admitted['input_fd'],1024),before)
            for fd in (admitted['input_fd'],admitted['engine_fd']):
                with self.assertRaises(OSError):os.write(fd,b'changed')
        finally:
            os.close(admitted['input_fd']);os.close(admitted['engine_fd'])

    def test_wrong_hash_extra_argv_links_and_expired_deadline_are_rejected_before_launch(self):
        cases=[{'engine_manifest_sha256':'0'*64},{'argv':['-h']},{'output_dir':str(self.root)},
               {'deadline_at':'2020-01-01T00:00:00Z'}]
        original=dict(self.request)
        for change in cases:
            with self.subTest(change=list(change)):
                write_json(self.job/'request.json',{**original,**change})
                with self.assertRaises(ValueError):nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        write_json(self.job/'request.json',original)
        input_file=self.job/'input.athinput';original_bytes=input_file.read_bytes();input_file.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'SHA-256'):nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        input_file.write_bytes(original_bytes);input_file.rename(self.root/'other-input')
        input_file.symlink_to(self.root/'other-input')
        with self.assertRaisesRegex(ValueError,'Linked'):nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        input_file.unlink();os.link(self.root/'other-input',input_file)
        with self.assertRaisesRegex(ValueError,'Hard-linked'):nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        self.assertFalse((self.job/'launch.json').exists())

    def test_fresh_execution_capacity_rejects_overbudget_requests_without_clamping(self):
        from unittest.mock import patch
        capacity=nr.execution_resources(self.jobs)
        capacity['memory_admission_ceiling_bytes']=64*1024**2
        with patch.object(nr,'execution_resources',return_value=capacity),self.assertRaisesRegex(ValueError,'fresh Linux'):
            nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        capacity['memory_admission_ceiling_bytes']=512*1024**2
        capacity['output_admission_ceiling_bytes']=65536
        with patch.object(nr,'execution_resources',return_value=capacity),self.assertRaisesRegex(ValueError,'fresh job-root'):
            nr.admit(self.root/'engine.json',self.job/'request.json',self.jobs)
        self.assertEqual(json.loads((self.job/'request.json').read_text()),self.request)
        self.assertFalse((self.job/'supervisor.lock').exists())


if __name__=='__main__':
    unittest.main()
