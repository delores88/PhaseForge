"""Native backend integration acceptance; requires Cargo-built binary. No AI/network.
This is NOT executed in the packaging container. Windows/Linux CI executes it.
"""
import argparse,hashlib,io,json,os,pathlib,subprocess,tempfile,time,urllib.request,urllib.error,zipfile
from test_proposal_schema import draft

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--port',type=int,default=17434);args=p.parse_args()
    binary=pathlib.Path(args.binary).resolve()
    if not binary.is_file():raise SystemExit('Build the backend first; this is a native integration test, not a source check.')
    with tempfile.TemporaryDirectory(prefix='phaseforge-discovery-') as folder:
        root=pathlib.Path(folder);config=root/'isolated.toml';config.write_text('bind_address = "127.0.0.1"\nport = '+str(args.port)+'\ndata_directory = '+json.dumps(str(root/'data'))+'\ngpu_enabled = false\n',encoding='utf-8')
        log=(root/'backend.log').open('w');proc=subprocess.Popen([str(binary),'--cpu-only','--config',str(config)],stdout=log,stderr=subprocess.STDOUT)
        def call(path,payload=None,method=None,raw=False):
            req=urllib.request.Request(f'http://127.0.0.1:{args.port}'+path,data=json.dumps(payload).encode() if payload is not None else None,method=method,headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=30) as response:return response.read() if raw else json.load(response)
        try:
            for _ in range(100):
                try:
                    if call('/api/health')['status']=='ok':break
                except Exception:
                    if proc.poll() is not None:raise RuntimeError((root/'backend.log').read_text())
                    time.sleep(.2)
            else:raise RuntimeError('Backend did not start')
            world=call('/api/projects',{'question':'Can a finite discovery campaign retain reproducible evidence?','name':'Discovery CI only'})
            m=draft();m['model']['variables'][0]['initial']=1;m['model']['derivatives'][0]['expression']='-x';m['integration']['max_steps']=101
            imported=call(f"/api/projects/{world['id']}/manifests",{'manifest':m,'auto_run':False})['manifest']
            recipe={'title':'Bounded CI campaign','hypothesis':'No scientific claim; contract test only','base_manifest_id':imported['id'],'strategy':'map_elites',
                'parameters':[{'name':'x0','target':'initial:x','minimum':.5,'maximum':1.5}],
                'objectives':[{'metric':'state','goal':'minimize'}],'descriptors':[{'metric':'state','minimum':0,'maximum':1,'bins':4}],
                'exploration_trials':4,'validation_finalists':1,'wall_seconds':60,'per_trial_seconds':10,'seed':7,'absolute_tolerance':1e-6,'relative_tolerance':.001,'auto_review':False}
            study=call('/api/discovery/studies',recipe)['study'];sid=study['id'];assert study['state']=='draft' and study['trials']==[]
            call(f'/api/discovery/studies/{sid}/start',{})
            for _ in range(600):
                detail=call(f'/api/discovery/studies/{sid}');study=detail['study']
                if study['state'] not in ('running','pausing'):break
                time.sleep(.2)
            assert study['state']=='completed',study
            assert len(study['trials'])==6,study['trials'];assert detail['summary']['eligible']==4
            assert detail['summary']['validation'][0]['status']=='agreement',detail
            runid=study['trials'][0]['run_id'];assert call('/api/runs/'+runid+'/signals')['series']
            notebook=call('/api/research/notebooks/'+world['id'])['notebook'];old=json.loads(json.dumps(notebook))
            notebook['notes']='Actual integration test record';saved=call('/api/research/notebooks/'+world['id'],notebook,'PUT')['notebook'];assert saved['revision']==1
            try:call('/api/research/notebooks/'+world['id'],old,'PUT');raise AssertionError('Stale edit accepted')
            except urllib.error.HTTPError as e:assert e.code==409
            raw=call(f'/api/discovery/studies/{sid}/export',raw=True)
            with zipfile.ZipFile(io.BytesIO(raw)) as z:
                assert z.testzip() is None;checksums=json.loads(z.read('checksums.json'))
                for name,wanted in checksums.items():assert hashlib.sha256(z.read(name)).hexdigest()==wanted,name
                assert 'manuscript.md' in z.namelist();assert 'reference_ode.py' in z.namelist()
                assert len([n for n in z.namelist() if n.startswith('runs/')])==6
                out=root/'bundle';z.extractall(out)
            result=subprocess.run([os.sys.executable,str(root/'bundle/verify_bundle.py'),str(root/'bundle')],capture_output=True,text=True);assert result.returncode==0,result.stderr+result.stdout
            first=study['trials'][0]
            replay=subprocess.run([os.sys.executable,str(root/'bundle/reference_ode.py'),str(root/'bundle/manifests'/f"{first['manifest_id']}.json"),'--run',str(root/'bundle/runs'/f"{runid}.json")],capture_output=True,text=True)
            assert replay.returncode==0,replay.stderr+replay.stdout
            # No budget was silently consumed by AI.
            assert call('/api/usage')['totals']['total_tokens']==0
            print('PASS actual native campaign: 4 exploration + 2 refinement runs, archive, metrics, record CAS, ZIP CRC, all payload hashes, independent ODE replay, zero paid calls.')
        finally:
            proc.terminate()
            try:proc.wait(timeout=10)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
            log.close()
if __name__=='__main__':main()
