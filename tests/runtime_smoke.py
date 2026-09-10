"""Real local-backend smoke test. Requires a successfully compiled PhaseForge binary.
No AI calls, credentials, or internet. Uses an isolated temporary DB and port 17433.
Run: python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend[.exe]
"""
import argparse, json, os, pathlib, subprocess, tempfile, time, urllib.request
from test_proposal_schema import draft

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--port',type=int,default=17433);args=parser.parse_args()
    binary=pathlib.Path(args.binary).resolve()
    if not binary.is_file(): raise SystemExit('Build the backend before running this integration test.')
    with tempfile.TemporaryDirectory(prefix='phaseforge-smoke-') as folder:
        root=pathlib.Path(folder)
        config=root/'test.toml';config.write_text('bind_address = "127.0.0.1"\nport = '+str(args.port)+'\ndata_directory = '+json.dumps(str(root/'data'))+'\ngpu_enabled = false\n',encoding='utf-8')
        log=(root/'backend.log').open('w')
        process=subprocess.Popen([str(binary),'--cpu-only','--config',str(config)],stdout=log,stderr=subprocess.STDOUT)
        base=f'http://127.0.0.1:{args.port}'
        def request(path,payload=None):
            body=json.dumps(payload).encode() if payload is not None else None
            req=urllib.request.Request(base+path,data=body,headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
        try:
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
        finally:
            process.terminate()
            try:process.wait(timeout=10)
            except subprocess.TimeoutExpired:process.kill();process.wait()
            log.close()
if __name__=='__main__':main()
