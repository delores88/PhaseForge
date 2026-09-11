"""Real local-backend smoke test. Requires a successfully compiled PhaseForge binary.
No AI calls, credentials, or internet. Uses an isolated temporary DB and port 17433.
Run: python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend[.exe]
"""
import argparse, json, os, pathlib, signal, subprocess, tempfile, time, urllib.request
from contextlib import contextmanager
from test_proposal_schema import draft

def wait_for_windows_tree_exit(process, stopped):
    """Judge taskkill's result only after waiting on our owned process handle.

    taskkill may return nonzero when a helper exits during tree enumeration,
    while the backend itself is still completing termination.
    """
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired as error:
        if stopped.returncode:
            raise RuntimeError('Backend process-tree shutdown failed: '
                               + stopped.stdout + stopped.stderr) from error
        raise

def wait_for_windows_log_release(log_path, timeout=10):
    """Wait for inherited Windows log handles, without hiding cleanup failures."""
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                       wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD,
                       wintypes.HANDLE]
    create.restype = wintypes.HANDLE
    close = kernel.CloseHandle
    close.argtypes = [wintypes.HANDLE]
    close.restype = wintypes.BOOL
    deadline = time.monotonic() + timeout
    while True:
        # OPEN_EXISTING + no sharing probes for surviving handles without
        # modifying/deleting the log or granting children delete sharing.
        handle = create(str(log_path.resolve()), 0x80000000, 0, None, 3, 0x80, None)
        if handle != ctypes.c_void_p(-1).value:
            if not close(handle):
                raise ctypes.WinError(ctypes.get_last_error())
            return
        error = ctypes.get_last_error()
        if error not in (32, 33):  # sharing violation / lock violation only
            raise ctypes.WinError(error)
        if time.monotonic() >= deadline:
            raise RuntimeError(f'Backend log remains locked after {timeout}s: {log_path}')
        time.sleep(.05)

@contextmanager
def backend_log(log_path):
    log = log_path.open('w', encoding='utf-8')
    try:
        yield log
    finally:
        log.close()
        if os.name == 'nt':
            # taskkill waits on the backend, not every inherited file handle.
            # Verify the log is released before TemporaryDirectory removes it.
            wait_for_windows_log_release(log_path)

@contextmanager
def backend_process(command, log_path, *, environment=None):
    """Own the backend and its helpers until their inherited log handles close.

    Telemetry and verification can launch child processes. Terminating only the
    backend leaves those children holding backend.log open on Windows, making
    TemporaryDirectory cleanup fail even after every API assertion passes.
    """
    options = ({'creationflags': subprocess.CREATE_NO_WINDOW} if os.name == 'nt'
               else {'start_new_session': True})
    with backend_log(log_path) as log:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log,
                                   stderr=subprocess.STDOUT, env=environment,
                                   close_fds=True, **options)
        try:
            yield process
        finally:
            try:
                if os.name == 'nt':
                    if process.poll() is None:
                        # Kill the tree while its parent still exists; killing
                        # the parent first loses taskkill's child ownership.
                        stopped = subprocess.run(
                            ['taskkill', '/PID', str(process.pid), '/T', '/F'],
                            stdin=subprocess.DEVNULL, capture_output=True,
                            text=True, timeout=15,
                            creationflags=subprocess.CREATE_NO_WINDOW)
                        wait_for_windows_tree_exit(process, stopped)
                else:
                    # The dedicated session contains this test's backend only,
                    # including helpers that outlive their immediate parent.
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                process.wait(timeout=10)
            finally:
                # A failed tree shutdown still must reap our immediate child.
                # Preserve the failure; do not ignore temporary-file errors.
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--port',type=int,default=17433);args=parser.parse_args()
    binary=pathlib.Path(args.binary).resolve()
    if not binary.is_file(): raise SystemExit('Build the backend before running this integration test.')
    with tempfile.TemporaryDirectory(prefix='phaseforge-smoke-') as folder:
        root=pathlib.Path(folder)
        config=root/'test.toml';config.write_text('bind_address = "127.0.0.1"\nport = '+str(args.port)+'\ndata_directory = '+json.dumps(str(root/'data'))+'\ngpu_enabled = false\n',encoding='utf-8')
        base=f'http://127.0.0.1:{args.port}'
        def request(path,payload=None):
            body=json.dumps(payload).encode() if payload is not None else None
            req=urllib.request.Request(base+path,data=body,headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
        with backend_process([str(binary),'--cpu-only','--config',str(config)], root/'backend.log') as process:
            for _ in range(90):
                try:
                    if request('/api/health')['status']=='ok':break
                except Exception:
                    if process.poll() is not None:raise RuntimeError('Backend exited: '+(root/'backend.log').read_text())
                    time.sleep(.2)
            else:raise RuntimeError('Backend did not become healthy')
            project=request('/api/projects',{'question':'Does the bounded evidence recorder retain actual measured differences?','name':'Integration test only'})
            model=draft();model['visualization'].update(x='x1',y='y1',z='',point_size=.03)
            model['model']={'kind':'state_vector_ode','variables':[{'name':n,'unit':'','initial':v} for n,v in [('x1',0),('y1',0),('x2',2),('y2',3)]],
                'derivatives':[{'variable':n,'expression':'0'} for n in ['x1','y1','x2','y2']]}
            model['observables']=[{'name':'measurement','expression':'x1','unit':'model length'}]
            model['constraints']=[{'name':'bound','expression':'x1','tolerance':1}]
            model['falsification']=[{'name':'perturb','kind':'initial_perturbation','target':'x1','magnitude':.001,'repetitions':3,'checks':[]}]
            imported=request(f"/api/projects/{project['id']}/manifests",{'manifest':model,'auto_run':False})
            run=request('/api/manifests/'+imported['manifest']['id']+'/run',{})
            for _ in range(300):
                run=request('/api/runs/'+run['id'])
                if run['status'] not in ['queued','running']:break
                time.sleep(.1)
            assert run['status']=='completed',run.get('error')
            result=run['result'];assert result['evidence_version']==2
            assert result['constraint_results'][0]['status']=='passed'
            test=result['falsification'][0];assert test['status']=='inconclusive' and test['survived'] is None
            assert len(test['trials'])==3 and len({t['perturbation'] for t in test['trials']})==3
            assert test['trials'][0]['metrics']['measurement']>result['metrics']['measurement']
            assert test['comparisons'][0]['relative_change'] is None
            assert len(result['visualization']['frames'][0]['entities'])==2
            assert result['numerical']['reached_end_time'] is True
            brief=request('/api/runs/'+run['id']+'/findings')
            assert brief['verdict']=='inconclusive' and brief['novelty']=='not_assessed'
            assert brief['manifest_id']==imported['manifest']['id'] and not brief.get('ai_analysis')
            flow=request('/api/workflow');assert all('result' not in row for row in flow['runs'])
            time.sleep(2.2);telemetry=request('/api/telemetry');assert 'ram_total_bytes' in telemetry
            assert isinstance(telemetry['gpus'],list)
            print('PASS actual backend API: isolated run, no-objective metric evidence, 3 distinct trials, constraints, multi-entity frames, findings, lightweight workflow and resource schema.')
if __name__=='__main__':main()
